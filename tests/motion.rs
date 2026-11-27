use bp::arp::{Arp, MidiRow};
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::core::params::Envelope;
use bp::core::{Params, Shape, Target, Trigger};
use bp::division::Division;
use bp::face::{CELL, Face, Faced, PITCH, READOUT_ROWS};
use bp::json;
use bp::project::{Project, Row, VELOCITY};
use support::{AUDIBLE, BLOCK, SAMPLE_RATE, SILENT, core_layer, frames, midi, peak_of, unseeded};

const KEY: u8 = 60;
const OTHER: u8 = 64;
const GATE_HZ: f64 = 2.0;
const LATER_SECONDS: f64 = 0.125;
const HELD_SECONDS: f64 = 1.0;
const OCTAVE: f64 = 1.0;
const WINDOW_SECONDS: f64 = 0.05;
const NEAR_CYCLES: f64 = 1.5;
const BPM: f64 = 120.0;
const VIEW_B: usize = 1;
const VIEW_C: usize = 2;
const STEP_SHARES: [(i32, &str); 4] = [(12, "OSC 1"), (13, "OSC 2"), (14, "OSC 3"), (15, "OSC 4")];
const THIRD: u8 = 67;
const SOFT: u8 = 20;
const FALLEN_SECONDS: f64 = 0.3;
const RISE_SECONDS: f64 = 0.002;
const CROWD_SECONDS: f64 = 0.25;

fn hertz(midi: u8) -> f64 {
    440.0 * ((f64::from(midi) - 69.0) / 12.0).exp2()
}

fn plain() -> Params {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    patch
}

fn gated(trigger: Trigger) -> Params {
    let mut patch = plain();
    patch.motion.lfo1.target = Target::Amp;
    patch.motion.lfo1.amount = -1.0;
    patch.motion.lfo1.shape = Shape::Square;
    patch.motion.lfo1.rate_hz = GATE_HZ;
    patch.motion.lfo1.trigger = trigger;
    patch
}

fn lifted(sustain: f64, decay_ms: f64) -> Params {
    let mut patch = plain();
    patch.motion.env1.target = Target::Pitch;
    patch.motion.env1.amount = OCTAVE;
    patch.motion.env1.env = Envelope { attack_ms: 1.0, decay_ms, sustain, release_ms: 1.0, ..Envelope::MOTION };
    patch
}

fn project(patch: Params) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch))];
    project
}

struct Session {
    audio: Audio,
    renderer: Renderer,
    out: Vec<f32>,
}

impl Session {
    fn open(project: &Project) -> Session {
        let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
        renderer.patient();
        renderer.start_workers(0, || None);
        audio.sync(project, None, &[], &[]);
        Session { audio, renderer, out: Vec::new() }
    }

    fn render(&mut self, seconds: f64) {
        let end = self.out.len() + frames(seconds);
        let (mut left, mut right) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
        while self.out.len() < end {
            let n = BLOCK.min(end - self.out.len());
            self.renderer.render(&mut left[..n], &mut right[..n]);
            self.out.extend_from_slice(&left[..n]);
        }
    }
}

fn held(patch: Params, seconds: f64) -> Vec<f32> {
    let mut session = Session::open(&project(patch));
    session.audio.note_on(0, KEY, VELOCITY);
    session.render(seconds);
    session.out
}

fn two_notes(patch: Params) -> Vec<f32> {
    let mut session = Session::open(&project(patch));
    session.audio.note_on(0, KEY, VELOCITY);
    session.render(LATER_SECONDS);
    session.audio.note_on(0, OTHER, VELOCITY);
    session.render(HELD_SECONDS - LATER_SECONDS);
    session.out
}

fn switched(before: Params, after: Params, at: f64, seconds: f64) -> Vec<f32> {
    let mut session = Session::open(&project(before));
    session.audio.note_on(0, KEY, VELOCITY);
    session.render(at);
    session.audio.sync(&project(after), None, &[], &[]);
    session.render(seconds - at);
    session.out
}

