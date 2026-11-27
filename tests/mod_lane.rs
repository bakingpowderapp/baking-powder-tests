use bp::app::App;
use bp::arp::{Arp, Lane, Pattern};
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::core::Params;
use bp::core::params::{Envelope, Modulation, Osc, Quantize};
use bp::json::{self, Json};
use bp::project::{Project, Row, VELOCITY};
use support::{AUDIBLE, BLOCK, CORE_ROW, Onsets, SAMPLE_RATE, WITHIN, amplitude, call, core_layer, first_frame_apart, midi, peak_of, unseeded};

const KEY: u8 = 69;
const KEY_HZ: f64 = 440.0;
const STEP_MS: f64 = 250.0;
const STEP: usize = 12_000;
const FULL: i8 = 100;
const SHORT_GATE: i8 = 10;
const SLICE: (usize, usize) = (2_400, 9_600);
const CLEAR: f64 = 10.0;
const SHUT_HZ: f64 = 110.0;
const FOUR_OCTAVES: f64 = 4.0;
const NEARLY_ALL_THE_WAY: f64 = 0.45;
const FAR_OVER: f32 = 100.0;
const NEW_DEPTHS: [(&str, &str); 3] = [("cutoffOctaves", "-5 to 5"), ("pan", "-1 to 1"), ("pulseWidth", "-0.45 to 0.45")];

fn patch() -> Params {
    let mut patch = Params::DEFAULT;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    patch
}

fn line(mods: &[i8], gate: i8) -> Pattern {
    let mut pattern = Pattern::DEFAULT;
    pattern.set_length(mods.len());
    for (index, value) in mods.iter().enumerate() {
        pattern.set(Lane::Mod, index, *value);
        pattern.set(Lane::Gate, index, gate);
    }
    pattern
}

fn stepped(pattern: Pattern) -> Arp {
    Arp { on: true, stepped: true, sync: false, time_ms: STEP_MS, pattern, ..Arp::DEFAULT }
}

fn project(arp: Arp, patch: Params) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(arp), Row::Core(core_layer(1.0, patch))];
    project
}

struct Session {
    audio: Audio,
    renderer: Renderer,
    left: Vec<f32>,
    right: Vec<f32>,
}

impl Session {
    fn open(project: &Project) -> Session {
        let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
        renderer.patient();
        renderer.start_workers(0, || None);
        audio.sync(project, None, &[], &[]);
        Session { audio, renderer, left: Vec::new(), right: Vec::new() }
    }

    fn render(&mut self, seconds: f64) {
        let end = self.left.len() + (seconds * SAMPLE_RATE).round() as usize;
        let (mut left, mut right) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
        while self.left.len() < end {
            let n = BLOCK.min(end - self.left.len());
            self.renderer.render(&mut left[..n], &mut right[..n]);
            self.left.extend_from_slice(&left[..n]);
            self.right.extend_from_slice(&right[..n]);
        }
    }

    fn onsets(&self) -> Vec<usize> {
        let loud: Vec<f32> = self.left.iter().zip(&self.right).map(|(l, r)| l.abs().max(r.abs())).collect();
        let mut onsets = Onsets::default();
        onsets.feed(&loud);
        onsets.at
    }
}

#[test]
fn a_step_at_minus_full_silences_an_oscillator_whose_level_mod_is_full() {
    let mut patch = patch();
    patch.modulation = Modulation { level1: 1.0, ..Modulation::DEFAULT };
    let mut session = Session::open(&project(stepped(line(&[0, -FULL, 0, -FULL], SHORT_GATE)), patch));
    session.audio.note_on(0, KEY, VELOCITY);
    session.render(1.1);
    let onsets = session.onsets();
    let wanted = [0, 2 * STEP, 4 * STEP];
    let landed = onsets.len() == wanted.len() && onsets.iter().zip(wanted).all(|(at, want)| at.abs_diff(want) <= WITHIN);
    assert!(landed, "wanted onsets on steps 1 and 3 alone at frames {wanted:?}, got {onsets:?}");
}

fn osc_2_alone() -> Params {
    let mut patch = patch();
    patch.osc1.level = 0.0;
    patch.osc2 = Osc { levels: [0.0, 0.0, 0.0, 1.0], level: 1.0, ..Osc::DEFAULT };
    patch.modulation = Modulation { tune2: 12.0, ..Modulation::DEFAULT };
    patch
}

fn pitched(modulation: i8) -> (f64, f64) {
    let mut session = Session::open(&project(stepped(line(&[modulation], FULL)), osc_2_alone()));
    session.audio.note_on(0, KEY, VELOCITY);
    session.render(SLICE.1 as f64 / SAMPLE_RATE);
    let slice = &session.left[SLICE.0..SLICE.1];
    (amplitude(slice, KEY_HZ), amplitude(slice, 2.0 * KEY_HZ))
}

