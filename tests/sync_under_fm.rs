use bp::core::params::Osc;
use bp::core::{Core, Params};
use support::{BLOCK, SAMPLE_RATE, amplitude, frames};

const KEY: u8 = 60;
const TRITONE: f64 = 6.0;
const FM: f64 = 0.5;
const SETTLE_SECONDS: f64 = 0.2;
const WINDOW: usize = 8_192;
const BIN_HZ: f64 = 25.0;
const TOP_BAND_HZ: f64 = 20_000.0;
const NYQUIST_HZ: f64 = 24_000.0;
const CLEAN: f64 = 5e-4;

fn synced_sine_under_fm(fm: f64) -> Params {
    let mut patch = Params::DEFAULT;
    patch.osc1 = Osc { levels: [0.0, 0.0, 0.0, 1.0], tune: TRITONE, sync: true, fm, ..Osc::with_level(1.0) };
    patch.osc2 = Osc { levels: [0.0, 0.0, 1.0, 0.0], ..Osc::with_level(0.0) };
    patch
}

fn band_power(window: &[f32], from_hz: f64, to_hz: f64) -> f64 {
    let mut power = 0.0;
    let mut hz = from_hz;
    while hz < to_hz {
        power += amplitude(window, hz).powi(2);
        hz += BIN_HZ;
    }
    power
}

pub(crate) fn top_band_share_of(fm: f64, top_hz: f64) -> f64 {
    let mut core = Core::new(SAMPLE_RATE, synced_sine_under_fm(fm));
    core.note_on(KEY, 127);
    let end = frames(SETTLE_SECONDS) + WINDOW;
    let mut out = Vec::with_capacity(end);
    let (mut left, mut right) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    while out.len() < end {
        let n = BLOCK.min(end - out.len());
        core.render(&mut left[..n], &mut right[..n]);
        out.extend_from_slice(&left[..n]);
    }
    let window = &out[frames(SETTLE_SECONDS)..];
    band_power(window, top_hz, NYQUIST_HZ) / band_power(window, BIN_HZ, NYQUIST_HZ)
}

fn top_band_share() -> f64 {
    top_band_share_of(FM, TOP_BAND_HZ)
}

#[test]
fn a_synced_sine_under_fm_keeps_its_resets_out_of_the_top_band() {
    let share = top_band_share();
    assert!(share < CLEAN, "wanted a sine hard-synced {TRITONE} semitones over its FM modulator to carry under {CLEAN} of its power above {TOP_BAND_HZ} Hz, as it does when each reset's step is measured at the phase heard, got {share:.6}");
}
