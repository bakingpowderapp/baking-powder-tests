use bp::arp::Arp;
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params as CoreParams;
use bp::core::params::Envelope;
use bp::fx::Slot;
use bp::fx::params::motion::MotionBand;
use bp::fx::params::{Chorus, ChorusMode, MotionMode};
use bp::project::{Project, Row, VELOCITY};
use support::{AUDIBLE, BLOCK, SAMPLE_RATE, SILENT, core_layer, midi, one_band, peak_of};

const KEY: u8 = 60;
const CENTER_MS: f64 = 40.0;
const FAR: f64 = 1.5;
const SLOW_HZ: f64 = 0.05;
const RENDER_SECONDS: f64 = 0.1;
const BEFORE_MS: f64 = 36.0;
const COPY_MS: f64 = 7.0;
const QUIET_MS: (f64, f64) = (54.0, 58.0);
const QUIET_SHARE: f32 = 0.08;
const ALIKE: f32 = 0.25;
const DUAL_LAG: f64 = 0.13;
const THIRD_PAIR_TURN: f64 = 2.0 / 3.0;
const ENSEMBLE_PHASE: f64 = 1.25 - THIRD_PAIR_TURN - DUAL_LAG;
const ENSEMBLE_SILENT_MS: f64 = 29.0;
const ENSEMBLE_TROUGH_MS: (f64, f64) = (66.0, 69.0);
const ENSEMBLE_FARTHEST_MS: (f64, f64) = (70.3, 72.3);
const CLEARLY: f32 = 2.0;

fn frames(ms: f64) -> usize {
    (ms * 0.001 * SAMPLE_RATE).round() as usize
}

fn click() -> CoreParams {
    let mut patch = CoreParams::DEFAULT;
    patch.osc1.level = 0.0;
    patch.noise.on = true;
    patch.noise.level = 1.0;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 0.0, release_ms: 1.0, ..Envelope::DEFAULT };
    patch
}

fn dual(mode: ChorusMode, depth: f64, start_phase: f64) -> Row {
    let chorus = Chorus { mode, dual: true, depth, center_ms: CENTER_MS, spread: 0.0, start_phase, rate_hz: SLOW_HZ, feedback: 0.0, width: 1.0, crossfade: true, ..Chorus::DEFAULT };
    Row::Effect(Slot { wet: 1.0, ..one_band(MotionBand { mode: MotionMode::Chorus, chorus, ..MotionBand::DEFAULT }) })
}

fn rendered(chorus: Row) -> Vec<f32> {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, click())), chorus];
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(&project, None, &[], &[]);
    audio.note_on(0, KEY, VELOCITY);
    let mut out = Vec::new();
    let total = frames(RENDER_SECONDS * 1000.0);
    let (mut left, mut right) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    while out.len() < total {
        let n = BLOCK.min(total - out.len());
        renderer.render(&mut left[..n], &mut right[..n]);
        out.extend_from_slice(&left[..n]);
    }
    out
}

fn peak_between(samples: &[f32], from_ms: f64, to_ms: f64) -> f32 {
    peak_of(&samples[frames(from_ms)..frames(to_ms)])
}

#[test]
fn dual_reads_at_the_centre_and_one_and_a_half_times_it_alike() {
    let out = rendered(dual(ChorusMode::Standard, 0.0, 0.0));
    let before = peak_between(&out, 0.0, BEFORE_MS);
    assert!(before <= SILENT, "wanted nothing before the first read at {CENTER_MS} ms, got a peak of {before}");
    let first = peak_between(&out, CENTER_MS - 1.0, CENTER_MS + COPY_MS);
    assert!(first > AUDIBLE, "wanted the click read at {CENTER_MS} ms, got a peak of {first}");
    let quiet = peak_between(&out, QUIET_MS.0, QUIET_MS.1);
    assert!(quiet < QUIET_SHARE * first, "wanted the first read's click gone between {} and {} ms, got {quiet} against its peak {first}", QUIET_MS.0, QUIET_MS.1);
    let far_ms = FAR * CENTER_MS;
    let second = peak_between(&out, far_ms - 1.0, far_ms + COPY_MS);
    assert!((second / first - 1.0).abs() < ALIKE, "wanted the second read at {far_ms} ms as loud as the first within {ALIKE}, got {second} against {first}");
}

#[test]
fn ensemble_dual_reaches_its_farthest_sweep_at_full_depth() {
    let out = rendered(dual(ChorusMode::Ensemble, 1.0, ENSEMBLE_PHASE));
    let before = peak_between(&out, 0.0, ENSEMBLE_SILENT_MS);
    assert!(before <= SILENT, "wanted nothing before the earliest read, got a peak of {before} inside {ENSEMBLE_SILENT_MS} ms");
    let trough = peak_between(&out, ENSEMBLE_TROUGH_MS.0, ENSEMBLE_TROUGH_MS.1);
    let farthest = peak_between(&out, ENSEMBLE_FARTHEST_MS.0, ENSEMBLE_FARTHEST_MS.1);
    assert!(farthest > AUDIBLE, "wanted the farthest read, 1.5 times {} ms plus a 6 ms sweep, to land at 70.5 ms, got a peak of {farthest}", CENTER_MS + 3.0);
    assert!(farthest > CLEARLY * trough, "wanted the read at 70.5 ms to stand clear of the lull before it, got {farthest} against {trough} between {} and {} ms", ENSEMBLE_TROUGH_MS.0, ENSEMBLE_TROUGH_MS.1);
}
