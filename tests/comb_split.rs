use bp::core::params::Envelope;
use bp::core::{Core, FilterMode, Params};
use support::{BLOCK, SAMPLE_RATE, frames, peak_of};

const KEY: u8 = 60;
const KEY_HZ: f64 = 261.6256;
const RESONANCE: f64 = 0.9;
const NOISE_LEVEL: f64 = 0.5;
const COUPLED_SECONDS: f64 = 0.5;
const SPLIT_SECONDS: f64 = 0.05;
const ALIKE: f32 = 1e-4;

fn within_the_first_period() -> usize {
    (SAMPLE_RATE / KEY_HZ).floor() as usize - 1
}

fn ringing(filtered: bool) -> Params {
    let mut patch = Params::DEFAULT;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    patch.filter.on = filtered;
    patch.filter.mode = FilterMode::Comb;
    patch.filter.cutoff_hz = KEY_HZ;
    patch.filter.resonance = RESONANCE;
    patch
}

fn with_stereo_noise(mut patch: Params) -> Params {
    patch.noise.on = true;
    patch.noise.level = NOISE_LEVEL;
    patch.noise.width = 1.0;
    patch
}

fn render(core: &mut Core, left: &mut Vec<f32>, right: &mut Vec<f32>, end: usize) {
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    while left.len() < end {
        let n = BLOCK.min(end - left.len());
        core.render(&mut l[..n], &mut r[..n]);
        left.extend_from_slice(&l[..n]);
        right.extend_from_slice(&r[..n]);
    }
}

fn split_by_noise(filtered: bool) -> (Vec<f32>, Vec<f32>) {
    let alone = ringing(filtered);
    let mut core = Core::new(SAMPLE_RATE, alone);
    core.note_on(KEY, 127);
    let (mut left, mut right) = (Vec::new(), Vec::new());
    render(&mut core, &mut left, &mut right, frames(COUPLED_SECONDS));
    core.set_params(with_stereo_noise(alone));
    render(&mut core, &mut left, &mut right, frames(COUPLED_SECONDS + SPLIT_SECONDS));
    (left, right)
}

pub(crate) fn stereo_gap_after_the_split() -> (f32, f32) {
    let (comb_l, comb_r) = split_by_noise(true);
    let (dry_l, dry_r) = split_by_noise(false);
    let from = frames(COUPLED_SECONDS);
    let gap = (from..from + within_the_first_period()).map(|i| ((comb_r[i] - comb_l[i]) - (dry_r[i] - dry_l[i])).abs()).fold(0.0f32, f32::max);
    (gap, peak_of(&comb_l[from..]) / peak_of(&dry_l[from..]))
}

#[test]
fn a_ringing_comb_that_turns_stereo_keeps_its_history_on_both_sides() {
    let (gap, ring) = stereo_gap_after_the_split();
    assert!(ring > 1.0, "wanted the comb at resonance {RESONANCE} ringing louder than the unfiltered voice after frame {}, got {ring} times its peak", frames(COUPLED_SECONDS));
    assert!(gap < ALIKE, "wanted the comb's left-right difference over the first period after stereo noise joins at frame {} to match the unfiltered voice's within {ALIKE}, as it does when both delay lines carry the same history, got {gap}", frames(COUPLED_SECONDS));
}