fn noisy() -> Params {
    let mut patch = plain();
    patch.osc1.level = 0.0;
    patch.noise.on = true;
    patch.noise.level = 0.0;
    patch.motion.env1.target = Target::Noise;
    patch.motion.env1.amount = 1.0;
    patch.motion.env1.env = Envelope { attack_ms: 1.0, sustain: 1.0, ..Envelope::MOTION };
    patch
}

fn peak_between(samples: &[f32], from: f64, to: f64) -> f32 {
    peak_of(&samples[frames(from)..frames(to)])
}

fn expect_sound(samples: &[f32], from: f64, to: f64, when: &str) {
    let got = peak_between(samples, from, to);
    assert!(got > AUDIBLE, "wanted sound between {from} and {to} s {when}, got a peak of {got}");
}

fn expect_silence(samples: &[f32], from: f64, to: f64, when: &str) {
    let got = peak_between(samples, from, to);
    assert!(got <= SILENT, "wanted silence between {from} and {to} s {when}, got a peak of {got}");
}

fn cycles_between(samples: &[f32], from: f64, to: f64) -> f64 {
    samples[frames(from)..frames(to)].windows(2).filter(|pair| (pair[0] < 0.0) != (pair[1] < 0.0)).count() as f64 / 2.0
}

fn expect_frequency(samples: &[f32], at: f64, hz: f64, when: &str) {
    let wanted = hz * WINDOW_SECONDS;
    let got = cycles_between(samples, at, at + WINDOW_SECONDS);
    assert!((got - wanted).abs() <= NEAR_CYCLES, "wanted about {wanted:.1} cycles of {hz:.1} Hz in the {WINDOW_SECONDS} s from {at} s {when}, got {got:.1}");
}

fn same_bytes(a: &[f32], b: &[f32]) -> Result<(), usize> {
    match a.iter().zip(b).position(|(x, y)| x.to_bits() != y.to_bits()) {
        None if a.len() == b.len() => Ok(()),
        None => Err(a.len().min(b.len())),
        Some(frame) => Err(frame),
    }
}

fn centre(row: i32, column: i32) -> (i32, i32) {
    (column * PITCH + CELL / 2, (READOUT_ROWS + row) * PITCH + CELL / 2)
}

fn setting_at(faced: &impl Faced, face: &Face, row: i32, column: i32) -> Option<(&'static str, &'static str)> {
    let (x, y) = centre(row, column);
    faced.index_at(x, y, face.view()).and_then(|index| faced.setting_of(index)).map(|(band, param, _)| (band, param))
}

#[test]
fn a_row_aimed_at_nothing_leaves_the_render_untouched() {
    let mut idle = plain();
    idle.motion.lfo1.amount = 0.7;
    idle.motion.lfo1.rate_hz = 5.0;
    idle.motion.env1.amount = -0.5;
    let mut still = plain();
    still.motion.lfo2.target = Target::Cutoff;
    still.motion.env2.target = Target::Pitch;
    let reference = held(plain(), 0.5);
    let loud = peak_of(&reference);
    assert!(loud > AUDIBLE, "wanted the plain sine to sound, got a peak of {loud}");
    assert_eq!(same_bytes(&held(idle, 0.5), &reference), Ok(()), "a row with no target changed the render, first at frame");
    assert_eq!(same_bytes(&held(still, 0.5), &reference), Ok(()), "a row with a target but no amount changed the render, first at frame");
}

#[test]
fn a_triggered_lfo_gates_each_voice_from_its_own_note() {
    let out = two_notes(gated(Trigger::Trig));
    expect_silence(&out, 0.14, 0.24, "while both notes sit in the silent half of their own square");
    expect_sound(&out, 0.52, 0.60, "while the later note's square is in its loud half and the first's is not");
}

#[test]
fn a_running_lfo_gates_every_voice_as_one() {
    let out = two_notes(gated(Trigger::Run));
    expect_sound(&out, 0.27, 0.48, "while the one running square is in its loud half");
    expect_silence(&out, 0.52, 0.60, "while the one running square silences both notes together");
}

