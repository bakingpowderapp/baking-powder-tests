use bp::arp::Arp;
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params as CoreParams;
use bp::core::params::Envelope;
use bp::fx::params::{Filter, Reverb};
use bp::fx::{Kind, Params, Slot};
use bp::project::{Project, Row, VELOCITY};
use support::{AUDIBLE, BLOCK, SAMPLE_RATE, core_layer, frames, midi, peak_of};

const LOW_KEY: u8 = 36;
const HIGH_KEY: u8 = 108;
const DECAY_S: f64 = 1.0;
const SIZE: f64 = 0.25;
const CROSSOVER_HZ: f64 = 300.0;
const SOFT_MS: f64 = 50.0;
const NOTE_SECONDS: f64 = 0.2;
const RENDER_SECONDS: f64 = 1.6;
const STEP_SECONDS: f64 = 0.1;
const LATE: (f64, f64) = (0.4, 1.4);
const EARLY: (f64, f64) = (0.3, 0.7);
const FULL_RATE: f64 = 60.0;
const RATE_TOLERANCE: f64 = 0.4;
const ALIKE: f64 = 0.3;
const HALVED_AT_LEAST: f64 = 1.5;
const QUARTERED_AT_LEAST: f64 = 2.0;
const FROZEN_DECAY_S: f64 = 0.1;
const FREEZE_AT: f64 = 0.5;
const FREEZE_RENDER_SECONDS: f64 = 1.5;
const BURST: f32 = 2.0;

fn sine() -> CoreParams {
    let mut patch = CoreParams::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.amp = Envelope { attack_ms: SOFT_MS, decay_ms: 1.0, sustain: 1.0, release_ms: SOFT_MS, ..Envelope::DEFAULT };
    patch
}

fn hall(decay_s: f64, low_decay: f64, freeze: bool) -> Row {
    let params = Reverb { decay_s, size: SIZE, low_decay, low_crossover_hz: CROSSOVER_HZ, damping_hz: 20_000.0, filter1: Filter { hz: Filter::MIN_HZ, ..Filter::LOW_CUT }, filter2: Filter { hz: Filter::MAX_HZ, ..Filter::HIGH_CUT }, pre_delay_ms: 0.0, mod_depth: 0.0, freeze, ..Reverb::DEFAULT };
    Row::Effect(Slot { wet: 1.0, params: Params::Reverb(params), ..Slot::new(Kind::Reverb) })
}

fn project(hall: Row) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, sine())), hall];
    project
}

fn open(project: &Project) -> (Audio, bp::audio::Renderer) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(project, None, &[], &[]);
    (audio, renderer)
}

fn render_to(renderer: &mut bp::audio::Renderer, out: &mut Vec<f32>, seconds: f64) {
    let end = frames(seconds);
    let (mut left, mut right) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    while out.len() < end {
        let n = BLOCK.min(end - out.len());
        renderer.render(&mut left[..n], &mut right[..n]);
        out.extend_from_slice(&left[..n]);
    }
}

fn tail(key: u8, low_decay: f64) -> Vec<f32> {
    let (audio, mut renderer) = open(&project(hall(DECAY_S, low_decay, false)));
    audio.note_on(0, key, VELOCITY);
    let mut out = Vec::new();
    render_to(&mut renderer, &mut out, NOTE_SECONDS);
    audio.note_off(0, key);
    render_to(&mut renderer, &mut out, RENDER_SECONDS);
    out
}

fn rms_db(samples: &[f32], from: f64) -> f64 {
    let span = &samples[frames(from)..frames(from + STEP_SECONDS)];
    let mean_square = span.iter().map(|&x| f64::from(x) * f64::from(x)).sum::<f64>() / span.len() as f64;
    10.0 * mean_square.log10()
}

fn rate(samples: &[f32], (from, to): (f64, f64)) -> f64 {
    let points: Vec<(f64, f64)> = (0..).map(|k| from + k as f64 * STEP_SECONDS).take_while(|at| *at < to).map(|at| (at, rms_db(samples, at))).collect();
    let n = points.len() as f64;
    let (mean_t, mean_db) = (points.iter().map(|(t, _)| t).sum::<f64>() / n, points.iter().map(|(_, db)| db).sum::<f64>() / n);
    let slope = points.iter().map(|(t, db)| (t - mean_t) * (db - mean_db)).sum::<f64>() / points.iter().map(|(t, _)| (t - mean_t).powi(2)).sum::<f64>();
    -slope
}

fn expect_rate(got: f64, wanted: f64, what: &str) {
    assert!((got - wanted).abs() < RATE_TOLERANCE * wanted, "wanted {what} to fall about {wanted:.0} dB a second, got {got:.1}");
}

#[test]
fn at_times_one_the_bass_and_the_top_decay_alike() {
    let low = rate(&tail(LOW_KEY, 1.0), LATE);
    let high = rate(&tail(HIGH_KEY, 1.0), LATE);
    expect_rate(high, FULL_RATE, "a 4 kHz tail at a decay of 1 s");
    assert!((low - high).abs() < ALIKE * high, "wanted a 65 Hz tail to fall as fast as the 4 kHz one at times 1, got {low:.1} against {high:.1} dB a second");
}

#[test]
fn at_times_four_the_bass_rings_four_times_as_long() {
    let low = rate(&tail(LOW_KEY, 4.0), LATE);
    let high = rate(&tail(HIGH_KEY, 4.0), LATE);
    expect_rate(high, FULL_RATE, "a 4 kHz tail with the bass at times 4");
    expect_rate(low, FULL_RATE / 4.0, "a 65 Hz tail with the bass at times 4");
}

#[test]
fn under_one_the_bass_dies_faster_and_the_top_keeps_its_time() {
    let whole = rate(&tail(LOW_KEY, 1.0), LATE);
    let halved = rate(&tail(LOW_KEY, 0.5), LATE);
    assert!(halved > HALVED_AT_LEAST * whole, "wanted a 65 Hz tail with the bass at times 0.5 to fall at least {HALVED_AT_LEAST} times as fast as at times 1, got {halved:.1} against {whole:.1} dB a second");
    let quartered = rate(&tail(LOW_KEY, 0.25), EARLY);
    assert!(quartered > QUARTERED_AT_LEAST * whole, "wanted a 65 Hz tail with the bass at a quarter to fall at least {QUARTERED_AT_LEAST} times as fast as at times 1 over its first half second, got {quartered:.1} against {whole:.1} dB a second");
    for times in [0.5, 0.25] {
        expect_rate(rate(&tail(HIGH_KEY, times), LATE), FULL_RATE, &format!("a 4 kHz tail with the bass at times {times}"));
    }
}

#[test]
fn freezing_a_short_decay_with_a_long_bass_never_bursts() {
    let (mut audio, mut renderer) = open(&project(hall(FROZEN_DECAY_S, 4.0, false)));
    audio.note_on(0, LOW_KEY, VELOCITY);
    let mut out = Vec::new();
    render_to(&mut renderer, &mut out, FREEZE_AT);
    let before = peak_of(&out);
    assert!(before > AUDIBLE, "wanted the hall to sound before the freeze, got a peak of {before}");
    audio.sync(&project(hall(FROZEN_DECAY_S, 4.0, true)), None, &[], &[]);
    render_to(&mut renderer, &mut out, FREEZE_RENDER_SECONDS);
    let loudest = peak_of(&out[frames(FREEZE_AT)..]);
    assert!(loudest < BURST * before, "wanted the freeze at a decay of {FROZEN_DECAY_S} s and bass times 4 to hold the tail near its level, got a peak of {loudest} against {before} before it");
}
