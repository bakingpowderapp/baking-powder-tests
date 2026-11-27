use bp::core::{Core, Params};
use support::{BLOCK, CHORD, SAMPLE_RATE, peak_of};

const VOICES: usize = 32;

fn render(core: &mut Core, blocks: usize) -> Vec<f32> {
    let mut out = Vec::with_capacity(blocks * BLOCK);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    for _ in 0..blocks {
        core.render(&mut l, &mut r);
        out.extend_from_slice(&l);
    }
    out
}

#[test]
fn lifting_every_key_ends_every_voice() {
    let mut core = Core::new(SAMPLE_RATE, Params::DEFAULT);
    for midi in CHORD {
        core.note_on(midi, 127);
    }
    assert!(peak_of(&render(&mut core, 8)) > 0.01, "the chord is silent");
    for midi in CHORD {
        core.note_off(midi);
    }
    let tail = render(&mut core, 200);
    let last = &tail[tail.len() - BLOCK * 8..];
    assert_eq!(peak_of(last), 0.0, "voices still sound a second after every key lifted");
}

fn first_note_after_the_rest_release(count: u8) -> f32 {
    let mut params = Params::DEFAULT;
    params.voices = VOICES;
    let mut core = Core::new(SAMPLE_RATE, params);
    for midi in 40..40 + count {
        core.note_on(midi, 127);
    }
    render(&mut core, 1);
    for midi in 41..40 + count {
        core.note_off(midi);
    }
    let tail = render(&mut core, 200);
    peak_of(&tail[tail.len() - BLOCK * 8..])
}

#[test]
fn thirty_two_voices_is_the_ceiling() {
    let held = first_note_after_the_rest_release(VOICES as u8);
    assert!(held > 0.01, "wanted the first of {VOICES} notes still sounding once the other {} released, peak {held}", VOICES - 1);
    let stolen = first_note_after_the_rest_release(VOICES as u8 + 1);
    assert_eq!(stolen, 0.0, "wanted note {} to steal the first voice, but the first note still sounds at peak {stolen}", VOICES + 1);
}