#[test]
fn a_full_step_lifts_osc_2_an_octave_at_twelve_semitones_of_tune_mod_and_a_zero_step_leaves_the_key() {
    let (key, octave) = pitched(FULL);
    assert!(octave > f64::from(AUDIBLE) && octave > CLEAR * key, "wanted a +100 step sounding at {} Hz and not {KEY_HZ} Hz between frames {} and {}, got {octave:.4} at the octave against {key:.4} at the key", 2.0 * KEY_HZ, SLICE.0, SLICE.1);
    let (key, octave) = pitched(0);
    assert!(key > f64::from(AUDIBLE) && key > CLEAR * octave, "wanted a 0 step sounding at {KEY_HZ} Hz and not {} Hz between frames {} and {}, got {key:.4} at the key against {octave:.4} at the octave", 2.0 * KEY_HZ, SLICE.0, SLICE.1);
}

#[test]
fn mod_values_survive_a_save_and_a_plain_project_writes_none() {
    let mut patch = Params::DEFAULT;
    patch.modulation = Modulation { level1: -1.0, level2: 1.5, tune2: 7.0, tune3: -5.0, tune4: 12.0, cutoff: -2.5, pan: 0.5, pulse_width: 0.3, sample: -0.6, sampled: true, quantize: Quantize::MinorChord, lag_ms: 20.0 };
    let project = project(stepped(line(&[37, -FULL, 0], FULL)), patch);
    let text = project.to_json().write();
    assert!(text.contains("\"mod\""), "wanted the MOD lane and the MOD page saved under mod, got {text}");
    let reopened = Project::from_json(&json::parse(&text).expect("a saved project is valid JSON")).expect("a saved project opens");
    assert!(unseeded(&reopened.tracks[0].rack) == project.tracks[0].rack, "wanted the MOD lane and the MOD page back exactly as saved from {text}");
    let plain = Project::new().to_json().write();
    assert!(!plain.contains("\"mod\""), "wanted a plain project saved without a mod key, got {plain}");
}

fn heard(patch: Params, modulation: i8) -> Session {
    let mut session = Session::open(&project(stepped(line(&[modulation], FULL)), patch));
    session.audio.note_on(0, KEY, VELOCITY);
    session.render(SLICE.1 as f64 / SAMPLE_RATE);
    session
}

fn shut_filter(depth: f64) -> Params {
    let mut patch = patch();
    patch.filter.on = true;
    patch.filter.cutoff_hz = SHUT_HZ;
    patch.filter.env_amount = 0.0;
    patch.filter.key_track = 0.0;
    patch.modulation = Modulation { cutoff: depth, ..Modulation::DEFAULT };
    patch
}

fn square_alone(depth: f64) -> Params {
    let mut patch = patch();
    patch.osc1 = Osc { levels: [0.0, 1.0, 0.0, 0.0], ..Osc::with_level(1.0) };
    patch.modulation = Modulation { pulse_width: depth, ..Modulation::DEFAULT };
    patch
}

fn panned(depth: f64) -> Params {
    let mut patch = patch();
    patch.modulation = Modulation { pan: depth, ..Modulation::DEFAULT };
    patch
}

fn at_the_key(session: &Session) -> f64 {
    amplitude(&session.left[SLICE.0..SLICE.1], KEY_HZ)
}

fn sides(session: &Session) -> (f32, f32) {
    (peak_of(&session.left[SLICE.0..SLICE.1]), peak_of(&session.right[SLICE.0..SLICE.1]))
}

#[test]
fn a_full_step_opens_the_filter_by_the_mods_cutoff_octaves_and_a_zero_step_leaves_it_shut() {
    let (shut, open) = (at_the_key(&heard(shut_filter(FOUR_OCTAVES), 0)), at_the_key(&heard(shut_filter(FOUR_OCTAVES), FULL)));
    assert!(open > f64::from(AUDIBLE) && open > CLEAR * shut, "wanted a +100 step to lift a filter shut at {SHUT_HZ} Hz {FOUR_OCTAVES} octaves and let the key through, got {open:.4} at {KEY_HZ} Hz against {shut:.4} with the step at 0");
    let closed = at_the_key(&heard(shut_filter(-FOUR_OCTAVES), FULL));
    assert!(closed < shut, "wanted a depth below zero to shut the filter further on a +100 step, got {closed:.5} against {shut:.5}");
}

