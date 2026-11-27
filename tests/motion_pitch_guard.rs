use bp::core::params::{Envelope, Lfo, Mover};
use bp::core::{Core, Params, Shape, Target};
use support::{AUDIBLE, BLOCK, SAMPLE_RATE, frames, peak_of};

const TOP: u8 = 120;
const LOW: u8 = 48;
const LOW_HZ: f64 = 130.8128;
const RAISED: f64 = 8.0;
const SLOW_HZ: f64 = 0.05;
const RENDER_SECONDS: f64 = 1.0;
const SETTLED_SECONDS: f64 = 0.5;
const BOUND: f32 = 1.0;
const NEAR_HZ: f64 = 20.0;

fn three_octaves_up() -> Params {
    let mut patch = Params::DEFAULT;
    let held = Envelope { attack_ms: 1.0, sustain: 1.0, ..Envelope::MOTION };
    patch.motion.env1 = Mover { target: Target::Pitch, amount: 1.0, env: held };
    patch.motion.env2 = Mover { target: Target::Pitch, amount: 1.0, env: held };
    patch.motion.lfo1 = Lfo { target: Target::Pitch, amount: 1.0, shape: Shape::Square, rate_hz: SLOW_HZ, ..Lfo::DEFAULT };
    patch
}

fn held(midi: u8) -> Vec<f32> {
    let mut core = Core::new(SAMPLE_RATE, three_octaves_up());
    core.note_on(midi, 127);
    let mut out = Vec::with_capacity(frames(RENDER_SECONDS));
    let (mut left, mut right) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    while out.len() < frames(RENDER_SECONDS) {
        let n = BLOCK.min(frames(RENDER_SECONDS) - out.len());
        core.render(&mut left[..n], &mut right[..n]);
        out.extend_from_slice(&left[..n]);
    }
    out
}

fn dominant_hz(samples: &[f32]) -> f64 {
    let crossings = samples.windows(2).filter(|pair| (pair[0] < 0.0) != (pair[1] < 0.0)).count();
    crossings as f64 / 2.0 / (samples.len() as f64 / SAMPLE_RATE)
}

#[test]
fn three_full_pitch_motions_on_a_top_note_stay_a_bounded_oscillator() {
    let out = held(TOP);
    let non_finite = out.iter().filter(|sample| !sample.is_finite()).count();
    let peak = peak_of(&out);
    assert!(non_finite == 0 && peak <= BOUND, "wanted MIDI {TOP} raised three octaves by two motion envelopes and an LFO held under the frequency guard, with every sample finite and a peak under {BOUND}, got {non_finite} non-finite samples and a peak of {peak}");
}

#[test]
fn the_same_motions_on_a_low_note_play_it_three_octaves_up() {
    let out = held(LOW);
    let settled = &out[frames(SETTLED_SECONDS)..];
    assert!(peak_of(settled) > AUDIBLE, "wanted the raised note audible from {SETTLED_SECONDS} s, got a peak of {}", peak_of(settled));
    let got = dominant_hz(settled);
    let wanted = LOW_HZ * RAISED;
    assert!((got - wanted).abs() < NEAR_HZ, "wanted MIDI {LOW} to sound at {wanted:.1} Hz under 36 semitones of motion from {SETTLED_SECONDS} s, got {got:.1} Hz");
}