#[test]
fn a_synced_lfo_follows_the_tempo() {
    let mut patch = gated(Trigger::Trig);
    patch.motion.lfo1.sync = true;
    patch.motion.lfo1.division = Division::Quarter;
    let timed = |bpm: f64| {
        let mut project = project(patch);
        project.bpm = bpm;
        let mut session = Session::open(&project);
        session.audio.note_on(0, KEY, VELOCITY);
        session.render(HELD_SECONDS);
        session.out
    };
    let (fast, slow) = (timed(BPM), timed(BPM / 2.0));
    expect_sound(&fast, 0.30, 0.45, "at 120 bpm, where a quarter-note square has turned loud");
    expect_silence(&slow, 0.30, 0.45, "at 60 bpm, where the quarter-note square is still in its silent half");
    expect_sound(&slow, 0.55, 0.95, "at 60 bpm once the quarter-note square turns loud");
}

#[test]
fn an_envelope_lifts_the_pitch_by_its_amount() {
    let raised = held(lifted(1.0, 100.0), 0.5);
    expect_frequency(&raised, 0.3, 2.0 * hertz(KEY), "with the envelope held at full on a 100% pitch amount");
    let fallen = held(lifted(0.0, 100.0), 1.2);
    expect_frequency(&fallen, 1.0, hertz(KEY), "once the envelope has fallen back to the note");
    let (early, late) = (cycles_between(&fallen, 0.0, WINDOW_SECONDS), cycles_between(&fallen, 1.0, 1.0 + WINDOW_SECONDS));
    assert!(early > late + NEAR_CYCLES, "wanted the note to start above where it settles, got {early:.1} cycles early and {late:.1} late");
}

#[test]
fn the_view_buttons_turn_the_face_to_b_and_c_and_back_and_the_arp_waits_on_the_midi_row() {
    let mut face = Face::new();
    let mut patch = Params::DEFAULT;
    let (x, y) = centre(0, 1);
    assert!(patch.down(&mut face, x, y, BPM), "wanted the B button on the tab row to take the press");
    assert_eq!(face.page(), VIEW_B, "wanted the B button to turn the face to view B");
    assert_eq!(setting_at(&patch, &face, 1, 0), Some(("LFO 1", "TARGET")), "wanted LFO 1's target at the top of view B under the tab row");
    assert_eq!(setting_at(&patch, &face, 4, 5), Some(("ENV 2", "ATTACK")), "wanted ENV 2's attack at row 4, column 5, right of its delay");
    for (column, band) in STEP_SHARES {
        assert_eq!(setting_at(&patch, &face, 7, column), Some((band, "STEP SHARE")), "wanted {band}'s step share at row 7, column {column} of view B");
    }
    let (cx, cy) = centre(0, 2);
    assert!(patch.down(&mut face, cx, cy, BPM), "wanted the C button on the tab row to take the press");
    assert_eq!(face.page(), VIEW_C, "wanted the C button to turn the face to view C");
    assert_eq!(setting_at(&patch, &face, 2, 0), Some(("ANALOG", "DRIFT")), "wanted drift at the top of view C under its title");
    assert_eq!(setting_at(&patch, &face, 7, 14), Some(("ANALOG", "BYPASS")), "wanted view C's bypass switch at row 7, column 14");
    assert_eq!(setting_at(&MidiRow::DEFAULT, &Face::new(), 0, 0), None, "wanted the ARP title, no setting, first on the MIDI row's face");
    assert_eq!(setting_at(&MidiRow::DEFAULT, &Face::new(), 1, 0), Some(("ARP", "MODE")), "wanted the arp's mode under the ARP title on the MIDI row's face");
    let (ax, ay) = centre(0, 0);
    assert!(patch.down(&mut face, ax, ay, BPM), "wanted the A button on the tab row to take the press");
    assert_eq!(face.page(), 0, "wanted the A button to turn the face back to view A");
    assert_eq!(setting_at(&patch, &face, 1, 0), Some(("OSC 1", "LEVEL")), "wanted osc 1's level back at the top of view A under the tab row");
}