#[test]
fn a_full_step_pans_the_note_by_the_mods_pan_and_a_zero_step_leaves_it_in_the_middle() {
    let (left, right) = sides(&heard(panned(1.0), FULL));
    assert!(right > AUDIBLE && left * FAR_OVER < right, "wanted a +100 step at a pan depth of 1 hard right, got {left} left and {right} right");
    let (left, right) = sides(&heard(panned(-1.0), FULL));
    assert!(left > AUDIBLE && right * FAR_OVER < left, "wanted a +100 step at a pan depth of -1 hard left, got {left} left and {right} right");
    let (left, right) = sides(&heard(panned(1.0), 0));
    assert!(left > AUDIBLE && (left - right).abs() < AUDIBLE, "wanted a 0 step left in the middle, got {left} left and {right} right");
}

#[test]
fn a_full_step_thins_a_square_by_the_mods_pulse_width_and_a_zero_step_leaves_it_square() {
    let second = |session: &Session| amplitude(&session.left[SLICE.0..SLICE.1], 2.0 * KEY_HZ);
    let (square, thin) = (second(&heard(square_alone(NEARLY_ALL_THE_WAY), 0)), second(&heard(square_alone(NEARLY_ALL_THE_WAY), FULL)));
    assert!(thin > f64::from(AUDIBLE) && thin > CLEAR * square, "wanted a +100 step to thin the square, bringing in the second harmonic a square lacks, got {thin:.4} at {} Hz against {square:.4} with the step at 0", 2.0 * KEY_HZ);
}

#[test]
fn a_step_moves_nothing_while_every_mod_depth_rests_at_zero() {
    for (what, patch) in [("the filter", shut_filter(0.0)), ("the pan", panned(0.0)), ("the square", square_alone(0.0))] {
        let (still, stepped) = (heard(patch, 0), heard(patch, FULL));
        assert_eq!((first_frame_apart(&stepped.left, &still.left), first_frame_apart(&stepped.right, &still.right)), (None, None), "wanted a +100 step to leave {what} sample for sample as a 0 step with its depth at 0, first frames apart");
        assert!(peak_of(&still.left) > AUDIBLE || what == "the filter", "wanted {what} heard, so the comparison proves something");
    }
}

fn agent() -> App {
    let mut project = Project::new();
    project.tracks.truncate(1);
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false}}", project.to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.take_requests();
    app
}

fn lane_written(app: &mut App, param: &str) -> Option<String> {
    let reply = call(app, "write", &format!(r#"{{"path":"tracks/0/curves","value":[{{"row":{CORE_ROW},"param":"{param}","nodes":[{{"beat":0,"value":0}},{{"beat":4,"value":1}}]}}]}}"#));
    assert!(!reply.failed, "wanted a curve written on {param}, got {}", reply.text);
    let curves = call(app, "read", r#"{"path":"tracks/0/curves"}"#);
    json::parse(&curves.text).and_then(|curves| curves.as_array().and_then(<[Json]>::first).and_then(|curve| curve.get("param").and_then(Json::as_str).map(String::from)))
}

#[test]
fn an_agent_finds_the_three_new_mod_depths_in_the_manual_and_a_curve_moves_each() {
    let mut app = agent();
    let manual = call(&mut app, "manual", r#"{"row":"core","path":"mod"}"#);
    let keys = json::parse(&manual.text).unwrap_or_else(|| panic!("wanted the Core's mod keys, got {}", manual.text));
    let lanes = call(&mut app, "manual", r#"{"row":"core","path":"lanes"}"#).text;
    for (key, range) in NEW_DEPTHS {
        let line = keys.get(key).and_then(Json::as_str).unwrap_or_default();
        assert!(line.starts_with(&format!("number {range}, default 0")), "wanted mod/{key} listed as a number {range} that rests at 0, got {line:?}");
        assert!(lanes.contains(&format!("mod.{key}")), "wanted mod.{key} among the keys a lane may move");
        assert_eq!(lane_written(&mut app, &format!("mod.{key}")).as_deref(), Some(format!("mod.{key}").as_str()), "wanted a curve on mod.{key} kept on that key");
    }
}

#[test]
fn a_lane_on_the_cores_sampled_switch_stays_on_the_switch_and_not_on_the_value_it_samples() {
    let mut app = agent();
    let lanes = call(&mut app, "manual", r#"{"row":"core","path":"lanes"}"#).text;
    assert!(lanes.contains("mod.sampled") && lanes.contains("mod.sample,"), "wanted the switch and the value it samples each among the keys a lane may move, got {lanes}");
    assert_eq!(lane_written(&mut app, "mod.sampled").as_deref(), Some("mod.sampled"), "wanted a curve written on mod.sampled kept on the switch");
    assert_eq!(lane_written(&mut app, "mod.sample").as_deref(), Some("mod.sample"), "wanted a curve written on mod.sample kept on the value");
}
