use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params as CoreParams;
use bp::core::params::Envelope;
use bp::face::Faced;
use bp::fx::params::{Delay, DelayMode};
use bp::fx::{Kind, Params, Slot};
use bp::project::{Aim, Clip, Curve, Node, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{AUDIBLE, BLOCK, CORE_ROW, FRAMES_PER_BEAT, SAMPLE_RATE, beat_frame, core_layer, midi, peak_of, worst_delta};

const TIME_MS: f64 = 333.33;
const FEEDBACK: f64 = 0.5;
const SECONDS: f64 = 7.0;
const FREEZE_AT: f64 = 0.6;
const RELEASE_AT: f64 = 4.0;
const SETTLED: f64 = 0.5;
const NOTE_BEATS: [usize; 3] = [0, 4, 11];
const NOTE_LENGTH: f32 = 0.25;
const CLIP_BEATS: i32 = 16;
const ECHO_SECONDS: f64 = 0.175;
const TOLERANCE: f32 = 1e-4;
const FADING: f32 = 0.9;
const FADE_TOLERANCE: f32 = 0.02;
const ECHO_TOLERANCE: f32 = 0.1;
const DRIVERS: [(bool, &str); 2] = [(false, "the FRZ cell pressed"), (true, "a FREEZE lane")];
const DELAY_ROW: usize = CORE_ROW + 1;

fn delay(freeze: bool) -> Row {
    let params = Params::Delay(Delay { mode: DelayMode::Clean, time_l_ms: TIME_MS, time_r_ms: TIME_MS, feedback: FEEDBACK, freeze, ..Delay::DEFAULT });
    Row::Effect(Slot { wet: 1.0, params, ..Slot::new(Kind::Delay) })
}

fn song() -> Project {
    let mut patch = CoreParams::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    let notes = NOTE_BEATS.iter().map(|&beat| Note { pitch: 69, start: beat as f32, length: NOTE_LENGTH, velocity: VELOCITY, articulation: Articulation::PLAIN }).collect();
    let mut project = Project::new();
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch)), delay(false)];
    project.tracks[0].clips = vec![Clip { start: 0, length: CLIP_BEATS * TICKS_PER_BEAT, label: String::new(), notes, file: None }];
    project
}

fn frame(seconds: f64) -> usize {
    (seconds * SAMPLE_RATE) as usize
}

fn tick(seconds: f64) -> i32 {
    (seconds * SAMPLE_RATE / FRAMES_PER_BEAT * f64::from(TICKS_PER_BEAT)).round() as i32
}

fn period() -> usize {
    (TIME_MS * 0.001 * SAMPLE_RATE).round() as usize
}

fn lane() -> Curve {
    let steps = [(0, 0.0), (tick(FREEZE_AT), 0.0), (tick(FREEZE_AT) + 1, 1.0), (tick(RELEASE_AT), 1.0), (tick(RELEASE_AT) + 1, 0.0)];
    let nodes = steps.iter().map(|&(tick, travel)| Node { tick, travel, bend: 0.0 }).collect();
    Curve { aim: Aim::knob(DELAY_ROW, "LOOP", "FREEZE"), lfo: None, nodes }
}

