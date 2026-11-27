use bp::core::params::{Modulation, Osc};
use bp::core::{Core, Params};
use support::{BLOCK, SAMPLE_RATE, frames, peak_of};

const TOP: u8 = 127;
const HIGH: u8 = 103;
const TWO_OCTAVES: f64 = 24.0;
const RENDER_SECONDS: f64 = 1.0;
const BOUND: f32 = 1.0;

fn two_octaves_up() -> Params {
    let mut patch = Params::DEFAULT;
    patch.osc1.tune = TWO_OCTAVES;
    patch
}

fn two_octaves_up_and_two_more_by_mod() -> Params {
    let mut patch = Params::DEFAULT;
    patch.osc1.level = 0.0;
    patch.osc2 = Osc { tune: TWO_OCTAVES, ..Osc::with_level(1.0) };
    patch.modulation = Modulation { tune2: TWO_OCTAVES, sampled: true, sample: 1.0, ..Modulation::DEFAULT };
    patch
}

pub(crate) fn held(patch: Params, midi: u8) -> (usize, f32) {
    let mut core = Core::new(SAMPLE_RATE, patch);
    core.note_on(midi, 127);
    let mut out = Vec::with_capacity(frames(RENDER_SECONDS));
    let (mut left, mut right) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    while out.len() < frames(RENDER_SECONDS) {
        let n = BLOCK.min(frames(RENDER_SECONDS) - out.len());
        core.render(&mut left[..n], &mut right[..n]);
        out.extend_from_slice(&left[..n]);
    }
    (out.iter().filter(|sample| !sample.is_finite()).count(), peak_of(&out))
}

#[test]
fn the_top_note_two_octaves_up_stays_a_bounded_oscillator() {
    let (non_finite, peak) = held(two_octaves_up(), TOP);
    assert!(non_finite == 0 && peak <= BOUND, "wanted MIDI {TOP} with TUNE +{TWO_OCTAVES} held under the increment ceiling for {RENDER_SECONDS} s, every sample finite and the peak under {BOUND}, got {non_finite} non-finite samples and a peak of {peak}");
}

#[test]
fn tune_and_mod_tune_stacked_on_a_high_note_stay_bounded() {
    let (non_finite, peak) = held(two_octaves_up_and_two_more_by_mod(), HIGH);
    assert!(non_finite == 0 && peak <= BOUND, "wanted MIDI {HIGH} with TUNE +{TWO_OCTAVES} and MOD tune +{TWO_OCTAVES} at a full MOD held under the increment ceiling for {RENDER_SECONDS} s, every sample finite and the peak under {BOUND}, got {non_finite} non-finite samples and a peak of {peak}");
}
