use bp::face::Param;
use bp::app::{App, Event, Request};
use bp::arp::{Arp, Articulation, Join, Lane, MidiRow, Step};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::division::Division;
use bp::factory::PRESETS;
use bp::fx::params;
use bp::fx::{self, Kind, Slot};
use bp::json::{self, Json};
use bp::project::{Aim, Clip, Curve, GAIN, LaneLfo, Layer, LfoField, Node, Note, Preset, Project, Row, TICKS_PER_BEAT, VELOCITY, VOL};
use bp::theory::ChordVariation;
use support::{BLOCK, BLOCKS, CORE_ROW, HEIGHT, LEVEL, RATE, SAMPLE_RATE, WIDTH, core, core_layer, effect, first_difference, midi, wav};

const KICK: &str = "/x/kick.wav";
const FILE_CELL_CENTRE: (i32, i32) = (60, 26);
const EXPORT_ITEM_CENTRE: (i32, i32) = (60, 325);
const WITHOUT_BOTH_ITEM_CENTRE: (i32, i32) = (404, 389);
const CUTOFF_LOW_HZ: f64 = 5.0;
const CUTOFF_SPAN: f64 = 4000.0;
const DECAY_LOW_S: f64 = 0.03;
const DECAY_SPAN: f64 = 2000.0;
const TOP_GAIN: f64 = 1.995_262_314_97;
const RANGE_TOP_TRAVEL: f64 = 0.95;
const RANGE_SPAN_DB: f64 = 24.0;
const POSITIONS: [f32; 11] = [0.0, 0.001, 0.02, 0.04, 0.2, 0.5, 0.9, 0.96, 0.97, 0.999, 1.0];
const CENTRES: [f64; 5] = [0.02, 0.3, 0.486_666_666_667, 0.5, 0.97];

const STATED: [(Kind, &[&str]); 7] = [
    (Kind::Reverb, &["preDelayMs", "decayS", "sizeTimes"]),
    (Kind::Delay, &["mode", "feedback"]),
    (Kind::Compressor, &["thresholdDb", "ratio", "attackMs", "releaseMs"]),
    (Kind::Sidechain, &["thresholdDb", "ratio", "attackMs", "releaseMs"]),
    (Kind::Transient, &["attackDb", "sustainDb"]),
    (Kind::Utility, &["ceilingDb"]),
    (Kind::VolumeLfo, &["points"]),
];

const ARP_TABLE: [(&str, &str); 4] = [("ARP", "TIME"), ("ARP", "GATE"), ("ARP", "SWING"), ("ARP", "DISTANCE")];

const HELD: [(Kind, &str, &str); 7] = [
    (Kind::Utility, "LIMIT", "LOOKAHEAD"),
    (Kind::Utility, "LIMIT", "LIMIT"),
    (Kind::Utility, "LIMIT", "HARD CLIP"),
    (Kind::Freak, "BANDS", "SLOPE"),
    (Kind::Motion, "BANDS", "SLOPE"),
    (Kind::Suppressor, "BANDS", "SLOPE"),
    (Kind::Reverb, "SPACE", "PLATEAU COPIES"),
];

fn named<P>(params: &[Param<P>]) -> Vec<(&'static str, &'static str)> {
    params.iter().map(|param| (param.band, param.name)).collect()
}

fn held_by(kind: Option<Kind>) -> Vec<(&'static str, &'static str)> {
    HELD.iter().filter(|(held, _, _)| Some(*held) == kind).map(|(_, band, name)| (*band, *name)).collect()
}

fn song(rack: Vec<Row>) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = rack;
    project.tracks[0].clips.push(Clip { start: 0, length: 8 * TICKS_PER_BEAT, label: String::new(), notes: vec![Note { pitch: 60, start: 0.0, length: 8.0, velocity: VELOCITY, articulation: Articulation::PLAIN }], file: None });
    project
}

fn filtered() -> Row {
    let mut patch = Params::DEFAULT;
    patch.filter.on = true;
    Row::Core(core_layer(LEVEL, patch))
}

fn lane(aim: Aim, travels: &[f32]) -> Curve {
    let nodes = travels.iter().enumerate().map(|(at, travel)| Node { tick: at as i32 * 2 * TICKS_PER_BEAT, travel: *travel, bend: 0.0 }).collect();
    Curve { aim, lfo: None, nodes }
}

fn parsed(text: &str) -> Json {
    json::parse(text).unwrap_or_else(|| panic!("wanted valid JSON, got {text}"))
}

