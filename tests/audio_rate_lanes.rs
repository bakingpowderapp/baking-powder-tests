use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::core::params::Envelope;
use bp::json;
use bp::project::{Aim, Clip, LaneLfo, LfoShape, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use std::f64::consts::LN_2;
use support::{BLOCK, CORE_ROW, FRAMES_PER_BEAT, SAMPLE_RATE, amplitude, core_layer, midi};

const SIDEBAND_MIDI: u8 = 83;
const SIDEBAND_RATE_HZ: f64 = 200.0;
const SIDEBAND_DB: f64 = 0.5;
const SEARCH_HZ: f64 = 2.0;
const SEARCH_STEP_HZ: f64 = 0.1;
const SETTLED: usize = 24_000;
const HEARD: usize = 48_000;
const RETRIGGER_MIDI: u8 = 72;
const RETRIGGER_RATE_HZ: f64 = 150.0;
const ONSETS: [f32; 2] = [0.3, 1.001];
const NOTE_BEATS: f32 = 0.4;
const COMPARED_BEATS: f64 = 0.35;
const SAME: f32 = 1e-4;
const GLIDE_MIDI: u8 = 81;
const GLIDE_RATE_HZ: f64 = 0.5;
const GLIDE_SEMITONES: f64 = 1.0;
const TUNE_RANGE: f64 = 96.0;
const LISTEN_MS: f64 = 40.0;
const PITCH_SEMITONES: f64 = 0.05;
const FAST_HZ: f64 = 500.0;

fn lane(param: &str, amount: f64, shape: LfoShape, rate_hz: f64, retrigger: bool, audio: bool) -> LaneLfo {
    LaneLfo { amount, shape, sync: false, rate_hz, retrigger, audio, ..LaneLfo::blank(Aim::knob(CORE_ROW, "OSC 1", param)) }
}

fn project(lfo: LaneLfo, pitch: u8, notes: &[(f32, f32)]) -> Project {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(0.5, patch))];
    track.automation.lfos.push(lfo);
    let notes = notes.iter().map(|&(start, length)| Note { pitch, start, length, velocity: VELOCITY, articulation: Articulation::PLAIN }).collect();
    track.clips.push(Clip { start: 0, length: 8 * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn render(project: &Project, frames: usize) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(project, None, &[], &[]);
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(frames);
    while out.len() < frames {
        let n = BLOCK.min(frames - out.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    out
}

fn midi_hz(midi: u8) -> f64 {
    440.0 * ((f64::from(midi) - 69.0) / 12.0).exp2()
}

fn line_near(samples: &[f32], hz: f64) -> f64 {
    let steps = (SEARCH_HZ / SEARCH_STEP_HZ) as i32;
    (-steps..=steps).map(|step| amplitude(samples, hz + f64::from(step) * SEARCH_STEP_HZ)).fold(0.0, f64::max)
}

fn bessel(order: i32, x: f64) -> f64 {
    let mut term = (x / 2.0).powi(order) / (1..=order).map(f64::from).product::<f64>();
    let mut sum = term;
    for k in 1..20 {
        term *= -(x / 2.0).powi(2) / (f64::from(k) * f64::from(k + order));
        sum += term;
    }
    sum
}

fn rising_crossings(samples: &[f32]) -> Vec<f64> {
    samples.windows(2).enumerate().filter(|(_, pair)| pair[0] < 0.0 && pair[1] >= 0.0).map(|(at, pair)| at as f64 + f64::from(pair[0]) / f64::from(pair[0] - pair[1])).collect()
}

fn pitch_at(samples: &[f32], seconds: f64) -> f64 {
    let half = (LISTEN_MS / 1000.0 * SAMPLE_RATE) as usize;
    let centre = (seconds * SAMPLE_RATE) as usize;
    let crossings = rising_crossings(&samples[centre - half..centre + half]);
    let (first, last) = (crossings[0], crossings[crossings.len() - 1]);
    (crossings.len() - 1) as f64 * SAMPLE_RATE / (last - first)
}

fn reopened(project: &Project) -> Project {
    let saved = project.to_json().write();
    let json = json::parse(&saved).expect("a saved project is valid JSON");
    Project::from_json(&json).expect("a saved project opens")
}

#[test]
fn a_fine_lane_at_audio_rate_puts_fm_sidebands_beside_the_note() {
    let lfo = lane("FINE", 1.0, LfoShape::Sine, SIDEBAND_RATE_HZ, false, true);
    let heard = render(&project(lfo, SIDEBAND_MIDI, &[(0.0, 4.0)]), SETTLED + HEARD).split_off(SETTLED);
    let carrier = midi_hz(SIDEBAND_MIDI);
    let index = carrier * LN_2 / 12.0 / SIDEBAND_RATE_HZ;
    let wanted = 20.0 * (bessel(1, index) / bessel(0, index)).log10();
    let note = line_near(&heard, carrier);
    for side in [-1.0, 1.0] {
        let hz = carrier + side * SIDEBAND_RATE_HZ;
        let got = 20.0 * (line_near(&heard, hz) / note).log10();
        assert!((got - wanted).abs() < SIDEBAND_DB, "wanted the sideband near {hz:.0} Hz {wanted:.2} dB under the note, got {got:.2} dB");
    }
}

#[test]
fn a_retriggered_lane_at_audio_rate_restarts_on_each_notes_own_frame() {
    let lfo = lane("FINE", 1.0, LfoShape::Sine, RETRIGGER_RATE_HZ, true, true);
    let notes: Vec<(f32, f32)> = ONSETS.iter().map(|&start| (start, NOTE_BEATS)).collect();
    let heard = render(&project(lfo, RETRIGGER_MIDI, &notes), (2.0 * FRAMES_PER_BEAT) as usize);
    let span = (COMPARED_BEATS * FRAMES_PER_BEAT) as usize;
    let [first, second] = ONSETS.map(|start| (f64::from(start) * FRAMES_PER_BEAT).round() as usize);
    assert_ne!(first % 64, second % 64, "the two notes must land at different offsets within the 64-frame lane grid");
    let (frame, gap) = heard[first..first + span].iter().zip(&heard[second..second + span]).map(|(a, b)| (a - b).abs()).enumerate().fold((0, 0.0), |worst, (frame, gap)| if gap > worst.1 { (frame, gap) } else { worst });
    assert!(gap < SAME, "wanted the note at frame {second} to sound as the note at frame {first} did, frame by frame from its onset, worst {frame} frames in, off by {gap:.2e}");
}

#[test]
fn a_tune_lane_at_audio_rate_glides_between_semitones() {
    let lfo = LaneLfo { centre: 0.5, ..lane("TUNE", 2.0 * GLIDE_SEMITONES / TUNE_RANGE, LfoShape::Ramp, GLIDE_RATE_HZ, false, true) };
    let heard = render(&project(lfo, GLIDE_MIDI, &[(0.0, 4.0)]), (2.0 * SAMPLE_RATE) as usize);
    for (seconds, semitones) in [(0.5, -0.5 * GLIDE_SEMITONES), (1.5, 0.5 * GLIDE_SEMITONES)] {
        let got = 12.0 * (pitch_at(&heard, seconds) / midi_hz(GLIDE_MIDI)).log2();
        assert!((got - semitones).abs() < PITCH_SEMITONES, "wanted the pitch {semitones:+.2} semitones off the note at frame {}, between the semitones a stepped TUNE lands on, got {got:+.3}", (seconds * SAMPLE_RATE) as usize);
    }
}

#[test]
fn a_pitch_lane_reopens_at_its_audio_rate() {
    let opened = reopened(&project(lane("FINE", 0.2, LfoShape::Walk, FAST_HZ, false, true), 60, &[]));
    let lfo = &opened.tracks[0].automation.lfos[0];
    assert!(lfo.audio, "wanted the FINE lane to reopen at audio rate");
    assert_eq!(lfo.rate_hz, FAST_HZ, "wanted the FINE lane to reopen at {FAST_HZ} Hz");
}

#[test]
fn a_lane_not_at_audio_rate_reopens_at_32_hz_at_most() {
    for (param, audio) in [("FINE", false), ("LEVEL", true)] {
        let opened = reopened(&project(lane(param, 0.2, LfoShape::Sine, FAST_HZ, false, audio), 60, &[]));
        let lfo = &opened.tracks[0].automation.lfos[0];
        assert!(!lfo.audio, "wanted the {param} lane to reopen off audio rate");
        assert_eq!(lfo.rate_hz, LaneLfo::MAX_HZ, "wanted the {param} lane saved at {FAST_HZ} Hz to reopen at {} Hz", LaneLfo::MAX_HZ);
    }
}
