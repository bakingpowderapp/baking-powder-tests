use bp::core::params::Envelope;
use bp::core::{Core, Params};
use support::{AUDIBLE, BLOCK, SAMPLE_RATE, frames, peak_of};

const VOICES: usize = 2;
const LOW: u8 = 48;
const MIDDLE: u8 = 60;
const HIGH: u8 = 72;
const SLOW_ATTACK_MS: f64 = 1000.0;
const DELAY_MS: f64 = 300.0;
const RELEASE_MS: f64 = 50.0;
const RENDER_SECONDS: f64 = 1.0;
const EARLY_STEAL_SECONDS: f64 = 0.1;
const LATER: (f64, f64) = (0.7, 0.8);
const TWO_VOICES: f32 = 0.28;

fn swelling(delay_ms: f64, attack_ms: f64) -> Params {
    let mut patch = Params::DEFAULT;
    patch.voices = VOICES;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.amp = Envelope { delay_ms, attack_ms, decay_ms: 1.0, sustain: 1.0, release_ms: RELEASE_MS, ..Envelope::DEFAULT };
    patch
}

fn render_until(core: &mut Core, out: &mut Vec<f32>, end: usize) {
    let (mut left, mut right) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    while out.len() < end {
        let n = BLOCK.min(end - out.len());
        core.render(&mut left[..n], &mut right[..n]);
        out.extend_from_slice(&left[..n]);
    }
}

fn two_held_then_a_third(patch: Params, steal_seconds: f64) -> Vec<f32> {
    let mut core = Core::new(SAMPLE_RATE, patch);
    core.note_on(LOW, 127);
    core.note_on(MIDDLE, 127);
    let mut out = Vec::with_capacity(frames(RENDER_SECONDS));
    render_until(&mut core, &mut out, frames(steal_seconds));
    core.note_on(HIGH, 127);
    render_until(&mut core, &mut out, frames(RENDER_SECONDS));
    out
}

fn peak_between(samples: &[f32], from: f64, to: f64) -> f32 {
    peak_of(&samples[frames(from)..frames(to)])
}

#[test]
fn a_voice_stolen_during_its_delay_never_starts_its_attack() {
    let out = two_held_then_a_third(swelling(DELAY_MS, SLOW_ATTACK_MS), EARLY_STEAL_SECONDS);
    let heard = peak_between(&out, LATER.0, LATER.1);
    assert!(heard > AUDIBLE, "wanted the surviving voice and the third note swelling between {} and {} s, got a peak of {heard}", LATER.0, LATER.1);
    assert!(heard < TWO_VOICES, "wanted only the two voices that survived the steal at frame {} sounding between {} and {} s, under {TWO_VOICES}, got {heard}", frames(EARLY_STEAL_SECONDS), LATER.0, LATER.1);
}