fn opened(text: &str) -> Project {
    Project::from_json(&parsed(text)).unwrap_or_else(|| panic!("wanted a project to open from {text}"))
}

fn reopened(project: &Project) -> Project {
    opened(&project.to_json().write())
}

fn lanes(project: &Project, key: &str) -> Vec<Json> {
    let json = project.to_json();
    json.get("tracks").and_then(Json::as_array).and_then(|tracks| tracks[0].get(key)).and_then(Json::as_array).map(<[Json]>::to_vec).unwrap_or_default()
}

fn inside<'a>(json: &'a Json, key: &str) -> &'a Json {
    json.get(key).unwrap_or_else(|| panic!("wanted {key} in {}", json.write()))
}

fn number(json: &Json, key: &str) -> f64 {
    inside(json, key).as_f64().unwrap_or_else(|| panic!("wanted a number under {key} in {}", json.write()))
}

fn word<'a>(json: &'a Json, key: &str) -> &'a str {
    inside(json, key).as_str().unwrap_or_else(|| panic!("wanted a word under {key} in {}", json.write()))
}

fn values(lane: &Json) -> Vec<f64> {
    inside(lane, "nodes").as_array().unwrap_or(&[]).iter().map(|node| number(node, "value")).collect()
}

fn close(got: &[f64], wanted: &[f64]) -> bool {
    got.len() == wanted.len() && got.iter().zip(wanted).all(|(got, wanted)| (got - wanted).abs() <= wanted.abs() * 1e-9 + 1e-12)
}

fn travels(curve: &Curve) -> Vec<f64> {
    curve.nodes.iter().map(|node| f64::from(node.travel)).collect()
}

fn near(got: &[f64], wanted: &[f64]) -> bool {
    got.len() == wanted.len() && got.iter().zip(wanted).all(|(got, wanted)| (got - wanted).abs() < 1e-6)
}

fn run(project: &Project) -> Vec<Vec<f32>> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, None, &[], &[]);
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(BLOCKS);
    for _ in 0..BLOCKS {
        renderer.render(&mut l, &mut r);
        out.push(l.clone());
    }
    out
}

#[test]
fn a_cutoff_lane_is_written_in_hertz_under_the_filters_own_key() {
    let mut project = song(vec![midi(Arp::DEFAULT), filtered()]);
    project.tracks[0].automation.curves.push(Curve { nodes: vec![Node { tick: 0, travel: 0.25, bend: 0.0 }, Node { tick: 16 * TICKS_PER_BEAT, travel: 0.75, bend: 0.5 }], ..lane(Aim::knob(CORE_ROW, "FILTER", "CUTOFF"), &[]) });
    let spelled = lanes(&project, "curves");
    assert_eq!(spelled.len(), 1, "wanted the one lane written, got {}", spelled.len());
    assert_eq!(word(&spelled[0], "param"), "filter.cutoffHz", "wanted the lane to name the key its row writes");
    assert!(spelled[0].get("band").is_none(), "wanted no face word beside the key, got {}", spelled[0].write());
    let wanted = [CUTOFF_LOW_HZ * CUTOFF_SPAN.powf(0.25), CUTOFF_LOW_HZ * CUTOFF_SPAN.powf(0.75)];
    assert!(close(&values(&spelled[0]), &wanted), "wanted the nodes at {wanted:?} Hz, got {:?}", values(&spelled[0]));
    let nodes = inside(&spelled[0], "nodes").as_array().expect("nodes");
    assert!(nodes.iter().all(|node| node.get("travel").is_none()), "wanted no travel left in a node, got {}", spelled[0].write());
    assert_eq!(number(&nodes[1], "bend"), 0.5, "wanted the bend kept beside the value");
    assert_eq!(reopened(&project).tracks[0].automation.curves, project.tracks[0].automation.curves, "the cutoff lane changed in the save");
}

#[test]
fn track_volume_is_a_gain_lane_with_no_row() {
    let mut project = song(vec![midi(Arp::DEFAULT), core(LEVEL)]);
    project.tracks[0].automation.curves.push(lane(Aim::Gain, &[0.75, 0.375, 0.0, 1.0]));
    let spelled = lanes(&project, "curves");
    assert_eq!(word(&spelled[0], "param"), "gain", "wanted the track's own gain key");
    assert!(spelled[0].get("row").is_none(), "wanted no row on a track volume lane, got {}", spelled[0].write());
    let wanted = [1.0, 0.5, 0.0, TOP_GAIN];
    assert!(close(&values(&spelled[0]), &wanted), "wanted unity at the fader's three-quarter mark and gains of {wanted:?}, got {:?}", values(&spelled[0]));
    assert_eq!(reopened(&project).tracks[0].automation.curves, project.tracks[0].automation.curves, "the volume lane changed in the save");
}

