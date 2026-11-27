use bp::fx::params::{FreakSlope, Suppressor};
use bp::fx::{Clock, Effect, Kind, Params, Slot};
use std::f64::consts::TAU;
use support::BLOCK;

const SAMPLE_RATE: f64 = 44_100.0;
const BPM: f64 = 120.0;
const TOP_HZ: f64 = 20_000.0;
const HZ: f64 = 440.0;
const LEVEL: f32 = 0.1;
const BLOCKS: usize = 40;

fn linear(bands: usize) -> Slot {
    let mut params = Suppressor::DEFAULT;
    params.band_count = bands;
    params.crossovers_hz[0] = TOP_HZ;
    params.slope = FreakSlope::Linear;
    Slot { params: Params::Suppressor(params), ..Slot::new(Kind::Suppressor) }
}

fn through(effect: &mut Effect, slot: &Slot, from: usize) -> Vec<f32> {
    let clock = Clock { sample_rate: SAMPLE_RATE, bpm: BPM, beat: None };
    let mut out = Vec::with_capacity(BLOCKS * BLOCK);
    for block in from..from + BLOCKS {
        let mut l: Vec<f32> = (block * BLOCK..(block + 1) * BLOCK).map(|i| LEVEL * (TAU * HZ * i as f64 / SAMPLE_RATE).sin() as f32).collect();
        let mut r = l.clone();
        effect.process(&mut l, &mut r, slot, &clock);
        out.extend_from_slice(&l);
    }
    out
}

#[test]
fn a_third_linear_band_coming_on_under_a_top_crossover_renders_finite_samples() {
    let mut effect = Effect::new(Kind::Suppressor, SAMPLE_RATE, BLOCK);
    let two = through(&mut effect, &linear(2), 0);
    assert!(two.iter().all(|sample| sample.is_finite()), "wanted two bands with their crossover at {TOP_HZ} Hz to render finite samples at {SAMPLE_RATE} Hz");
    let three = through(&mut effect, &linear(3), BLOCKS);
    if let Some(frame) = three.iter().position(|sample| !sample.is_finite()) {
        panic!("wanted every sample finite after a third linear-phase band came on under a crossover at {TOP_HZ} Hz, got a non-finite sample {frame} frames after the change");
    }
}
