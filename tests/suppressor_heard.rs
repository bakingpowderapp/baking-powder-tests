use bp::fx::params::Suppressor;
use bp::fx::{Clock, Effect, Kind, Params, Slot};
use std::f64::consts::TAU;
use support::{BLOCK, SAMPLE_RATE};

const BPM: f64 = 120.0;
const SECONDS: f64 = 2.0;
const LOW_HZ: f64 = 100.0;
const HIGH_HZ: f64 = 5000.0;
const LEVEL: f32 = 0.1;
const CROSSOVER_HZ: f64 = 800.0;
const SETTLED_FROM: f64 = 1.0;
const SETTLED_TO: f64 = 1.9;
const GONE_DB: f64 = 30.0;
const KEPT_DB: f64 = 1.0;

fn two_bands(tweak: impl FnOnce(&mut Suppressor)) -> Slot {
    let mut params = Suppressor::DEFAULT;
    params.band_count = 2;
    params.crossovers_hz[0] = CROSSOVER_HZ;
    for band in &mut params.bands {
        band.depth = 0.0;
    }
    tweak(&mut params);
    Slot { params: Params::Suppressor(params), ..Slot::new(Kind::Suppressor) }
}

fn sines(frames: usize) -> Vec<f32> {
    (0..frames).map(|i| [LOW_HZ, HIGH_HZ].iter().map(|hz| LEVEL * (TAU * hz * i as f64 / SAMPLE_RATE).sin() as f32).sum()).collect()
}

fn through(slot: &Slot, input: &[f32]) -> Vec<f32> {
    let mut effect = Effect::new(slot.kind(), SAMPLE_RATE, BLOCK);
    let clock = Clock { sample_rate: SAMPLE_RATE, bpm: BPM, beat: None };
    let mut output = Vec::with_capacity(input.len());
    for chunk in input.chunks(BLOCK) {
        let (mut l, mut r) = (chunk.to_vec(), chunk.to_vec());
        effect.process(&mut l, &mut r, slot, &clock);
        output.extend_from_slice(&l);
    }
    output
}

fn level_at(signal: &[f32], hz: f64) -> f64 {
    let (from, to) = ((SETTLED_FROM * SAMPLE_RATE) as usize, (SETTLED_TO * SAMPLE_RATE) as usize);
    let (mut re, mut im) = (0.0f64, 0.0f64);
    for (i, sample) in signal.iter().enumerate().take(to).skip(from) {
        let phase = TAU * hz * i as f64 / SAMPLE_RATE;
        re += f64::from(*sample) * phase.cos();
        im += f64::from(*sample) * phase.sin();
    }
    (re * re + im * im).sqrt()
}

fn cuts(tweak: impl FnOnce(&mut Suppressor)) -> (f64, f64) {
    let dry = sines((SECONDS * SAMPLE_RATE) as usize);
    let wet = through(&two_bands(tweak), &dry);
    let cut = |hz: f64| 20.0 * (level_at(&dry, hz) / level_at(&wet, hz).max(1e-12)).log10();
    (cut(LOW_HZ), cut(HIGH_HZ))
}

#[test]
fn a_solo_on_a_band_set_off_leaves_every_band_heard() {
    let (low, high) = cuts(|p| {
        p.bands[0].on = false;
        p.bands[0].solo = true;
    });
    assert!(low.abs() < KEPT_DB && high.abs() < KEPT_DB, "wanted a solo on a band set off to solo nothing, {LOW_HZ} and {HIGH_HZ} Hz both kept within {KEPT_DB} dB, got {low:.2} and {high:.2} dB");
}

#[test]
fn a_mute_on_a_band_set_off_leaves_its_range_heard() {
    let (low, high) = cuts(|p| {
        p.bands[0].on = false;
        p.bands[0].mute = true;
    });
    assert!(low.abs() < KEPT_DB && high.abs() < KEPT_DB, "wanted a mute on a band set off to mute nothing, {LOW_HZ} and {HIGH_HZ} Hz both kept within {KEPT_DB} dB, got {low:.2} and {high:.2} dB");
}

#[test]
fn a_soloed_band_that_is_on_silences_a_soloed_band_that_is_off() {
    let (low, high) = cuts(|p| {
        p.bands[0].on = false;
        p.bands[0].solo = true;
        p.bands[1].solo = true;
    });
    assert!(low > GONE_DB, "wanted the low band, off and soloed, silenced by the high band's solo, {LOW_HZ} Hz down by more than {GONE_DB} dB, got {low:.1} dB");
    assert!(high.abs() < KEPT_DB, "wanted {HIGH_HZ} Hz kept within {KEPT_DB} dB under the high band's own solo, got {high:.2} dB");
}
