use bp::fx::params::Utility;
use bp::fx::{Clock, Effect, Kind, Params, Slot};
use std::f64::consts::TAU;
use support::{BLOCK, SAMPLE_RATE, SILENT};

const BPM: f64 = 120.0;
const HZ: f64 = 220.0;
const LEVEL: f32 = 0.25;
const BEFORE: usize = 100;
const AFTER: usize = 20;
const LOOK_AHEAD: usize = 71;
const HOLE: usize = 2;
const ALIKE: f32 = 1e-6;
const OPEN_DB: f64 = 0.0;

fn sine(frames: usize) -> Vec<f32> {
    (0..frames).map(|n| (TAU * HZ * n as f64 / SAMPLE_RATE).sin() as f32 * LEVEL).collect()
}

fn armed() -> Slot {
    Slot { params: Params::Utility(Utility { limit: true, ceiling_db: OPEN_DB, ..Utility::DEFAULT }), ..Slot::new(Kind::Utility) }
}

fn armed_midway(input: &[f32]) -> Vec<f32> {
    let (off, on) = (Slot::new(Kind::Utility), armed());
    let mut effect = Effect::new(Kind::Utility, SAMPLE_RATE, BLOCK);
    let clock = Clock { sample_rate: SAMPLE_RATE, bpm: BPM, beat: None };
    let mut output = Vec::with_capacity(input.len());
    for (index, chunk) in input.chunks(BLOCK).enumerate() {
        let slot = if index < BEFORE { &off } else { &on };
        let (mut l, mut r) = (chunk.to_vec(), chunk.to_vec());
        effect.process(&mut l, &mut r, slot, &clock);
        output.extend_from_slice(&l);
    }
    output
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

fn first_stray(input: &[f32], output: &[f32], from: usize, by: usize) -> Option<(usize, f32, f32)> {
    (from..output.len()).find_map(|n| ((output[n] - input[n - by]).abs() >= ALIKE).then_some((n, input[n - by], output[n])))
}

#[test]
fn arming_the_limiter_leaves_no_hole() {
    let reported = armed().latency(SAMPLE_RATE);
    assert_eq!(reported, LOOK_AHEAD, "wanted the armed limiter to report {LOOK_AHEAD}, got {reported}");
    let input = sine((BEFORE + AFTER) * BLOCK);
    let output = armed_midway(&input);
    let arm = BEFORE * BLOCK;
    if let Some((frame, wanted, got)) = first_stray(&input, &output, 0, 0).filter(|stray| stray.0 < arm) {
        panic!("wanted the sine untouched with the limiter off before frame {arm}, frame {frame} wanted {wanted}, got {got}");
    }
    let (start, run) = longest_silence(&output[arm..arm + LOOK_AHEAD]);
    assert!(run <= HOLE, "wanted no more than {HOLE} silent frames in the {LOOK_AHEAD} after arming at frame {arm}, got {run} from frame {}", arm + start);
    if let Some((frame, wanted, got)) = first_stray(&input, &output, arm, LOOK_AHEAD) {
        panic!("wanted the sine delayed by {LOOK_AHEAD} from the arming at frame {arm} on, frame {frame} wanted {wanted}, got {got}");
    }
}
