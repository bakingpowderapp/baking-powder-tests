use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::core::params::Envelope;
use bp::dsp::sample_hold;
use bp::param;
use bp::project::{Aim, Clip, DEFAULT_BPM, LaneLfo, LfoShape, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use std::f64::consts::TAU;
use support::{CORE_ROW, SAMPLE_RATE, WINDOW, core_layer, midi};

const SECONDS: usize = 4;
const TOLERANCE: f32 = 0.03;
const RETRIGGER_TOLERANCE: f32 = 0.08;
const TWO_NOTES: [(f32, f32); 2] = [(0.0, 2.0), (2.5, 5.5)];
const EFFECT_ROW: usize = CORE_ROW + 1;
const FIRST_BAND: &str = "KEY";
const RENAMED: [(&str, &str); 4] = [("RANGE", "LOW"), ("RANGE", "HIGH"), ("SIGNAL", "DETECTOR"), ("SIGNAL", "TAP")];
const FIRST_LOW: f64 = 0.77;
const TWO_PLACES: f64 = 0.005;
const ROUNDED: f64 = 1e-9;

fn lfo(sync: bool, rate_hz: f64, phase: f64, amount: f64, retrigger: bool) -> LaneLfo {
    LaneLfo { amount, sync, rate_hz, phase, retrigger, ..LaneLfo::blank(Aim::knob(CORE_ROW, "OSC 1", "LEVEL")) }
}

fn project(lfo: LaneLfo, notes: &[(f32, f32)]) -> Project {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.osc1.level = 0.5;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    let mut project = Project::new();
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch))];
    track.automation.lfos.push(lfo);
    let notes = notes.iter().map(|&(start, length)| Note { pitch: 69, start, length, velocity: VELOCITY, articulation: Articulation::PLAIN }).collect();
    track.clips.push(Clip { start: 0, length: 8 * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn render(project: &Project, block: usize) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, block, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(project, None, &[], &[]);
    audio.play(0.0);
    let frames = SECONDS * SAMPLE_RATE as usize;
    let (mut left, mut right) = (vec![0.0; block], vec![0.0; block]);
    let mut out = Vec::with_capacity(frames);
    while out.len() < frames {
        let n = block.min(frames - out.len());
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

fn expect_level(envelope: &[f32], frame: usize, wanted: f32, within: f32) {
    let got = envelope[frame / WINDOW];
    assert!((got - wanted).abs() <= within, "wanted level {wanted:.2} at frame {frame}, got {got:.3}");
}

fn expect_sine(envelope: &[f32], shape: impl Fn(f64) -> f64, from: usize) {
    for (index, got) in envelope.iter().enumerate().skip(from / WINDOW) {
        let frame = index * WINDOW + WINDOW / 2;
        let wanted = shape(frame as f64 / SAMPLE_RATE) as f32;
        assert!((got - wanted).abs() <= TOLERANCE, "wanted level {wanted:.3} at frame {frame}, got {got:.3}");
    }
}

#[test]
fn free_at_one_hertz_peaks_a_quarter_second_in() {
    let envelope = envelope(&render(&project(lfo(false, 1.0, 0.0, 1.0, false), &[(0.0, 8.0)]), 256));
    expect_level(&envelope, 12_000, 1.0, TOLERANCE);
    expect_level(&envelope, 36_000, 0.0, TOLERANCE);
    expect_sine(&envelope, |t| 0.5 + 0.5 * (TAU * t).sin(), WINDOW);
}

#[test]
fn half_the_height_swings_half_as_far() {
    let envelope = envelope(&render(&project(lfo(true, 1.0, 0.0, 0.5, false), &[(0.0, 8.0)]), 256));
    expect_level(&envelope, 18_000, 0.25 / 0.75, TOLERANCE);
    expect_sine(&envelope, |t| (0.5 + 0.25 * (TAU * 2.0 * t).sin()) / 0.75, WINDOW);
}

#[test]
fn the_render_is_the_same_bytes_at_every_buffer() {
    for lfo in [lfo(true, 1.0, 0.0, 1.0, false), lfo(true, 1.0, 0.0, 1.0, true), lfo(false, 1.0, 0.0, 1.0, false)] {
        let project = project(lfo, &TWO_NOTES);
        let reference = render(&project, 64);
        for block in [256, 4096] {
            let other = render(&project, block);
            if let Some(frame) = reference.iter().zip(&other).position(|(a, b)| a.to_bits() != b.to_bits()) {
                panic!("wanted the same bytes at {block} frames as at 64, got a difference at frame {frame}");
            }
        }
    }
}

#[test]
fn retrigger_restarts_the_cycle_at_the_second_note() {
    let plain = envelope(&render(&project(lfo(true, 1.0, 0.0, 1.0, false), &TWO_NOTES), 256));
    let retriggered = envelope(&render(&project(lfo(true, 1.0, 0.0, 1.0, true), &TWO_NOTES), 256));
    expect_level(&plain, 66_000, 0.0, RETRIGGER_TOLERANCE);
    expect_level(&retriggered, 66_000, 1.0, RETRIGGER_TOLERANCE);
    expect_level(&retriggered, 78_000, 0.0, RETRIGGER_TOLERANCE);
}

#[test]
fn a_random_lane_on_a_renamed_control_draws_the_run_it_drew_under_its_first_words() {
    for (band, name) in RENAMED {
        let lane = LaneLfo::blank(Aim::knob(EFFECT_ROW, band, name));
        assert_eq!(lane.seed, param::seed(FIRST_BAND, name), "wanted a lane on {band} {name} to draw the run it drew as {FIRST_BAND} {name}, not the one its new words would give");
    }
    let low = LaneLfo { shape: LfoShape::Random, amount: 1.0, ..LaneLfo::blank(Aim::knob(EFFECT_ROW, "RANGE", "LOW")) };
    let first = 2.0 * (low.travel(&[], &[], 0.0, DEFAULT_BPM, 1.0) - low.centre);
    let drawn = sample_hold(0, param::seed(FIRST_BAND, "LOW"));
    assert!((first - drawn).abs() < ROUNDED, "wanted the first step of a Random lane on RANGE LOW to be the {drawn:+.3} KEY LOW drew, got {first:+.3}");
    assert!((first - FIRST_LOW).abs() < TWO_PLACES, "wanted a Random lane on Space's RANGE LOW to open at +{FIRST_LOW} as it did before the rename, got {first:+.3}");
}