fn play(by_lane: bool) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    let mut project = song();
    let mut presses = if by_lane { Vec::new() } else { vec![(frame(FREEZE_AT), true), (frame(RELEASE_AT), false)] }.into_iter().peekable();
    if by_lane {
        project.tracks[0].automation.curves.push(lane());
    }
    audio.sync(&project, None, &[], &[]);
    audio.play(0.0);
    let frames = frame(SECONDS);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(frames);
    while out.len() < frames {
        if let Some(&(at, freeze)) = presses.peek()
            && at <= out.len()
        {
            project.tracks[0].rack[DELAY_ROW] = delay(freeze);
            audio.sync(&project, None, &[], &[]);
            presses.next();
        }
        let until = presses.peek().map_or(frames, |&(at, _)| at);
        let n = BLOCK.min(frames - out.len()).min(until - out.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    out
}

fn turn(out: &[f32], k: usize) -> &[f32] {
    &out[k * period()..(k + 1) * period()]
}

fn echo_of(out: &[f32], note_beat: usize) -> f32 {
    let start = beat_frame(note_beat) + period();
    peak_of(&out[start..start + frame(ECHO_SECONDS)])
}

#[test]
fn a_frozen_line_repeats_its_turn_unchanged_and_hears_no_new_note() {
    for (by_lane, how) in DRIVERS {
        let out = play(by_lane);
        let period = period();
        let first = frame(FREEZE_AT + SETTLED) / period + 1;
        let turns = frame(RELEASE_AT) / period;
        let held = peak_of(turn(&out, first));
        assert!(held > AUDIBLE, "{how}: wanted the frozen line to still carry the note from frame {}, peak {held}", first * period);
        for k in first..turns - 1 {
            let worst = worst_delta(turn(&out, k), turn(&out, k + 1));
            assert!(worst < TOLERANCE, "{how}: wanted turn {} at frames {}..{} to match the turn before within {TOLERANCE}, worst delta {worst}", k + 1, (k + 1) * period, (k + 2) * period);
        }
    }
}

#[test]
fn a_released_line_fades_by_its_feedback_each_turn_and_echoes_the_next_note_as_before() {
    for (by_lane, how) in DRIVERS {
        let out = play(by_lane);
        let fade = |k: usize| peak_of(turn(&out, k + 1)) / peak_of(turn(&out, k));
        let before = fade(1);
        assert!(before < FADING, "{how}: wanted feedback {FEEDBACK} to fade the echoes turn by turn before the freeze, ratio {before}");
        let settled = frame(RELEASE_AT + SETTLED) / period() + 1;
        let earlier = peak_of(turn(&out, settled));
        assert!(earlier > AUDIBLE, "{how}: wanted the line to keep sounding after release at turn {settled}, peak {earlier}");
        let after = fade(settled);
        assert!((after - before).abs() < FADE_TOLERANCE, "{how}: wanted turn {} to fade from turn {settled} as turn 2 faded from turn 1: ratio {before} before the freeze, {after} after release", settled + 1);
        let (first, again) = (echo_of(&out, NOTE_BEATS[0]), echo_of(&out, NOTE_BEATS[2]));
        assert!((again - first).abs() < first * ECHO_TOLERANCE, "{how}: wanted the note at beat {} to echo as the first did once released: peaks {first} then {again}", NOTE_BEATS[2]);
    }
}

#[test]
fn the_freeze_cell_arms_a_lane_and_the_lane_saves_by_key() {
    let slot = Slot::new(Kind::Delay);
    let cell = slot.automatable_of("LOOP", "FREEZE").map(|(index, _, _)| index).expect("FREEZE should be automatable");
    assert_eq!(slot.armable(cell), Some(("LOOP", "FREEZE")));
    assert_eq!(slot.params.travel_of("LOOP", "FREEZE"), Some(0.0));
    let mut frozen = slot;
    assert!(frozen.params.set_travel("LOOP", "FREEZE", 1.0));
    assert_eq!(frozen.params.travel_of("LOOP", "FREEZE"), Some(1.0));
    assert!(matches!(frozen.params, Params::Delay(Delay { freeze: true, .. })));
    assert_eq!(slot.spelling("LOOP", "FREEZE", 1.0).map(|(word, _)| word), Some(String::from("ON")));
    assert_eq!(slot.spelling("LOOP", "FREEZE", 0.0).map(|(word, _)| word), Some(String::from("OFF")));
    let knob = Kind::Delay.knobs().iter().find(|knob| knob.band == "LOOP" && knob.name == "FREEZE").expect("FREEZE should be a knob a lane saves by");
    assert_eq!(knob.path, "freeze");
}
