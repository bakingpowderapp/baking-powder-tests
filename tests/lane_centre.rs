use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::core::params::Envelope;
use bp::json;
use bp::project::{Aim, Clip, LaneLfo, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use std::f64::consts::TAU;
use support::{BLOCK, CORE_ROW, SAMPLE_RATE, WINDOW, core_layer, midi};

const SECONDS: usize = 2;
const TOLERANCE: f32 = 0.03;
const KNOB: f64 = 0.5;
const FULL: f64 = 1.0;
const CENTRE: f64 = 0.9;
const TOP_FROM: usize = 2_400;
const TOP_TO: usize = 21_600;
const TROUGH_AT: usize = 36_000;
const TROUGH: f32 = 0.4;
const EXACT: f64 = 1e-9;

fn lane(amount: f64, centre: f64) -> LaneLfo {
    LaneLfo { amount, centre, sync: false, rate_hz: 1.0, ..LaneLfo::blank(Aim::knob(CORE_ROW, "OSC 1", "LEVEL")) }
}

fn project(level: f64, lanes: Vec<LaneLfo>) -> Project {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.osc1.level = level;
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

fn peaks(samples: &[f32]) -> Vec<f32> {
    samples.as_chunks::<WINDOW>().0.iter().map(|window| window.iter().fold(0.0, |peak, s| s.abs().max(peak))).collect()
}

fn against_full_level(samples: &[f32]) -> Vec<f32> {
    let full = peaks(&render(&project(FULL, Vec::new()))).into_iter().skip(1).fold(0.0, f32::max);
    peaks(samples).iter().map(|peak| peak / full).collect()
}

fn expect_level(envelope: &[f32], frame: usize, wanted: f32) {
    let got = envelope[frame / WINDOW];
    assert!((got - wanted).abs() <= TOLERANCE, "wanted level {wanted:.2} of full at frame {frame}, got {got:.3}");
}

fn expect_shape(envelope: &[f32], shape: impl Fn(f64) -> f64, from: usize) {
    for (index, got) in envelope.iter().enumerate().skip(from / WINDOW) {
        let frame = index * WINDOW + WINDOW / 2;
        let wanted = shape(frame as f64 / SAMPLE_RATE) as f32;
        assert!((got - wanted).abs() <= TOLERANCE, "wanted level {wanted:.3} of full at frame {frame}, got {got:.3}");
    }
}

#[test]
fn a_high_centre_cuts_the_wave_at_the_top_and_keeps_its_trough() {
    let envelope = against_full_level(&render(&project(KNOB, vec![lane(1.0, CENTRE)])));
    for frame in (TOP_FROM..=TOP_TO).step_by(WINDOW) {
        expect_level(&envelope, frame, 1.0);
    }
    expect_level(&envelope, TROUGH_AT, TROUGH);
    expect_shape(&envelope, |t| (CENTRE + 0.5 * (TAU * t).sin()).clamp(0.0, 1.0), WINDOW);
}

#[test]
fn a_save_keeps_a_centre_the_amount_once_capped() {
    let text = project(KNOB, vec![lane(1.0, CENTRE)]).to_json().write();
    let reopened = Project::from_json(&json::parse(&text).expect("a saved project is valid JSON")).expect("a saved project opens");
    let kept = reopened.tracks[0].automation.lfos.first().unwrap_or_else(|| panic!("wanted the lane back from {text}"));
    assert!((kept.centre - CENTRE).abs() < EXACT && kept.amount == 1.0, "wanted the lane back with centre {CENTRE} and amount 1, got centre {} and amount {} from {text}", kept.centre, kept.amount);
}
