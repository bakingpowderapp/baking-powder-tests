use bp::arp::Arp;
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::core::params::{Envelope, Lfo, Modulator, Shape, Target};
use bp::face::{CELL, Faced, PITCH, READOUT_ROWS, View};
use bp::json;
use bp::project::{Preset, Project, Row, VELOCITY};
use support::{AUDIBLE, BLOCK, BLOCKS, SAMPLE_RATE, core_layer, first_difference, midi, peak};

const THIRD_TUNE: f64 = 7.0;
const VIEW_A: usize = 0;
const WOBBLE_HZ: f64 = 5.0;

fn quick() -> Params {
    let mut patch = Params::DEFAULT;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    patch
}

fn run(patch: Params) -> Vec<Vec<f32>> {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(0.5, patch))];
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(&project, None, &[], &[]);
    audio.note_on(0, 60, VELOCITY);
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(BLOCKS);
    for _ in 0..BLOCKS {
        renderer.render(&mut l, &mut r);
        out.push(l.clone());
    }
    out
}

fn cell_name(patch: &Params, page: usize, (row, column): (i32, i32)) -> Option<(&'static str, &'static str)> {
    let (x, y) = (column * PITCH + CELL / 2, (READOUT_ROWS + row) * PITCH + CELL / 2);
    patch.index_at(x, y, View { page, tray: None }).and_then(|index| patch.setting_of(index)).map(|(band, param, _)| (band, param))
}

#[test]
fn oscillators_three_and_four_sit_silent_until_raised() {
    let plain = run(quick());
    assert!(peak(&plain) > AUDIBLE, "the note is silent");
    let mut shaped = quick();
    shaped.osc3.levels[0] = 1.0;
    shaped.osc3.tune = THIRD_TUNE;
    shaped.osc4.levels[1] = 1.0;
    assert_eq!(first_difference(&run(shaped), &plain), None, "shapes on osc 3 and osc 4 at level 0 changed the sound");
    let mut third = shaped;
    third.osc3.level = 1.0;
    let with_third = run(third);
    assert!(first_difference(&with_third, &plain).is_some(), "osc 3 at level 1 added nothing");
    let mut fourth = shaped;
    fourth.osc4.level = 1.0;
    let with_fourth = run(fourth);
    assert!(first_difference(&with_fourth, &plain).is_some(), "osc 4 at level 1 added nothing");
    assert!(first_difference(&with_fourth, &with_third).is_some(), "osc 4 sounds exactly as osc 3 a fifth up");
}

#[test]
fn motion_reaches_the_new_oscillators_levels() {
    let mut still = quick();
    still.osc3.levels[0] = 1.0;
    still.osc3.level = 0.5;
    let steady = run(still);
    let mut third = still;
    third.motion.lfo1 = Lfo { target: Target::Level3, amount: 1.0, shape: Shape::Sine, rate_hz: WOBBLE_HZ, ..Lfo::DEFAULT };
    assert!(first_difference(&run(third), &steady).is_some(), "an LFO aimed at LEVEL 3 moved nothing");
    let mut fourth = still;
    fourth.motion.lfo1 = Lfo { target: Target::Level4, amount: 1.0, shape: Shape::Sine, rate_hz: WOBBLE_HZ, ..Lfo::DEFAULT };
    assert!(first_difference(&run(fourth), &steady).is_some(), "an LFO aimed at LEVEL 4 lifted the silent osc 4 to nothing");
    assert_eq!(Target::from_name("level3"), Some(Target::Level3), "the lane name level3 reads as LEVEL 3");
    assert_eq!(Target::from_name("pitch4"), Some(Target::Pitch4), "the lane name pitch4 reads as PITCH 4");
}

#[test]
fn the_new_keys_read_back_and_view_a_shows_four_oscillator_rows() {
    let mut patch = quick();
    patch.osc3.level = 0.3;
    patch.osc3.tune = THIRD_TUNE;
    patch.osc4.level = 0.2;
    patch.modulator = Modulator::Osc3;
    let preset = Preset::Row(Row::Core(core_layer(1.0, patch)), Vec::new());
    let text = preset.to_json().write();
    assert!(text.contains("\"osc3\"") && text.contains("\"osc4\"") && text.contains("\"modulator\": \"osc3\""), "wanted osc3, osc4 and modulator in the saved row, got {text}");
    assert_eq!(Preset::from_json(&json::parse(&text).expect("a saved row is valid JSON")), Some(preset), "the Core row changed in the save");
    for (row, band) in [(1, "OSC 1"), (2, "OSC 2"), (3, "OSC 3"), (4, "OSC 4")] {
        assert_eq!(cell_name(&patch, VIEW_A, (row, 0)), Some((band, "LEVEL")), "wanted {band}'s level at row {row} of view A");
    }
    assert_eq!(cell_name(&patch, VIEW_A, (5, 12)), Some(("OSC 1", "MODULATOR")), "wanted the modulator switch after the LFO FM cells on the toggles row");
    assert_eq!(cell_name(&patch, VIEW_A, (0, 3)), Some(("VOICE", "VOICES")), "wanted the voices cell on the tab row past the two view cells");
}