#[test]
fn a_rows_mix_lane_is_named_for_its_level_or_wet() {
    let mut project = song(vec![midi(Arp::DEFAULT), core(LEVEL), effect(Kind::Reverb)]);
    project.tracks[0].automation.curves.push(lane(Aim::Level(CORE_ROW), &[0.3, 1.0]));
    project.tracks[0].automation.curves.push(lane(Aim::Wet(CORE_ROW + 1), &[0.6]));
    let spelled = lanes(&project, "curves");
    assert_eq!((word(&spelled[0], "param"), word(&spelled[1], "param")), ("level", "wet"), "wanted each mix lane under its row's own mix key");
    assert_eq!((values(&spelled[0]), values(&spelled[1])), (vec![0.3, 1.0], vec![0.6]), "wanted the mix values as the row would write them");
    assert_eq!(reopened(&project).tracks[0].automation.curves, project.tracks[0].automation.curves, "a mix lane changed in the save");
}

#[test]
fn a_lane_lfo_states_its_centre_in_the_knobs_units() {
    let mut project = song(vec![midi(Arp::DEFAULT), filtered()]);
    project.tracks[0].automation.lfos.push(LaneLfo { amount: 0.4, sync: false, rate_hz: 2.0, ..LaneLfo::blank(Aim::knob(CORE_ROW, "FILTER", "CUTOFF")) });
    project.tracks[0].automation.curves.push(Curve { lfo: Some(LfoField::Center), ..lane(Aim::knob(CORE_ROW, "FILTER", "CUTOFF"), &[0.25]) });
    project.tracks[0].automation.curves.push(Curve { lfo: Some(LfoField::Amount), ..lane(Aim::knob(CORE_ROW, "FILTER", "CUTOFF"), &[0.6]) });
    let lfo = &lanes(&project, "lfos")[0];
    assert_eq!(word(lfo, "param"), "filter.cutoffHz", "wanted the LFO to name the key it waves");
    let centre = CUTOFF_LOW_HZ * CUTOFF_SPAN.powf(0.5);
    assert!(close(&[number(lfo, "center")], &[centre]), "wanted the centre at {centre} Hz, got {}", number(lfo, "center"));
    assert_eq!(number(lfo, "amount"), 0.4, "wanted the amount left a share of the knob's travel");
    let curves = lanes(&project, "curves");
    assert_eq!((word(&curves[0], "lfoParam"), word(&curves[1], "lfoParam")), ("center", "amount"), "wanted each LFO lane to name the LFO's own key");
    assert!(close(&values(&curves[0]), &[CUTOFF_LOW_HZ * CUTOFF_SPAN.powf(0.25)]), "wanted the centre lane in Hz, got {:?}", values(&curves[0]));
    assert_eq!(values(&curves[1]), vec![0.6], "wanted the amount lane as a share");
    let back = reopened(&project);
    assert_eq!(back.tracks[0].automation.lfos, project.tracks[0].automation.lfos, "the LFO changed in the save");
    assert_eq!(back.tracks[0].automation.curves, project.tracks[0].automation.curves, "an LFO lane changed in the save");
}

