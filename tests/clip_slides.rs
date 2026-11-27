use bp::arp::{Arp, Articulation, Join};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::core::params::{Envelope, GlideCurve, Mono};
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{BLOCK, HALF, SAMPLE_RATE, WINDOW, amplitude, beat_frame, core_layer, midi, peak_of};

const LOW: u8 = 57;
const HIGH: u8 = 69;
const HIGH_HZ: f64 = 440.0;
const MIDWAY_HZ: f64 = 311.127;
const MONO: usize = 1;
const POLY: usize = 16;
const GLIDE_MS: f64 = 100.0;
const ATTACK_MS: f64 = 50.0;
const DECAY_MS: f64 = 50.0;
const SUSTAIN: f64 = 0.5;
const CLIP_BEATS: i32 = 2;
const HELD_MS: f64 = 20.0;
const AFTER_MS: f64 = 150.0;
const MIDWAY_MS: (f64, f64) = (45.0, 55.0);
const TIED_MS: f64 = 5.0;
const REACHED_MS: f64 = 110.0;
const SPAN: usize = 1_200;
const STEADY: f64 = 0.05;
const NO_NEW_PEAK: f64 = 1.1;
const NEW_PEAK: f64 = 1.5;
const IN_TUNE_CENTS: f64 = 10.0;

fn patch(voices: usize) -> Params {
    let mut patch = Params::DEFAULT;
    patch.voices = voices;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.amp = Envelope { attack_ms: ATTACK_MS, decay_ms: DECAY_MS, sustain: SUSTAIN, ..Envelope::DEFAULT };
    patch.mono = Mono { glide_ms: GLIDE_MS, curve: GlideCurve::Linear, ..Mono::DEFAULT };
    patch
}

fn note(pitch: u8, start: f32, join: Join) -> Note {
    Note { pitch, start, length: 1.0, velocity: VELOCITY, articulation: Articulation { join, ..Articulation::PLAIN } }
}

fn project(voices: usize, join: Join) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch(voices)))];
    track.clips.push(Clip { start: 0, length: CLIP_BEATS * TICKS_PER_BEAT, label: String::new(), notes: vec![note(LOW, 0.0, Join::Play), note(HIGH, 1.0, join)], file: None });
    project
}

