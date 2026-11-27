use bp::fx::params::Suppressor;
use bp::fx::{Clock, Effect, Kind, Params, Slot};
use std::f64::consts::TAU;
use support::{BLOCK, SAMPLE_RATE};

const BPM: f64 = 120.0;
const LOOK_AHEAD: usize = 240;
const SECONDS: f64 = 3.0;
const SETTLED_FROM: f64 = 1.8;
const SETTLED_TO: f64 = 2.8;
const RESONANCE_HZ: f64 = 2000.0;
const PROBE_HZ: f64 = 2828.0;
const NOISE_LEVEL: f32 = 0.02;
const RESONANCE_LEVEL: f32 = 0.006;
const PROBE_LEVEL: f32 = 0.0003;
const CLICK: f32 = 0.25;
const CLICKS: [usize; 3] = [10, 500, 950];
const WITHIN: f32 = 1e-3;
const ONSET: f64 = 1.0;
const OFFSET: f64 = 2.0;
const WINDOW: usize = 480;
const QUIET_CUT_DB: f64 = 0.3;
const SKIRT_DB: f64 = 0.5;
const PLAIN_CUT_DB: f64 = 0.5;
const DEEP_CUT_DB: f64 = 2.0;
const SETTLED_SHARE: f64 = 0.6;
const FAST_MS: f64 = 30.0;
const SLOW_MS: f64 = 100.0;

fn suppressor(tweak: impl FnOnce(&mut Suppressor)) -> Slot {
    let mut params = Suppressor::DEFAULT;
    tweak(&mut params);
    Slot { params: Params::Suppressor(params), ..Slot::new(Kind::Suppressor) }
}

fn pink(frames: usize) -> Vec<f32> {
    let mut state = 0x9E37_79B9u32;
    let mut rows = [0.0f32; 8];
    let mut out = Vec::with_capacity(frames);
    for i in 0..frames {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let white = state as f32 / u32::MAX as f32 * 2.0 - 1.0;
        let row = (i.trailing_zeros() as usize).min(rows.len() - 1);
        rows[row] = white;
        out.push(rows.iter().sum::<f32>() / rows.len() as f32);
    }
    out
}

fn bed(frames: usize) -> Vec<f32> {
    pink(frames).into_iter().map(|x| x * NOISE_LEVEL).collect()
}