#[test]
fn a_hand_written_lane_reaches_a_knob_left_at_its_default() {
    let project = opened(
        r#"{"kind": "project", "tracks": [{"kind": "midi", "rack": [{"kind": "core"}, {"kind": "reverb"}],
        "curves": [
            {"row": 0, "param": "filter.cutoffHz", "nodes": [{"beat": 0, "value": 200}, {"beat": 16, "value": 4000, "bend": 0.25}]},
            {"row": 1, "param": "decayS", "nodes": [{"beat": 0, "value": 1.6}, {"beat": 32, "value": 6}]},
            {"param": "gain", "nodes": [{"beat": 56, "value": 1}, {"beat": 64, "value": 0}]}
        ],
        "lfos": [{"row": 0, "param": "noise.tone", "amount": 0.4, "center": 0, "tempoSync": true, "division": "1/1"}]}]}"#,
    );
    let curves = &project.tracks[0].automation.curves;
    let aims: Vec<(&str, &str)> = curves.iter().map(|curve| curve.aim.words()).collect();
    assert_eq!(aims, [("FILTER", "CUTOFF"), ("SPACE", "DECAY"), (VOL, GAIN)], "wanted each lane on the knob its key names, got {aims:?}");
    let cutoff = [(200.0 / CUTOFF_LOW_HZ).ln() / CUTOFF_SPAN.ln(), (4000.0 / CUTOFF_LOW_HZ).ln() / CUTOFF_SPAN.ln()];
    assert!(near(&travels(&curves[0]), &cutoff), "wanted 200 Hz and 4 kHz at travels {cutoff:?}, got {:?}", travels(&curves[0]));
    assert_eq!(curves[0].nodes.iter().map(|node| node.bend).collect::<Vec<f32>>(), [0.0, 0.25], "wanted a bend left out to be straight and one given to be kept");
    let decay = [(1.6 / DECAY_LOW_S).ln() / DECAY_SPAN.ln(), (6.0 / DECAY_LOW_S).ln() / DECAY_SPAN.ln()];
    assert!(near(&travels(&curves[1]), &decay), "wanted 1.6 s and 6 s at travels {decay:?}, got {:?}", travels(&curves[1]));
    assert!(near(&travels(&curves[2]), &[0.75, 0.0]), "wanted unity gain at the fader's three-quarter mark, got {:?}", travels(&curves[2]));
    let lfos = &project.tracks[0].automation.lfos;
    assert_eq!(lfos.len(), 1, "wanted the tone LFO kept, got {}", lfos.len());
    let (band, name) = lfos[0].aim.words();
    assert_eq!((band, name, lfos[0].centre, lfos[0].amount, lfos[0].division), ("NOISE", "TONE", 0.5, 0.4, Division::Bar), "wanted the LFO centred on the tone knob's middle");
}

#[test]
fn a_lane_aimed_at_nothing_is_dropped() {
    let project = opened(
        r#"{"kind": "project", "tracks": [{"kind": "midi", "rack": [{"kind": "core"}],
        "curves": [
            {"row": 0, "param": "filter.cutoff", "nodes": [{"beat": 0, "value": 200}]},
            {"row": 3, "param": "decayS", "nodes": [{"beat": 0, "value": 2}]},
            {"row": 0, "band": "FILTER", "param": "CUTOFF", "nodes": [{"beat": 0, "travel": 0.5}]},
            {"row": 0, "param": "filter.cutoffHz", "lfoParam": "depth", "nodes": [{"beat": 0, "value": 0.5}]}
        ],
        "lfos": [{"row": 0, "param": "REVERB", "amount": 0.4}]}]}"#,
    );
    assert!(project.tracks[0].automation.curves.is_empty(), "wanted every lane with no knob to land on dropped, got {:?}", project.tracks[0].automation.curves);
    assert!(project.tracks[0].automation.lfos.is_empty(), "wanted the LFO with no knob dropped, got {:?}", project.tracks[0].automation.lfos);
    let mut stale = song(vec![midi(Arp::DEFAULT), core(LEVEL)]);
    stale.tracks[0].automation.curves.push(lane(Aim::knob(4, "SPACE", "DECAY"), &[0.5]));
    assert!(lanes(&stale, "curves").is_empty(), "wanted a lane whose row is gone left out of the save, got {:?}", lanes(&stale, "curves").iter().map(Json::write).collect::<Vec<String>>());
}

