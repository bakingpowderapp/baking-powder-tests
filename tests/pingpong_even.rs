use bp::arp::Arp;
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params as CoreParams;
use bp::core::params::Envelope;
use bp::fx::params::{Delay, DelayMode, Filter};
use bp::fx::{Kind, Params, Slot};
use bp::project::{Project, Row, VELOCITY};
use support::{AUDIBLE, BLOCK, SAMPLE_RATE, core_layer, frames, midi};

const KEY: u8 = 43;
const LEFT_MS: f64 = 200.0;
const RIGHT_MS: f64 = 150.0;
const LOW_CUT_HZ: f64 = 500.0;
const FEEDBACK: f64 = 0.5;
const NOTE_SECONDS: f64 = 0.05;
const RENDER_SECONDS: f64 = 0.6;
const ECHO_SECONDS: f64 = 0.06;
const EQUAL_DB: f64 = 0.5;
const HUSHED: f64 = 0.1;

fn low_sine() -> CoreParams {
    let mut patch = CoreParams::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    patch
}

fn even_ping_pong() -> Row {
    let params = Delay { mode: DelayMode::Clean, time_l_ms: LEFT_MS, time_r_ms: RIGHT_MS, feedback: FEEDBACK, ping_pong: 1.0, even: true, filter1: Filter { hz: LOW_CUT_HZ, ..Filter::LOW_CUT }, drive: 0.0, width: 1.0, ..Delay::DEFAULT };
    Row::Effect(Slot { wet: 1.0, params: Params::Delay(params), ..Slot::new(Kind::Delay) })
}

fn rendered() -> (Vec<f32>, Vec<f32>) {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, low_sine())), even_ping_pong()];
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(&project, None, &[], &[]);
    audio.note_on(0, KEY, VELOCITY);
    let (mut left, mut right) = (Vec::new(), Vec::new());
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut lifted = false;
    while left.len() < frames(RENDER_SECONDS) {
        if !lifted && left.len() >= frames(NOTE_SECONDS) {
            audio.note_off(0, KEY);
            lifted = true;
        }
        let n = BLOCK.min(frames(RENDER_SECONDS) - left.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        left.extend_from_slice(&l[..n]);
        right.extend_from_slice(&r[..n]);
    }
    (left, right)
}

fn rms(samples: &[f32], from: f64) -> f64 {
    let span = &samples[frames(from)..frames(from + ECHO_SECONDS)];
    (span.iter().map(|&x| f64::from(x) * f64::from(x)).sum::<f64>() / span.len() as f64).sqrt()
}

#[test]
fn the_first_right_echo_matches_the_first_left_echo_under_the_cut() {
    let (left, right) = rendered();
    let first_left = rms(&left, LEFT_MS / 1000.0);
    assert!(first_left > f64::from(AUDIBLE), "wanted a left echo at {LEFT_MS} ms, got an rms of {first_left:.5}");
    let right_meanwhile = rms(&right, LEFT_MS / 1000.0);
    assert!(right_meanwhile < HUSHED * first_left, "wanted the right side quiet while the first left echo plays, got {right_meanwhile:.5} against the left's {first_left:.5}");
    let first_right = rms(&right, (LEFT_MS + RIGHT_MS) / 1000.0);
    let gap = 20.0 * (first_right / first_left).log10();
    assert!(gap.abs() < EQUAL_DB, "wanted the first right echo at {} ms within {EQUAL_DB} dB of the first left on a {KEY} note under the {LOW_CUT_HZ} Hz cut, got {gap:+.2} dB", LEFT_MS + RIGHT_MS);
}
