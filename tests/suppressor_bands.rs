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
const NOISE_LEVEL: f32 = 0.02;
const RESONANCE_LEVEL: f32 = 0.006;
const LOW_RESONANCE_HZ: f64 = 300.0;
const HIGH_RESONANCE_HZ: f64 = 3000.0;
const DEEP: f64 = 8.0;
const CUT_DB: f64 = 2.0;
const UNCUT_DB: f64 = 0.3;

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

fn sines(frames: usize, tones: &[(f64, f32)]) -> Vec<f32> {
    (0..frames).map(|i| tones.iter().map(|(hz, level)| level * (TAU * hz * i as f64 / SAMPLE_RATE).sin() as f32).sum()).collect()
}

fn pink(frames: usize) -> Vec<f32> {
    let mut state = 0x9E37_79B9u32;
    let mut rows = [0.0f32; 8];
    (0..frames)
        .map(|i| {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            rows[(i.trailing_zeros() as usize).min(rows.len() - 1)] = state as f32 / u32::MAX as f32 * 2.0 - 1.0;
            rows.iter().sum::<f32>() / rows.len() as f32 * NOISE_LEVEL
        })
        .collect()
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

fn cut_db(dry: &[f32], wet: &[f32], hz: f64) -> f64 {
    20.0 * (level_at(dry, hz) / level_at(wet, hz).max(1e-12)).log10()
}

fn frames() -> usize {
    (SECONDS * SAMPLE_RATE) as usize
}

#[test]
fn a_muted_band_removes_its_range_and_leaves_the_other() {
    let dry = sines(frames(), &[(LOW_HZ, LEVEL), (HIGH_HZ, LEVEL)]);
    let wet = through(&two_bands(|p| p.bands[0].mute = true), &dry);
    let (low, high) = (cut_db(&dry, &wet, LOW_HZ), cut_db(&dry, &wet, HIGH_HZ));
    assert!(low > GONE_DB, "wanted the low band muted to take {LOW_HZ} Hz down by more than {GONE_DB} dB, got {low:.1} dB");
    assert!(high.abs() < KEPT_DB, "wanted {HIGH_HZ} Hz kept within {KEPT_DB} dB with only the low band muted, got {high:.2} dB");
}

#[test]
fn a_soloed_band_keeps_only_its_range() {
    let dry = sines(frames(), &[(LOW_HZ, LEVEL), (HIGH_HZ, LEVEL)]);
    let wet = through(&two_bands(|p| p.bands[1].solo = true), &dry);
    let (low, high) = (cut_db(&dry, &wet, LOW_HZ), cut_db(&dry, &wet, HIGH_HZ));
    assert!(low > GONE_DB, "wanted the high band soloed to take {LOW_HZ} Hz down by more than {GONE_DB} dB, got {low:.1} dB");
    assert!(high.abs() < KEPT_DB, "wanted {HIGH_HZ} Hz kept within {KEPT_DB} dB under its own solo, got {high:.2} dB");
}

#[test]
fn an_off_band_leaves_its_resonance_while_the_other_band_cuts_its_own() {
    let bed = pink(frames());
    let dry: Vec<f32> = bed.iter().zip(sines(frames(), &[(LOW_RESONANCE_HZ, RESONANCE_LEVEL), (HIGH_RESONANCE_HZ, RESONANCE_LEVEL)])).map(|(noise, tone)| noise + tone).collect();
    let wet = through(
        &two_bands(|p| {
            p.bands[0].on = false;
            p.bands[0].depth = DEEP;
            p.bands[1].depth = DEEP;
        }),
        &dry,
    );
    let (low, high) = (cut_db(&dry, &wet, LOW_RESONANCE_HZ), cut_db(&dry, &wet, HIGH_RESONANCE_HZ));
    assert!(low.abs() < UNCUT_DB, "wanted the resonance at {LOW_RESONANCE_HZ} Hz left within {UNCUT_DB} dB under a band set off, got {low:.2} dB");
    assert!(high > CUT_DB, "wanted the resonance at {HIGH_RESONANCE_HZ} Hz cut by more than {CUT_DB} dB by the band that is on, got {high:.2} dB");
}