fn render(voices: usize, join: Join) -> Vec<f32> {
    let frames = beat_frame(CLIP_BEATS as usize);
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(&project(voices, join), None, &[], &[]);
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

fn frames(ms: f64) -> usize {
    (ms * SAMPLE_RATE / 1000.0).round() as usize
}

fn sustain(out: &[f32]) -> f64 {
    let beat = beat_frame(1);
    f64::from(peak_of(&out[beat - frames(HELD_MS)..beat]))
}

fn levels(out: &[f32], from: usize, to: usize) -> Vec<(usize, f64)> {
    let held = sustain(out);
    (from..to).step_by(WINDOW).map(|at| (at, f64::from(peak_of(&out[at..at + WINDOW])) / held)).collect()
}

fn loudest_after_beat(out: &[f32]) -> (usize, f64) {
    let beat = beat_frame(1);
    levels(out, beat, beat + frames(AFTER_MS)).into_iter().max_by(|a, b| a.1.total_cmp(&b.1)).unwrap_or((beat, f64::NAN))
}

fn furthest_from_sustain(out: &[f32]) -> (usize, f64) {
    let beat = beat_frame(1);
    levels(out, beat - frames(HELD_MS), beat + frames(AFTER_MS)).into_iter().max_by(|a, b| (a.1 - 1.0).abs().total_cmp(&(b.1 - 1.0).abs())).unwrap_or((beat, f64::NAN))
}

fn strongest_midway(out: &[f32]) -> (usize, f64) {
    let beat = beat_frame(1);
    (0..=GLIDE_MS as usize).map(|ms| beat + frames(ms as f64)).map(|at| (at, amplitude(&out[at - SPAN / 2..at + SPAN / 2], MIDWAY_HZ))).max_by(|a, b| a.1.total_cmp(&b.1)).unwrap_or((beat, f64::NAN))
}

fn cents(heard: f64, wanted: f64) -> f64 {
    1200.0 * (heard / wanted).log2()
}

fn furthest_period(out: &[f32], from: usize, wanted: f64) -> (usize, f64) {
    let rises: Vec<f64> = out[from..from + SPAN].windows(2).enumerate().filter(|(_, pair)| pair[0] < 0.0 && pair[1] >= 0.0).map(|(at, pair)| (from + at) as f64 + f64::from(pair[0] / (pair[0] - pair[1]))).collect();
    rises.windows(2).map(|pair| (pair[0] as usize, SAMPLE_RATE / (pair[1] - pair[0]))).max_by(|a, b| cents(a.1, wanted).abs().total_cmp(&cents(b.1, wanted).abs())).unwrap_or((from, 0.0))
}

fn in_tune_from(join: Join, after_ms: f64) {
    let from = beat_frame(1) + frames(after_ms);
    let (at, heard) = furthest_period(&render(MONO, join), from, HIGH_HZ);
    assert!(cents(heard, HIGH_HZ).abs() <= IN_TUNE_CENTS, "wanted the {} into pitch {HIGH} on a mono Core at {HIGH_HZ} Hz from {after_ms} ms after beat 1, every period from frame {from} to {} within {IN_TUNE_CENTS} cents of it, got {heard:.2} Hz in the period from frame {at}", join.spell(), from + SPAN);
}

fn no_new_attack(voices: usize, join: Join) {
    let out = render(voices, join);
    let (at, loudest) = loudest_after_beat(&out);
    let beat = beat_frame(1);
    assert!(loudest < NO_NEW_PEAK, "wanted the {} into pitch {HIGH} on a {voices} voice Core to carry on with no new attack, no 5 ms window from frame {beat} to {} over {NO_NEW_PEAK} times the sustain level {} held before beat 1, got {loudest:.3} times it in the window from frame {at}", join.spell(), beat + frames(AFTER_MS), sustain(&out));
}

#[test]
fn a_slide_into_a_new_pitch_on_a_mono_core_holds_its_sustain_with_no_new_attack() {
    let out = render(MONO, Join::Slide);
    let (at, level) = furthest_from_sustain(&out);
    let beat = beat_frame(1);
    assert!((level - 1.0).abs() <= STEADY, "wanted every 5 ms window from frame {} to {} within {STEADY} of the sustain level {} held before beat 1, got {level:.3} times it in the window from frame {at}", beat - frames(HELD_MS), beat + frames(AFTER_MS), sustain(&out));
}

#[test]
fn a_slide_on_a_mono_core_passes_the_midway_pitch_halfway_through_its_glide() {
    let out = render(MONO, Join::Slide);
    let (at, strongest) = strongest_midway(&out);
    let beat = beat_frame(1);
    let after_ms = (at - beat) as f64 * 1000.0 / SAMPLE_RATE;
    let level = sustain(&out);
    assert!((MIDWAY_MS.0..=MIDWAY_MS.1).contains(&after_ms) && strongest > f64::from(HALF) * level, "wanted {MIDWAY_HZ} Hz strongest {} to {} ms after beat 1, between frames {} and {}, at over half the sustain level {level}, got its strongest reading {strongest:.4} at frame {at}, {after_ms} ms after", MIDWAY_MS.0, MIDWAY_MS.1, beat + frames(MIDWAY_MS.0), beat + frames(MIDWAY_MS.1));
}

#[test]
fn a_slide_on_a_mono_core_reaches_the_new_pitch_once_its_glide_is_done() {
    in_tune_from(Join::Slide, REACHED_MS);
}

#[test]
fn a_tie_into_a_new_pitch_on_a_mono_core_sounds_it_at_once() {
    in_tune_from(Join::Tie, TIED_MS);
}

#[test]
fn a_tie_into_a_new_pitch_on_a_mono_core_strikes_no_new_peak() {
    no_new_attack(MONO, Join::Tie);
}

#[test]
fn a_plain_note_after_another_on_a_mono_core_strikes_a_new_attack() {
    let out = render(MONO, Join::Play);
    let (at, loudest) = loudest_after_beat(&out);
    let beat = beat_frame(1);
    assert!(loudest > NEW_PEAK, "wanted a new attack rising past {NEW_PEAK} times the sustain level {} held before beat 1 between frames {beat} and {}, got at most {loudest:.3} times it in the window from frame {at}", sustain(&out), beat + frames(AFTER_MS));
}

#[test]
fn a_tie_into_a_new_pitch_on_a_polyphonic_core_strikes_no_new_attack() {
    no_new_attack(POLY, Join::Tie);
}

#[test]
fn a_slide_into_a_new_pitch_on_a_polyphonic_core_strikes_no_new_attack() {
    no_new_attack(POLY, Join::Slide);
}
