use bp::arp::Arp;
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params as CoreParams;
use bp::core::params::Envelope;
use bp::fx::params::Reverb;
use bp::fx::{Kind, Params, Slot};
use bp::project::{Project, Row, VELOCITY};
use support::{AUDIBLE, BLOCK, SAMPLE_RATE, core_layer, frames, midi};

const KEY: u8 = 69;
const DECAY_S: f64 = 2.0;
const PRE_DELAY_MS: f64 = 20.0;
const RENDER_SECONDS: f64 = 2.5;
const STEADY_FROM: f64 = 2.0;
const EARLY_SECONDS: f64 = 0.05;
const SHORT_WINDOW_MS: f64 = 100.0;
const LONG_WINDOW_MS: f64 = 800.0;
const LEVEL_TOLERANCE_DB: f64 = 0.5;
const SHORT_WITHHELD: f64 = 0.5;
const LONG_WITHHELD: f64 = 0.1;

fn sine() -> CoreParams {
    let mut patch = CoreParams::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    patch
}

fn hall(attack_ms: f64) -> Row {
    let params = Reverb { decay_s: DECAY_S, pre_delay_ms: PRE_DELAY_MS, mod_depth: 0.0, attack_ms, ..Reverb::DEFAULT };
    Row::Effect(Slot { wet: 1.0, params: Params::Reverb(params), ..Slot::new(Kind::Reverb) })
}

fn rendered(attack_ms: f64, until: f64) -> Vec<f32> {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, sine())), hall(attack_ms)];
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(&project, None, &[], &[]);
    audio.note_on(0, KEY, VELOCITY);
    let mut out = Vec::new();
    let (mut left, mut right) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    while out.len() < frames(until) {
        let n = BLOCK.min(frames(RENDER_SECONDS) - out.len());
        renderer.render(&mut left[..n], &mut right[..n]);
        out.extend_from_slice(&left[..n]);
    }
    out
}

fn rms(samples: &[f32], from: f64, to: f64) -> f64 {
    let span = &samples[frames(from)..frames(to)];
    (span.iter().map(|&x| f64::from(x) * f64::from(x)).sum::<f64>() / span.len() as f64).sqrt()
}

#[test]
fn the_attack_keeps_the_held_tail_at_the_level_of_no_attack() {
    let plain = rendered(0.0, RENDER_SECONDS);
    let steady = rms(&plain, STEADY_FROM, RENDER_SECONDS);
    assert!(steady > f64::from(AUDIBLE), "wanted the hall to sound under a held note, got an rms of {steady:.5}");
    for window_ms in [SHORT_WINDOW_MS, LONG_WINDOW_MS] {
        let bloomed = rendered(window_ms, RENDER_SECONDS);
        let gap = 20.0 * (rms(&bloomed, STEADY_FROM, RENDER_SECONDS) / steady).log10();
        assert!(gap.abs() < LEVEL_TOLERANCE_DB, "wanted an attack of {window_ms} ms to leave the held tail at the level of none from {STEADY_FROM} s, got {gap:+.2} dB");
    }
}

#[test]
fn the_attack_withholds_the_tails_first_moments() {
    let early_from = PRE_DELAY_MS / 1000.0;
    let early_to = early_from + EARLY_SECONDS;
    let plain = rendered(0.0, early_to);
    let early = rms(&plain, early_from, early_to);
    assert!(early > f64::from(AUDIBLE), "wanted the hall without attack to speak right after its {PRE_DELAY_MS} ms pre-delay, got an rms of {early:.5}");
    for (window_ms, withheld) in [(SHORT_WINDOW_MS, SHORT_WITHHELD), (LONG_WINDOW_MS, LONG_WITHHELD)] {
        let share = rms(&rendered(window_ms, early_to), early_from, early_to) / early;
        assert!(share < withheld, "wanted an attack of {window_ms} ms to hold the first {EARLY_SECONDS} s of the tail under {withheld} of its level without attack, got {share:.3}");
    }
}