#[test]
fn every_knob_has_a_key_and_keeps_its_lanes_through_a_save() {
    let table_of = |kind: Kind| match kind {
        Kind::Compressor => named(params::compressor::PARAMS),
        Kind::Delay => named(params::delay::PARAMS),
        Kind::Eq => named(params::eq::PARAMS),
        Kind::Freak => named(params::freak::PARAMS),
        Kind::Gate => named(params::gate::PARAMS),
        Kind::Motion => named(params::motion::PARAMS),
        Kind::Repeat => named(params::repeat::PARAMS),
        Kind::Reverb => named(params::reverb::PARAMS),
        Kind::Sidechain => named(params::sidechain::PARAMS),
        Kind::Space => named(params::space::PARAMS),
        Kind::Suppressor => named(params::suppressor::PARAMS),
        Kind::Time => named(params::time::PARAMS),
        Kind::Transient => named(params::transient::PARAMS),
        Kind::Utility => named(params::utility::PARAMS),
        Kind::VolumeLfo => named(params::volume_lfo::PARAMS),
    };
    let rows = [midi(Arp::DEFAULT), core(LEVEL)].into_iter().chain(Kind::ALL.into_iter().map(effect));
    for row in rows {
        let (listed, kind) = match row {
            Row::Midi(_) => (ARP_TABLE.to_vec(), None),
            Row::Bus(_) => (Vec::new(), None),
            Row::Core(_) => (named(bp::core::params::PARAMS), None),
            Row::Effect(slot) => (table_of(slot.kind()), Some(slot.kind())),
        };
        let held = held_by(kind);
        let table: Vec<(&str, &str)> = listed.into_iter().filter(|control| !held.contains(control)).collect();
        let knobs = row.knobs();
        for (band, name) in &held {
            assert!(!knobs.iter().any(|knob| (knob.band, knob.name) == (*band, *name)), "{}: wanted {band} {name}, which moves the row's latency, held out of its knobs", row.label());
            assert!(!row.automatable(band, name), "{}: wanted {band} {name}, which moves the row's latency, refused as a lane target", row.label());
        }
        let mut paths: Vec<&str> = knobs.iter().map(|knob| knob.path.as_str()).collect();
        paths.sort_unstable();
        paths.dedup();
        assert_eq!(paths.len(), knobs.len(), "{}: wanted each of its {} knobs under a key of its own, got {} keys", row.label(), knobs.len(), paths.len());
        for (band, name) in &table {
            assert!(knobs.iter().any(|knob| (knob.band, knob.name) == (*band, *name)), "{}: wanted the table's {band} {name} among its knobs", row.label());
        }
        assert!(knobs.len() >= table.len(), "{}: wanted every table row and on or off cell among its knobs, got {} for {} rows", row.label(), knobs.len(), table.len());
        let (rack, at) = match row {
            Row::Midi(_) => (vec![row, core(LEVEL)], 0),
            Row::Core(_) => (vec![midi(Arp::DEFAULT), row], CORE_ROW),
            Row::Effect(_) | Row::Bus(_) => (vec![midi(Arp::DEFAULT), core(LEVEL), row], CORE_ROW + 1),
        };
        for knob in knobs {
            for centre in CENTRES {
                let mut project = song(rack.clone());
                project.tracks[0].automation.curves.push(lane(Aim::knob(at, knob.band, knob.name), &POSITIONS));
                project.tracks[0].automation.lfos.push(LaneLfo { amount: 0.02, centre, ..LaneLfo::blank(Aim::knob(at, knob.band, knob.name)) });
                let back = reopened(&project);
                assert_eq!(back.tracks[0].automation.curves, project.tracks[0].automation.curves, "{} {} {}: the lane under {} changed in the save", row.label(), knob.band, knob.name, knob.path);
                assert_eq!(back.tracks[0].automation.lfos, project.tracks[0].automation.lfos, "{} {} {}: the LFO centred at {centre} under {} changed in the save", row.label(), knob.band, knob.name, knob.path);
            }
        }
    }
}

#[test]
fn a_position_the_value_cannot_say_is_kept_beside_it() {
    let mut project = song(vec![midi(Arp::DEFAULT), core(LEVEL), effect(Kind::Compressor), effect(Kind::Freak)]);
    project.tracks[0].automation.curves.push(lane(Aim::knob(CORE_ROW + 1, "OUTPUT", "RANGE"), &[0.97, 0.5]));
    project.tracks[0].automation.curves.push(lane(Aim::knob(CORE_ROW + 2, "MASTER", "SUB CUT"), &[0.03, 0.5]));
    project.tracks[0].automation.lfos.push(LaneLfo { amount: 0.04, centre: 0.97, ..LaneLfo::blank(Aim::knob(CORE_ROW + 1, "OUTPUT", "RANGE")) });
    let curves = lanes(&project, "curves");
    let nodes = |lane: &Json| inside(lane, "nodes").as_array().unwrap_or(&[]).to_vec();
    let (range, sub_cut) = (nodes(&curves[0]), nodes(&curves[1]));
    assert_eq!((number(&range[0], "value"), number(&range[0], "travel")), (60.0, 0.97), "wanted a node on the range's flat top to say where on it it sits");
    assert_eq!((number(&sub_cut[0], "value"), number(&sub_cut[0], "travel")), (0.0, 0.03), "wanted a node in the sub cut's off zone to say where in it it sits");
    assert!(range[1].get("travel").is_none() && sub_cut[1].get("travel").is_none(), "wanted no travel beside a value that says the position, got {} and {}", range[1].write(), sub_cut[1].write());
    let lfo = &lanes(&project, "lfos")[0];
    assert_eq!((number(lfo, "center"), number(lfo, "centerTravel")), (60.0, 0.97), "wanted an LFO centred on the flat top to say where on it it sits");
    let back = reopened(&project);
    assert_eq!(back.tracks[0].automation.curves, project.tracks[0].automation.curves, "a lane on a flat range changed in the save");
    assert_eq!(back.tracks[0].automation.lfos, project.tracks[0].automation.lfos, "the LFO on a flat range changed in the save");
    assert_eq!(first_difference(&run(&back), &run(&project)), None, "the reopened project renders differently");
    let edited = opened(
        r#"{"kind": "project", "tracks": [{"kind": "midi", "rack": [{"kind": "core"}, {"kind": "compressor"}],
        "curves": [{"row": 1, "param": "rangeDb", "nodes": [{"beat": 0, "value": 12, "travel": 0.97}]}]}]}"#,
    );
    let wanted = [RANGE_TOP_TRAVEL * 12f64.ln() / RANGE_SPAN_DB.ln()];
    assert!(near(&travels(&edited.tracks[0].automation.curves[0]), &wanted), "wanted a value edited by hand to win over the travel left beside it, at {wanted:?}, got {:?}", travels(&edited.tracks[0].automation.curves[0]));
}