#[test]
fn the_motion_survives_a_save_and_the_default_writes_nothing() {
    let mut patch = plain();
    patch.motion.lfo1.target = Target::Pitch;
    patch.motion.lfo1.amount = 0.3;
    patch.motion.lfo1.trigger = Trigger::Run;
    patch.motion.env2.target = Target::Cutoff;
    patch.motion.env2.amount = -0.4;
    let saved = project(patch);
    let text = saved.to_json().write();
    for word in ["\"motion\"", "\"lfo1\"", "\"target\": \"pitch\"", "\"mode\": \"run\"", "\"env2\"", "\"target\": \"cutoff\""] {
        assert!(text.contains(word), "wanted {word} in the save, got {text}");
    }
    let reopened = Project::from_json(&json::parse(&text).expect("a saved project is valid JSON")).expect("a saved project opens");
    assert!(unseeded(&reopened.tracks[0].rack) == saved.tracks[0].rack, "wanted the rack back exactly as saved from {text}");
    let rested = project(plain()).to_json().write();
    assert!(!rested.contains("motion"), "wanted a resting motion left out of the save, got {rested}");
}

#[test]
fn an_envelope_brings_the_mixed_noise_in_from_a_level_of_zero() {
    expect_sound(&held(noisy(), 0.5), 0.1, 0.5, "with the noise level at zero and an envelope aimed at it");
    let mut unaimed = noisy();
    unaimed.motion.env1.target = Target::Off;
    expect_silence(&held(unaimed, 0.5), 0.1, 0.5, "with the noise level at zero and nothing aimed at it");
}

#[test]
fn a_triggered_lfo_keeps_its_time_while_its_amount_rests_at_zero() {
    let mut resting = gated(Trigger::Trig);
    resting.motion.lfo1.amount = 0.0;
    let out = switched(resting, gated(Trigger::Trig), LATER_SECONDS, HELD_SECONDS);
    expect_sound(&out, 0.02, 0.10, "while the amount rests at zero");
    expect_silence(&out, 0.14, 0.24, "once the amount rises inside the silent half the square reached from the note");
    expect_sound(&out, 0.27, 0.35, "in the loud half timed from the note, not from the rise");
    expect_silence(&out, 0.52, 0.60, "in the next silent half timed from the note");
}

#[test]
fn an_envelope_keeps_its_time_while_its_amount_rests_at_zero() {
    let mut resting = lifted(0.0, 100.0);
    resting.motion.env1.amount = 0.0;
    let out = switched(resting, lifted(0.0, 100.0), FALLEN_SECONDS, 2.0 * FALLEN_SECONDS);
    expect_frequency(&out, FALLEN_SECONDS + RISE_SECONDS, hertz(KEY), "right after the amount rises, the envelope having fallen long before");
}

#[test]
fn stealing_hears_a_level_an_envelope_opened() {
    let mut patch = plain();
    patch.voices = 2;
    patch.osc1.level = 0.0;
    patch.motion.env1.target = Target::Level1;
    patch.motion.env1.amount = 1.0;
    patch.motion.env1.env = Envelope { attack_ms: 1.0, sustain: 1.0, velocity: 1.0, ..Envelope::MOTION };
    let mut session = Session::open(&project(patch));
    session.audio.note_on(0, KEY, VELOCITY);
    session.audio.note_on(0, OTHER, SOFT);
    session.render(CROWD_SECONDS);
    session.audio.note_on(0, THIRD, SOFT);
    session.render(CROWD_SECONDS);
    let out = session.out;
    let (before, after) = (peak_between(&out, 0.1, CROWD_SECONDS), peak_between(&out, CROWD_SECONDS + 0.05, 2.0 * CROWD_SECONDS));
    assert!(before > AUDIBLE, "wanted the loud note to sound through its envelope, got a peak of {before}");
    assert!(after > before * 0.75, "wanted the soft note stolen and the loud one kept, got a peak of {after} after against {before} before");
}
