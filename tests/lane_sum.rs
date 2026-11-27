use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::core::params::Envelope;
use bp::project::{Aim, Clip, LaneLfo, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use std::f64::consts::TAU;
use support::{BLOCK, CORE_ROW, SAMPLE_RATE, WINDOW, core_layer, midi, worst_delta};

const SECONDS: usize = 3;
const TOLERANCE: f32 = 0.03;
const SAME: f32 = 1e-4;
const HALF_TURN: f64 = 0.5;
const KNOB: f64 = 0.5;
const SWING: f64 = 0.25;

fn lane(rate_hz: f64, phase: f64, centre: f64) -> LaneLfo {
    LaneLfo { amount: 2.0 * SWING, sync: false, rate_hz, phase, centre, ..LaneLfo::blank(Aim::knob(CORE_ROW, "OSC 1", "LEVEL")) }
}

fn project(lanes: Vec<LaneLfo>) -> Project {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.osc1.level = KNOB;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch))];
    track.automation.lfos = lanes;
    let notes = vec![Note { pitch: 69, start: 0.0, length: 8.0, velocity: VELOCITY, articulation: Articulation::PLAIN }];
    track.clips.push(Clip { start: 0, length: 8 * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn render(project: &Project) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(project, None, &[], &[]);
    audio.play(0.0);
    let frames = SECONDS * SAMPLE_RATE as usize;
    let (mut left, mut right) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(frames);
    while out.len() < frames {
        let n = BLOCK.min(frames - out.len());
        renderer.render(&mut left[..n], &mut right[..n]);
        out.extend_from_slice(&left[..n]);
    }
    out
}

fn envelope(samples: &[f32]) -> Vec<f32> {
    let peaks: Vec<f32> = samples.as_chunks::<WINDOW>().0.iter().map(|window| window.iter().fold(0.0, |peak, s| s.abs().max(peak))).collect();
    let loudest = peaks.iter().copied().fold(0.0, f32::max);
    peaks.iter().map(|peak| peak / loudest).collect()
}

fn at_window(index: usize) -> (usize, f64) {
    let frame = index * WINDOW + WINDOW / 2;
    (frame, frame as f64 / SAMPLE_RATE)
}

fn expect_shape(envelope: &[f32], shape: impl Fn(f64) -> f64, from: usize) {
    let loudest = (from / WINDOW..envelope.len()).map(|index| shape(at_window(index).1)).fold(0.0, f64::max);
    for (index, got) in envelope.iter().enumerate().skip(from / WINDOW) {
        let (frame, seconds) = at_window(index);
        let wanted = (shape(seconds) / loudest) as f32;
        assert!((got - wanted).abs() <= TOLERANCE, "wanted level {wanted:.3} of the loudest at frame {frame}, got {got:.3}");
    }
}

fn first_difference(a: &[f32], b: &[f32]) -> Option<usize> {
    a.iter().zip(b).position(|(x, y)| x.to_bits() != y.to_bits())
}

#[test]
fn opposite_phases_cancel_and_the_level_holds() {
    let plain = render(&project(Vec::new()));
    let cancelled = render(&project(vec![lane(1.0, 0.0, KNOB), lane(1.0, HALF_TURN, KNOB)]));
    for (index, got) in envelope(&cancelled).iter().enumerate().skip(1) {
        let (frame, _) = at_window(index);
        assert!((got - 1.0).abs() <= TOLERANCE, "wanted the level flat at frame {frame} with the swings cancelling, got {got:.3} of the loudest window");
    }
    let worst = worst_delta(&plain[WINDOW..], &cancelled[WINDOW..]);
    let differs = first_difference(&plain, &cancelled);
    assert!(worst < SAME, "wanted two opposite lanes to render as no lane within {SAME}, got a worst delta of {worst} with the first difference at frame {differs:?}");
}

#[test]
fn two_rates_add_their_swings_around_the_first_centre() {
    let envelope = envelope(&render(&project(vec![lane(1.0, 0.0, KNOB), lane(2.0, 0.0, KNOB)])));
    expect_shape(&envelope, |t| KNOB + SWING * (TAU * t).sin() + SWING * (TAU * 2.0 * t).sin(), WINDOW);
}

#[test]
fn only_the_first_lanes_centre_is_heard() {
    let reference = render(&project(vec![lane(1.0, 0.0, KNOB), lane(2.0, 0.0, KNOB)]));
    let second_moved = render(&project(vec![lane(1.0, 0.0, KNOB), lane(2.0, 0.0, 0.9)]));
    if let Some(frame) = first_difference(&reference, &second_moved) {
        panic!("wanted the second lane's centre ignored, got different bytes at frame {frame}: {} against {}", reference[frame], second_moved[frame]);
    }
    let first_moved = render(&project(vec![lane(1.0, 0.0, 0.9), lane(2.0, 0.0, KNOB)]));
    assert!(first_difference(&reference, &first_moved).is_some(), "wanted the first lane's centre at 0.9 to lift the level, got the same bytes as at {KNOB}");
}