#[test]
fn a_reopened_project_sounds_as_it_did() {
    let mut project = song(vec![midi(Arp::DEFAULT), filtered(), effect(Kind::Reverb)]);
    let track = &mut project.tracks[0];
    track.automation.curves.push(Curve { nodes: vec![Node { tick: 0, travel: 0.2, bend: 0.5 }, Node { tick: 2 * TICKS_PER_BEAT, travel: 0.8, bend: -0.7 }, Node { tick: 4 * TICKS_PER_BEAT, travel: 0.3, bend: 0.0 }], ..lane(Aim::knob(CORE_ROW, "FILTER", "CUTOFF"), &[]) });
    track.automation.curves.push(lane(Aim::Level(CORE_ROW), &[0.9, 0.4]));
    track.automation.curves.push(lane(Aim::knob(CORE_ROW + 1, "SPACE", "DECAY"), &[0.3, 0.6]));
    track.automation.curves.push(lane(Aim::Gain, &[0.75, 0.6, 0.9]));
    track.automation.lfos.push(LaneLfo { amount: 0.5, sync: false, rate_hz: 3.0, ..LaneLfo::blank(Aim::knob(CORE_ROW, "NOISE", "TONE")) });
    track.automation.curves.push(Curve { lfo: Some(LfoField::Amount), ..lane(Aim::knob(CORE_ROW, "NOISE", "TONE"), &[0.5, 0.1]) });
    let back = reopened(&project);
    assert_eq!(back.tracks[0].automation.curves, project.tracks[0].automation.curves, "a lane changed in the save");
    assert_eq!(back.tracks[0].automation.lfos, project.tracks[0].automation.lfos, "the LFO changed in the save");
    assert_eq!(first_difference(&run(&back), &run(&project)), None, "the reopened project renders differently");
}

