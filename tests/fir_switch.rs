use bp::fx::params::{Freak, FreakSlope};
use bp::fx::{Clock, Effect, Kind, Params, Slot};
use std::f64::consts::TAU;
use support::{BLOCK, SAMPLE_RATE, SILENT};

const BPM: f64 = 120.0;
const HZ: f64 = 220.0;
const LEVEL: f32 = 0.25;
const BEFORE: usize = 200;
const AFTER: usize = 20;
const HALF: usize = 431;
const HOLE: usize = 64;
const SPLICE: usize = 64;
const STEP_SLACK: f32 = 1.1;
const ALIKE: f32 = 1e-3;

fn sine(frames: usize) -> Vec<f32> {
    (0..frames).map(|n| (TAU * HZ * n as f64 / SAMPLE_RATE).sin() as f32 * LEVEL).collect()
}

fn soloed(slope: FreakSlope, oversample: bool) -> Slot {
    let mut freak = Freak { slope, oversample, ..Freak::DEFAULT };
    for band in &mut freak.bands {
        band.solo = true;
    }
    Slot { params: Params::Freak(freak), ..Slot::new(Kind::Freak) }
}

fn switched(before: &Slot, after: &Slot, at: usize, input: &[f32]) -> Vec<f32> {
    let mut effect = Effect::new(before.kind(), SAMPLE_RATE, BLOCK);
    let clock = Clock { sample_rate: SAMPLE_RATE, bpm: BPM, beat: None };
    let mut output = Vec::with_capacity(input.len());
    for (index, chunk) in input.chunks(BLOCK).enumerate() {
        let slot = if index < at { before } else { after };
        let (mut l, mut r) = (chunk.to_vec(), chunk.to_vec());
        effect.process(&mut l, &mut r, slot, &clock);
        output.extend_from_slice(&l);
    }
    output
}

fn largest_step(samples: &[f32]) -> (usize, f32) {
    samples.windows(2).enumerate().map(|(n, pair)| (n + 1, (pair[1] - pair[0]).abs())).fold((0, 0.0), |worst, step| if step.1 > worst.1 { step } else { worst })
}

fn longest_silence(samples: &[f32]) -> (usize, usize) {
    let (mut longest, mut run, mut start) = ((0, 0), 0, 0);
    for (n, sample) in samples.iter().enumerate() {
        if sample.abs() < SILENT {
            if run == 0 {
                start = n;
            }
            run += 1;
            if run > longest.1 {
                longest = (start, run);
            }
        } else {
            run = 0;
        }
    }
    longest
}

fn worst_stray(wanted: &[f32], output: &[f32], from: usize, by: usize) -> (usize, f32) {
    (from..output.len()).map(|n| (n, (output[n] - wanted[n - by]).abs())).fold((from, 0.0), |worst, stray| if stray.1 > worst.1 { stray } else { worst })
}

fn judge(what: &str, before: &Slot, after: &Slot, aligned: bool) {
    let input = sine((BEFORE + AFTER) * BLOCK);
    let output = switched(before, after, BEFORE, &input);
    let steady = switched(after, after, 0, &input);
    let switch = BEFORE * BLOCK;
    let own = largest_step(&input).1;
    let settled = &output[switch - BLOCK..switch];
    assert!(settled.iter().any(|sample| sample.abs() > LEVEL / 2.0), "wanted the {what} sounding on the 24 slope before the switch at frame {switch}");
    let (at, step) = largest_step(&output[switch + SPLICE..=switch + HALF]);
    assert!(step <= own * STEP_SLACK, "wanted every step from {SPLICE} frames after the {what} switched to LIN at frame {switch} to {HALF} within the sine's own largest step {own:.5}, got {step:.5} between frames {} and {}", switch + SPLICE + at - 1, switch + SPLICE + at);
    let (start, run) = longest_silence(&output[switch..switch + HALF]);
    assert!(run <= HOLE, "wanted no silence longer than {HOLE} frames after the {what} switched to LIN at frame {switch}, got {run} frames from frame {}", switch + start);
    let (frame, stray) = worst_stray(&steady, &output, switch + SPLICE, 0);
    assert!(stray < ALIKE, "wanted the {what} from {SPLICE} frames after the switch at frame {switch} to match the same row run on LIN from the start, frame {frame} strays {stray}");
    if aligned {
        let (frame, stray) = worst_stray(&input, &output, switch, HALF);
        assert!(stray < ALIKE, "wanted the {what} to put out the sine delayed by {HALF} from the switch at frame {switch} on, frame {frame} strays {stray}");
    }
}

#[test]
fn a_freak_switched_to_lin_plays_on_from_its_history() {
    judge("Freak", &soloed(FreakSlope::Linkwitz, false), &soloed(FreakSlope::Linear, false), true);
    judge("oversampled Freak", &soloed(FreakSlope::Linkwitz, true), &soloed(FreakSlope::Linear, true), false);
}
