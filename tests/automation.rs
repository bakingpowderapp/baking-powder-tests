use bp::arp::Arp;
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::fx::{Kind, Slot};
use bp::project::{Aim, Clip, Curve, Node, Project, Row, TICKS_PER_BEAT};
use support::{BLOCK, CHORD, CORE_ROW, FRAMES_PER_BEAT, SAMPLE_RATE, core_layer, first_frame_apart, midi, peak_of, worst_delta};

const BAR_BEATS: f64 = 4.0;
const FLAT_TRAVEL: f32 = 0.46;
const TOLERANCE: f32 = 1e-4;

fn curve(row: usize, band: &str, param: &str, nodes: &[(i32, f32)]) -> Curve {
    let nodes = nodes.iter().map(|&(beat, travel)| Node { tick: beat * TICKS_PER_BEAT, travel, bend: 0.0 }).collect();
    Curve { aim: Aim::knob(row, band, param), lfo: None, nodes }
}

fn project(rack: Vec<Row>, curves: Vec<Curve>) -> Project {
    let clip = Clip::chord(0, String::new(), &CHORD);
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = rack;
    project.tracks[0].automation.curves = curves;
    project.tracks[0].clips.push(clip);
    project
}

fn play(project: &Project) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, None, &[], &[]);
    audio.play(0.0);
    let total = (BAR_BEATS * FRAMES_PER_BEAT) as usize;
    let mut out = Vec::with_capacity(total + BLOCK);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    while out.len() < total {
        renderer.render(&mut l, &mut r);
        out.extend_from_slice(&l);
    }
    out
}

#[test]
fn a_flat_curve_matches_the_knob_once_settled() {
    let mut patch = Params::DEFAULT;
    patch.filter.on = true;
    let mut knob_patch = patch;
    assert!(knob_patch.set_travel("FILTER", "CUTOFF", f64::from(FLAT_TRAVEL)), "the filter cutoff is automatable");
    let mut knob_strip = Slot::new(Kind::Utility);
    knob_strip.params.set_travel("STRIP", "GAIN", f64::from(FLAT_TRAVEL));
    let knobs = play(&project(vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, knob_patch)), Row::Effect(knob_strip)], Vec::new()));
    let curves = play(&project(
        vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch)), Row::Effect(Slot::new(Kind::Utility))],
        vec![curve(CORE_ROW, "FILTER", "CUTOFF", &[(0, FLAT_TRAVEL)]), curve(CORE_ROW + 1, "STRIP", "GAIN", &[(0, FLAT_TRAVEL)])],
    ));
    assert!(peak_of(&knobs) > 0.05, "the chord is silent");
    let from = FRAMES_PER_BEAT as usize;
    let worst = worst_delta(&knobs[from..], &curves[from..]);
    assert!(worst < TOLERANCE, "wanted flat curves at travel {FLAT_TRAVEL} within {TOLERANCE} of the knobs after the first beat, worst delta {worst}");
}

#[test]
fn a_level_or_wet_lane_left_on_a_row_of_the_other_kind_moves_nothing() {
    let rack = || vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, Params::DEFAULT)), Row::Effect(Slot::new(Kind::Delay))];
    let silent = |aim: Aim| Curve { aim, lfo: None, nodes: vec![Node { tick: 0, travel: 0.0, bend: 0.0 }] };
    let plain = play(&project(rack(), Vec::new()));
    assert!(peak_of(&plain) > 0.05, "the chord is silent");
    for (stale, what) in [(Aim::Wet(CORE_ROW), "a wet lane on a row that is a Core"), (Aim::Level(CORE_ROW + 1), "a level lane on a row that is an effect")] {
        let lane = play(&project(rack(), vec![silent(stale)]));
        let apart = first_frame_apart(&lane, &plain);
        assert!(apart.is_none(), "wanted {what} to move nothing, the song playing as it does with no lane, but they part at frame {apart:?}");
    }
    let driven = play(&project(rack(), vec![silent(Aim::Level(CORE_ROW))]));
    assert!(first_frame_apart(&driven, &plain).is_some(), "wanted a level lane on the Core itself still to move its level");
}
