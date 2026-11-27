use bp::core::{Core, Params};
use support::{BLOCK, SAMPLE_RATE, amplitude};

const HELD: u8 = 60;
const RELEASING: u8 = 67;
const NEWCOMER: u8 = 64;
const RELEASE_MS: f64 = 2_000.0;
const SETTLE_BLOCKS: usize = 16;
const HEARD_BLOCKS: usize = 94;
const PRESENT: f64 = 0.05;
const GONE: f64 = 1e-4;

fn render(core: &mut Core, blocks: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(blocks * BLOCK);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    for _ in 0..blocks {
        core.render(&mut l, &mut r);
        out.extend_from_slice(&l);
    }
    out
}

fn midi_hz(midi: u8) -> f64 {
    440.0 * ((f64::from(midi) - 69.0) / 12.0).exp2()
}

fn third_note_over_two_voices(fresh: bool) -> [f64; 3] {
    let mut params = Params::DEFAULT;
    params.voices = 2;
    params.fresh = fresh;
    params.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    params.amp.release_ms = RELEASE_MS;
    let mut core = Core::new(SAMPLE_RATE, params);
    core.note_on(HELD, 127);
    core.note_on(RELEASING, 127);
    render(&mut core, SETTLE_BLOCKS);
    core.note_off(RELEASING);
    render(&mut core, SETTLE_BLOCKS);
    core.note_on(NEWCOMER, 127);
    render(&mut core, SETTLE_BLOCKS);
    let heard = render(&mut core, HEARD_BLOCKS);
    [HELD, RELEASING, NEWCOMER].map(|midi| amplitude(&heard, midi_hz(midi)))
}

#[test]
fn a_fresh_voice_takes_the_releasing_tail_and_spares_the_held_note() {
    let [held, releasing, newcomer] = third_note_over_two_voices(true);
    assert!(held > PRESENT, "wanted the held note {HELD} still sounding after the third note, got amplitude {held:.4}");
    assert!(newcomer > PRESENT, "wanted the third note {NEWCOMER} sounding, got amplitude {newcomer:.4}");
    assert!(releasing < GONE, "wanted the releasing note {RELEASING} stolen for the third note, got amplitude {releasing:.4}");
}

#[test]
fn without_fresh_voice_the_releasing_tail_still_goes_before_the_held_note() {
    let [held, releasing, newcomer] = third_note_over_two_voices(false);
    assert!(newcomer > PRESENT, "wanted the third note {NEWCOMER} sounding, got amplitude {newcomer:.4}");
    assert!(held > PRESENT, "wanted the held note {HELD} kept over the releasing tail without fresh voice, got amplitude {held:.4}");
    assert!(releasing < GONE, "wanted the releasing note {RELEASING} taken first without fresh voice, got amplitude {releasing:.4}");
}
