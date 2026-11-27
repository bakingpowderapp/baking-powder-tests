use bp::fx::params::{Gate, Lookahead};
use bp::fx::{Clock, Effect, Kind, Params, Slot};
use support::{BLOCK, SAMPLE_RATE};

const BPM: f64 = 120.0;
const LOOK_AHEAD: usize = 240;
const CLICK: f32 = 0.25;
const CLICKS: [usize; 3] = [10, 500, 950];
const FRAMES: usize = 8 * BLOCK;
const OPEN_DB: f64 = -60.0;
const QUICK_MS: f64 = 0.1;
const HELD_MS: f64 = 2000.0;

fn fresh_gate() -> Slot {
    Slot { params: Params::Gate(Gate { lookahead: Lookahead::Ms5, threshold_db: OPEN_DB, attack_ms: QUICK_MS, hold_ms: HELD_MS, ..Gate::DEFAULT }), ..Slot::new(Kind::Gate) }
}

fn clicks() -> Vec<f32> {
    let mut input = vec![0.0; FRAMES];
    for at in CLICKS {
        input[at] = CLICK;
    }
    input
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

fn first_mismatch(input: &[f32], output: &[f32], by: usize) -> Option<(usize, f32, f32)> {
    output.iter().enumerate().find_map(|(n, &got)| {
        let wanted = if n >= by { input[n - by] } else { 0.0 };
        (got != wanted).then_some((n, wanted, got))
    })
}

#[test]
fn a_fresh_gate_delays_its_first_clicks_by_the_whole_look_ahead() {
    let slot = fresh_gate();
    let reported = slot.latency(SAMPLE_RATE);
    assert_eq!(reported, LOOK_AHEAD, "wanted a Gate at 5 ms to report {LOOK_AHEAD}, got {reported}");
    let input = clicks();
    let output = through(&slot, &input);
    let early: Vec<usize> = (0..CLICKS[0] + LOOK_AHEAD).filter(|&n| output[n] != 0.0).collect();
    assert!(early.is_empty(), "wanted silence until frame {}, got sound at frames {early:?}", CLICKS[0] + LOOK_AHEAD);
    for at in CLICKS {
        let emerged = output[at + LOOK_AHEAD];
        assert_eq!(emerged, CLICK, "wanted the click from frame {at} whole at frame {}, got {emerged}", at + LOOK_AHEAD);
    }
    if let Some((frame, wanted, got)) = first_mismatch(&input, &output, LOOK_AHEAD) {
        panic!("frame {frame} wanted {wanted}, the input {LOOK_AHEAD} frames earlier, got {got}");
    }
}