#[test]
fn an_arp_pattern_is_spelled_step_by_step_and_keeps_the_steps_past_its_length() {
    let mut arp = Arp::DEFAULT;
    for (index, note) in [1, 2, 4, 9, 2, 6].into_iter().enumerate() {
        arp.pattern.set(Lane::Note, index, note);
    }
    arp.pattern.set(Lane::Octave, 1, 1);
    arp.pattern.set_rest(2, true);
    arp.pattern.set_length(5);
    let row = midi(arp);
    let json = row.to_json();
    let pattern = inside(inside(&json, "arp"), "pattern");
    assert_eq!(number(pattern, "length"), 5.0, "wanted the active length written");
    let steps: Vec<String> = inside(pattern, "steps").as_array().unwrap_or(&[]).iter().map(|step| step.write_at(0)).collect();
    let wanted = [r#"{"fromLowest": 1}"#, r#"{"fromLowest": 2, "octaveShift": 1}"#, r#"{"fromLowest": 4, "rest": true}"#, r#"{"fromLowest": 9}"#, r#"{"fromLowest": 2}"#, r#"{"fromLowest": 6}"#];
    assert_eq!(steps, wanted, "wanted the five active steps and the sixth kept past the length");
    assert!(pattern.get("note").is_none() && pattern.get("rests").is_none(), "wanted no lane arrays left, got {}", pattern.write());
    assert_eq!(Row::from_json(&json), Some(row), "the pattern changed in the save");
    let plain = Row::Midi(MidiRow::DEFAULT).to_json();
    assert!(plain.get("arp").is_none(), "wanted a plain arp left out, got {}", plain.write());
}

#[test]
fn hand_written_steps_set_the_patterns_length() {
    let text = r#"{"kind": "midi", "arp": {"on": true, "pattern": {"steps": [{"fromLowest": 1}, {"followsStyle": true}, {"fromLowest": 12, "rest": true, "octaveShift": -1}]}}}"#;
    let Some(Row::Midi(row)) = Row::from_json(&parsed(text)) else {
        panic!("wanted a MIDI row from {text}");
    };
    let pattern = row.arp.pattern;
    assert_eq!(pattern.length(), 3, "wanted three steps written to mean a three-step pattern, got {}", pattern.length());
    let step = |note, octave, rest| Step { note, octave, rest, semitone: 0, cents: 0, velocity: Lane::Velocity.initial(), gate: Lane::Gate.initial(), modulation: Lane::Mod.initial(), join: Join::Play };
    let wanted = [step(1, 0, false), step(0, 0, false), step(12, -1, true)];
    assert_eq!([pattern.step(0), pattern.step(1), pattern.step(2)], wanted, "wanted each step read as it is spelled");
}

#[test]
fn a_core_row_at_its_defaults_writes_its_amp_envelope_but_not_its_oscillator() {
    let plain = Row::Core(Layer::DEFAULT).to_json();
    assert!(plain.get("osc1").is_none(), "wanted a Core at its defaults to leave its first oscillator unwritten, got {}", plain.write());
    assert_eq!(inside(&plain, "ampEnv").write_at(0), r#"{"attackMs": 5, "decayMs": 200, "sustain": 0.8, "releaseMs": 200}"#, "wanted the envelope's four stages written at their defaults");
    assert!(plain.get("osc2").is_none() && plain.get("filter").is_none(), "wanted the silent oscillator and the filter left out while untouched, got {}", plain.write());
    let mut patch = Params::DEFAULT;
    patch.osc1.levels[3] = 1.0;
    patch.osc2.level = 0.5;
    patch.filter.on = true;
    patch.filter.env.sustain = 0.1;
    let row = Row::Core(core_layer(1.0, patch));
    let json = row.to_json();
    assert_eq!(inside(&json, "osc1").write_at(0), "{\n  \"levels\": [1, 0, 0, 1]\n}", "wanted a touched oscillator to write only what differs from its defaults");
    assert_eq!(inside(&json, "osc2").write_at(0), r#"{"level": 0.5}"#, "wanted the second oscillator's level alone, its mix left at its defaults");
    let filter = inside(&json, "filter");
    assert_eq!((word(filter, "mode"), number(filter, "slopeDbPerOctave"), number(filter, "cutoffHz")), ("svf-low-pass", 24.0, 2000.0), "wanted an enabled filter to state its mode, slope and cutoff");
    assert_eq!(inside(filter, "env").write_at(0), r#"{"attackMs": 5, "decayMs": 300, "sustain": 0.1, "releaseMs": 200}"#, "wanted a touched filter envelope to state its four stages");
    assert_eq!(Row::from_json(&json), Some(row), "the Core row changed in the save");
}

#[test]
fn an_enabled_eq_band_writes_its_whole_self() {
    let mut slot = Slot::new(Kind::Eq);
    if let fx::Params::Eq(eq) = &mut slot.params {
        eq.bands[3].on = true;
    }
    let row = Row::Effect(slot);
    let json = row.to_json();
    let bands = inside(&json, "bands").as_array().expect("bands");
    assert_eq!(bands[3].write_at(0), r#"{"on": true, "shape": "bell", "channel": "stereo", "frequencyHz": 3100, "gainDb": -10, "q": 18}"#, "wanted the enabled band to state everything it does");
    assert_eq!(bands[0].write_at(0), "{}", "wanted a band left off and untouched to stay empty");
    assert_eq!(Row::from_json(&json), Some(row), "the EQ changed in the save");
}

#[test]
fn each_effect_states_the_values_that_define_it() {
    for (kind, keys) in STATED {
        let row = effect(kind);
        let json = row.to_json();
        let missing: Vec<&str> = keys.iter().copied().filter(|key| json.get(key).is_none()).collect();
        assert!(missing.is_empty(), "{}: wanted {keys:?} written at their defaults, missing {missing:?} in {}", row.label(), json.write());
        assert_eq!(Row::from_json(&json), Some(row), "{}: the default row changed in the save", row.label());
    }
}

#[test]
fn the_last_small_words_are_spelled_like_their_neighbours() {
    assert_eq!((ChordVariation::SixNine.name(), ChordVariation::SevenSus4.name()), ("six-nine", "seven-sus4"), "wanted the chord words in kebab case");
    let mut patch = Params::DEFAULT;
    patch.amp.velocity = 0.4;
    let row = Row::Core(core_layer(1.0, patch));
    let json = row.to_json();
    let amp = inside(&json, "ampEnv");
    assert_eq!(number(amp, "velocityAmount"), 0.4, "wanted the envelope's velocity share under its own word");
    assert!(amp.get("velocity").is_none(), "wanted no bare velocity on an envelope, got {}", amp.write());
    assert_eq!(Row::from_json(&json), Some(row), "the Core row changed in the save");
    let arped = midi(Arp { on: true, rate: Division::Sixteenth, ..Arp::DEFAULT });
    let spelled = arped.to_json();
    let arp = inside(&spelled, "arp");
    assert_eq!(word(arp, "division"), "1/16", "wanted the arp's synced step under division");
    assert!(arp.get("rate").is_none(), "wanted no rate on the arp, got {}", arp.write());
    assert_eq!(Row::from_json(&spelled), Some(arped), "the MIDI row changed in the save");
    let lfo = effect(Kind::VolumeLfo).to_json();
    let points = inside(&lfo, "points").as_array().expect("points");
    assert!(points.iter().all(|point| point.get("bend").is_some() && point.get("curve").is_none()), "wanted each point's bow under bend, got {}", lfo.write());
    let blended = Row::Effect(Slot::new(Kind::Reverb).blended(0.35, 0.65)).to_json();
    assert_eq!((number(&blended, "forwardLevel"), number(&blended, "reverseLevel")), (0.35, 0.65), "wanted the two blends named as levels");
}

#[test]
fn sixteen_empty_returns_print_on_one_line() {
    let text = Project::new().to_json().write();
    let returns = format!(r#""returns": [{}]"#, vec!["{}"; 16].join(", "));
    assert!(text.lines().any(|line| line.trim() == returns), "wanted sixteen empty returns on one line, got {text}");
}

#[test]
fn every_factory_preset_reads_back_as_it_was_written() {
    let preset = |name: &str| {
        let text = PRESETS.iter().find(|(known, _)| *known == name).unwrap_or_else(|| panic!("wanted {name} in the bank")).1;
        Preset::from_json(&parsed(text)).unwrap_or_else(|| panic!("wanted {name} to open"))
    };
    for (name, _) in PRESETS {
        let opened = preset(name);
        assert_eq!(Preset::from_json(&opened.to_json()), Some(opened), "{name} changed when saved again");
    }
}

#[test]
fn an_export_without_audio_points_at_wave_name() {
    let mut app = App::new();
    app.draw(WIDTH, HEIGHT);
    app.take_requests();
    app.dropped(String::from(KICK), bp::browser::Kind::Wave, WIDTH - 20, HEIGHT - 20);
    app.take_requests();
    let samples: Vec<i16> = (0..(RATE / 2) as i16).map(|frame| frame.wrapping_mul(37)).collect();
    app.read(String::from(KICK), Some(wav(&samples)));
    app.take_requests();
    for (x, y) in [FILE_CELL_CENTRE, EXPORT_ITEM_CENTRE, WITHOUT_BOTH_ITEM_CENTRE] {
        app.event(Event::Move { x, y });
        app.event(Event::Down { x, y, clicks: 1 });
        app.event(Event::Up { x, y });
        app.draw(WIDTH, HEIGHT);
    }
    let exported: Vec<String> = app.take_requests().into_iter().filter_map(|request| match request {
        Request::Preset { json, .. } => Some(json),
        _ => None,
    }).collect();
    assert_eq!(exported.len(), 1, "wanted one export after File, Export Project then Without Both, got {}", exported.len());
    let file = parsed(&exported[0]);
    let note = word(&file, "note");
    assert!(note.contains("wave.name") && note.contains("kick") && !note.contains("file.name"), "wanted the note to point at wave.name and name the wave, got {note}");
    let waves = inside(&file, "waves").as_array().expect("waves");
    assert_eq!(waves.len(), 1, "wanted the one wave listed, got {}", waves.len());
    assert!(waves[0].get("seconds").is_some() && waves[0].get("pcm16Base64").is_none(), "wanted the wave listed by its length with no samples, got {}", waves[0].write());
    assert!(file.get("versions").is_none(), "wanted no history in an export without both");
}
