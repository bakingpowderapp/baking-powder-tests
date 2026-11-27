use bp::core::params::Envelope;
use bp::core::{Core, Params};
use support::{BLOCK, SAMPLE_RATE, amplitude, frames};

const VOICES: usize = 2;
const LOUD: u8 = 60;
const FAINT: u8 = 72;
const THIRD: u8 = 67;
const WHISPER: u8 = 1;
const SETTLE_SECONDS: f64 = 0.1;
const HEARD_SECONDS: f64 = 0.5;
const PRESENT: f64 = 0.05;

fn midi_hz(midi: u8) -> f64 {
    440.0 * ((f64::from(midi) - 69.0) / 12.0).exp2()
}

fn sines() -> Params {
    let mut patch = Params::DEFAULT;
    patch.voices = VOICES;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    patch
}

fn render(core: &mut Core, seconds: f64) -> Vec<f32> {
    let mut out = Vec::with_capacity(frames(seconds));
    let (mut left, mut right) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    while out.len() < frames(seconds) {
        let n = BLOCK.min(frames(seconds) - out.len());
        core.render(&mut left[..n], &mut right[..n]);
        out.extend_from_slice(&left[..n]);
    }
    out
}

pub(crate) fn heard_after_the_third() -> [f64; 3] {
    let mut core = Core::new(SAMPLE_RATE, sines());
    core.note_on(LOUD, 127);
    render(&mut core, SETTLE_SECONDS);
    core.note_on(FAINT, WHISPER);
    render(&mut core, SETTLE_SECONDS);
    core.note_on(THIRD, 127);
    render(&mut core, SETTLE_SECONDS);
    let heard = render(&mut core, HEARD_SECONDS);
    [LOUD, FAINT, THIRD].map(|midi| amplitude(&heard, midi_hz(midi)))
}

#[test]
fn a_whispered_newer_note_is_stolen_before_a_loud_older_one() {
    let [loud, faint, third] = heard_after_the_third();
    assert!(loud > PRESENT && third > PRESENT, "wanted the third note {THIRD} on two voices to take the newer note {FAINT} at velocity {WHISPER} and leave the older note {LOUD} at velocity 127 sounding, got amplitudes {loud:.4} at {LOUD}, {faint:.6} at {FAINT} and {third:.4} at {THIRD}");
}