fn with_sine(mut signal: Vec<f32>, hz: f64, level: f32, from: f64, to: f64) -> Vec<f32> {
    let (from, to) = ((from * SAMPLE_RATE) as usize, (to * SAMPLE_RATE) as usize);
    for (i, sample) in signal.iter_mut().enumerate().take(to).skip(from) {
        *sample += level * (TAU * hz * i as f64 / SAMPLE_RATE).sin() as f32;
    }
    signal
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

fn level_at(signal: &[f32], hz: f64, from: usize, to: usize) -> f64 {
    let (mut re, mut im) = (0.0f64, 0.0f64);
    for (i, sample) in signal.iter().enumerate().take(to).skip(from) {
        let phase = TAU * hz * i as f64 / SAMPLE_RATE;
        re += f64::from(*sample) * phase.cos();
        im += f64::from(*sample) * phase.sin();
    }
    (re * re + im * im).sqrt() / (to - from) as f64
}

fn cut_db(dry: &[f32], wet: &[f32], hz: f64, from: f64, to: f64) -> f64 {
    let (from, to) = ((from * SAMPLE_RATE) as usize, (to * SAMPLE_RATE) as usize);
    let (dry_level, wet_level) = (level_at(dry, hz, from, to), level_at(&wet[LOOK_AHEAD..], hz, from, to));
    20.0 * (dry_level / wet_level).log10()
}

fn resonant() -> Vec<f32> {
    with_sine(bed((SECONDS * SAMPLE_RATE) as usize), RESONANCE_HZ, RESONANCE_LEVEL, 0.0, SECONDS)
}

fn gated() -> Vec<f32> {
    with_sine(bed((SECONDS * SAMPLE_RATE) as usize), RESONANCE_HZ, RESONANCE_LEVEL, ONSET, OFFSET)
}

fn cut_after(dry: &[f32], wet: &[f32], seconds: f64) -> f64 {
    let at = (seconds * SAMPLE_RATE) as usize;
    cut_db(dry, wet, RESONANCE_HZ, at as f64 / SAMPLE_RATE, (at + WINDOW) as f64 / SAMPLE_RATE)
}

#[test]
fn a_resonance_over_a_noise_bed_is_cut_and_the_bed_is_not() {
    let dry = resonant();
    let wet = through(&suppressor(|_| {}), &dry);
    let cut = cut_db(&dry, &wet, RESONANCE_HZ, SETTLED_FROM, SETTLED_TO);
    assert!(cut > DEEP_CUT_DB, "wanted the 2 kHz resonance cut by more than {DEEP_CUT_DB} dB at depth 5, got {cut:.2} dB");
    let plain = bed((SECONDS * SAMPLE_RATE) as usize);
    let plain_wet = through(&suppressor(|_| {}), &plain);
    for hz in [300.0, 1000.0, RESONANCE_HZ, 6000.0] {
        let bed_cut = cut_db(&plain, &plain_wet, hz, SETTLED_FROM, SETTLED_TO);
        assert!(bed_cut.abs() < PLAIN_CUT_DB, "wanted the plain noise bed left within {PLAIN_CUT_DB} dB at {hz} Hz, got {bed_cut:.2} dB");
    }
}

#[test]
fn depth_deepens_the_cut_and_zero_depth_cuts_nothing() {
    let dry = resonant();
    let mut last = f64::NEG_INFINITY;
    for depth in [0.0, 2.0, 5.0, 8.0, 10.0] {
        let wet = through(&suppressor(|p| p.bands[0].depth = depth), &dry);
        let cut = cut_db(&dry, &wet, RESONANCE_HZ, SETTLED_FROM, SETTLED_TO);
        if depth == 0.0 {
            assert!(cut.abs() < QUIET_CUT_DB, "wanted no cut at depth 0, got {cut:.2} dB");
        } else {
            assert!(cut > last, "wanted depth {depth} to cut more than the depth before ({last:.2} dB), got {cut:.2} dB");
        }
        last = cut;
    }
}

#[test]
fn detail_narrows_the_cut_around_the_resonance() {
    let dry = with_sine(resonant(), PROBE_HZ, PROBE_LEVEL, 0.0, SECONDS);
    let wide = through(&suppressor(|p| p.bands[0].detail = 0.0), &dry);
    let narrow = through(&suppressor(|p| p.bands[0].detail = 10.0), &dry);
    let (wide_probe, narrow_probe) = (cut_db(&dry, &wide, PROBE_HZ, SETTLED_FROM, SETTLED_TO), cut_db(&dry, &narrow, PROBE_HZ, SETTLED_FROM, SETTLED_TO));
    let (wide_cut, narrow_cut) = (cut_db(&dry, &wide, RESONANCE_HZ, SETTLED_FROM, SETTLED_TO), cut_db(&dry, &narrow, RESONANCE_HZ, SETTLED_FROM, SETTLED_TO));
    assert!(wide_probe > 2.0 * narrow_probe, "wanted the probe half an octave up cut much more at detail 0 than at detail 10, got {wide_probe:.2} dB against {narrow_probe:.2} dB");
    assert!(narrow_cut > wide_cut, "wanted the resonance itself cut deeper at detail 10 ({narrow_cut:.2} dB) than at detail 0 ({wide_cut:.2} dB)");
    assert!(narrow_probe < SKIRT_DB, "wanted the probe left within {SKIRT_DB} dB at detail 10, got {narrow_probe:.2} dB");
}

#[test]
fn attack_sets_how_fast_a_cut_lands() {
    let dry = gated();
    let fast = through(&suppressor(|p| p.bands[0].attack = 0.0), &dry);
    let slow = through(&suppressor(|p| p.bands[0].attack = 10.0), &dry);
    let settled = cut_db(&dry, &fast, RESONANCE_HZ, OFFSET - 0.3, OFFSET - 0.05);
    let fast_early = cut_after(&dry, &fast, ONSET + FAST_MS * 0.001);
    let slow_late = cut_after(&dry, &slow, ONSET + SLOW_MS * 0.001);
    assert!(fast_early > SETTLED_SHARE * settled, "wanted attack 0 at most of its {settled:.2} dB cut {FAST_MS} ms after the onset, got {fast_early:.2} dB");
    assert!(slow_late < SETTLED_SHARE * settled, "wanted attack 10 still short of its cut {SLOW_MS} ms after the onset, got {slow_late:.2} dB of {settled:.2} dB");
}

#[test]
fn release_sets_how_fast_a_cut_lets_go() {
    let dry = with_sine(gated(), RESONANCE_HZ, PROBE_LEVEL, OFFSET, SECONDS);
    let fast = through(&suppressor(|p| p.bands[0].release = 0.0), &dry);
    let slow = through(&suppressor(|p| p.bands[0].release = 10.0), &dry);
    let settled = cut_db(&dry, &slow, RESONANCE_HZ, OFFSET - 0.3, OFFSET - 0.05);
    let fast_after = cut_after(&dry, &fast, OFFSET + SLOW_MS * 0.001);
    let slow_after = cut_after(&dry, &slow, OFFSET + SLOW_MS * 0.001);
    assert!(fast_after < QUIET_CUT_DB, "wanted release 0 to let the cut go within {SLOW_MS} ms of the resonance ending, got {fast_after:.2} dB left");
    assert!(slow_after > SETTLED_SHARE * settled, "wanted release 10 to hold most of its {settled:.2} dB cut {SLOW_MS} ms after the resonance ended, got {slow_after:.2} dB");
}

#[test]
fn a_signal_with_nothing_to_cut_passes_delayed_by_the_look_ahead() {
    let slot = suppressor(|_| {});
    let reported = slot.latency(SAMPLE_RATE);
    assert_eq!(reported, LOOK_AHEAD, "wanted a Suppressor to report {LOOK_AHEAD} frames of latency at 48 kHz, got {reported}");
    let mut input = vec![0.0; 8 * BLOCK];
    for at in CLICKS {
        input[at] = CLICK;
    }
    let output = through(&slot, &input);
    let early: Vec<usize> = (0..CLICKS[0] + LOOK_AHEAD).filter(|&n| output[n] != 0.0).collect();
    assert!(early.is_empty(), "wanted silence until frame {}, got sound at frames {early:?}", CLICKS[0] + LOOK_AHEAD);
    for (n, got) in output.iter().enumerate() {
        let wanted = if n >= LOOK_AHEAD { input[n - LOOK_AHEAD] } else { 0.0 };
        assert!((got - wanted).abs() < WITHIN, "frame {n} wanted {wanted}, the input {LOOK_AHEAD} frames earlier, got {got}");
    }
}
