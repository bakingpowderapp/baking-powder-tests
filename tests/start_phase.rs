use bp::arp::Arp;
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::core::Params;
use bp::core::params::Envelope;
use bp::project::{Project, Row, VELOCITY};
use support::{AUDIBLE, BLOCK, SAMPLE_RATE, core_layer, frames, midi, peak_of, worst_delta};

const A3: u8 = 57;
const A1: u8 = 33;
const COPIES: usize = 7;
const DETUNE_CENTS: f64 = 10.0;
const DEFAULT_SCATTER: f64 = Params::DEFAULT.phase_random;
const QUARTER_TURN: f64 = 0.25;
const HALF_TURN: f64 = 0.5;
const ONSET_SECONDS: f64 = 0.02;
const HELD_SECONDS: f64 = 0.5;
const AGAIN_SECONDS: f64 = 1.0;
const MIRROR_TOLERANCE: f32 = 1e-4;
const IN_STEP_TOLERANCE: f32 = 1e-4;

fn sine(phase: f64, copies: usize, detune: f64, scatter: f64) -> Params {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.osc1.phase = phase;
    patch.osc1.unison = copies;
    patch.osc1.detune = detune;
    patch.phase_random = scatter;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    patch
}

fn stack(copies: usize, scatter: f64) -> Params {
    let mut patch = Params::DEFAULT;
    patch.osc1.unison = copies;
    patch.osc1.detune = DETUNE_CENTS;
    patch.osc1.alternate = true;
    patch.phase_random = scatter;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    patch
}

fn project(patch: Params) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch))];
    project
}

struct Session {
    audio: Audio,
    renderer: Renderer,
    out: Vec<f32>,
}

impl Session {
    fn open(patch: Params) -> Session {
        let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
        renderer.patient();
        renderer.start_workers(0, || None);
        audio.sync(&project(patch), None, &[], &[]);
        Session { audio, renderer, out: Vec::new() }
    }

    fn render(&mut self, seconds: f64) {
        let end = self.out.len() + frames(seconds);
        let (mut left, mut right) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
        while self.out.len() < end {
            let n = BLOCK.min(end - self.out.len());
            self.renderer.render(&mut left[..n], &mut right[..n]);
            self.out.extend_from_slice(&left[..n]);
        }
    }

    fn onset(&self, at: f64) -> &[f32] {
        &self.out[frames(at)..frames(at + ONSET_SECONDS)]
    }
}

fn held(patch: Params) -> Vec<f32> {
    let mut session = Session::open(patch);
    session.audio.note_on(0, A3, VELOCITY);
    session.render(HELD_SECONDS);
    session.out
}

fn struck_once(patch: Params, key: u8) -> Vec<f32> {
    let mut session = Session::open(patch);
    session.audio.note_on(0, key, VELOCITY);
    session.render(ONSET_SECONDS);
    session.out
}

fn struck_twice(patch: Params, key: u8) -> (Vec<f32>, Vec<f32>) {
    let mut session = Session::open(patch);
    session.audio.note_on(0, key, VELOCITY);
    session.render(ONSET_SECONDS);
    session.audio.note_off(0, key);
    session.render(AGAIN_SECONDS - ONSET_SECONDS);
    session.audio.note_on(0, key, VELOCITY);
    session.render(ONSET_SECONDS);
    (session.onset(0.0).to_vec(), session.onset(AGAIN_SECONDS).to_vec())
}

fn same_bytes(a: &[f32], b: &[f32]) -> Result<(), usize> {
    match a.iter().zip(b).position(|(x, y)| x.to_bits() != y.to_bits()) {
        None if a.len() == b.len() => Ok(()),
        None => Err(a.len().min(b.len())),
        Some(frame) => Err(frame),
    }
}

#[test]
fn a_sine_started_half_a_turn_later_is_the_first_one_upside_down() {
    let up = held(sine(QUARTER_TURN, 1, 0.0, 1.0));
    let down = held(sine(QUARTER_TURN + HALF_TURN, 1, 0.0, 1.0));
    let peak = peak_of(&up);
    assert!(peak > AUDIBLE, "wanted the sine to sound, got a peak of {peak}");
    let mirrored: Vec<f32> = down.iter().map(|sample| -sample).collect();
    let worst = worst_delta(&up, &mirrored) / peak;
    assert!(worst < MIRROR_TOLERANCE, "wanted a sine started at {} of a turn to be the one started at {QUARTER_TURN} upside down, got a worst gap of {worst} of the peak", QUARTER_TURN + HALF_TURN);
}

#[test]
fn copies_with_no_scatter_start_in_step() {
    let lone = held(sine(QUARTER_TURN, 1, 0.0, 0.0));
    let together = held(sine(QUARTER_TURN, COPIES, 0.0, 0.0));
    let ratio = peak_of(&together) / peak_of(&lone);
    assert!(ratio > 1.0, "wanted {COPIES} copies in step to sum louder than one, got {ratio} times");
    let scaled: Vec<f32> = lone.iter().map(|sample| sample * ratio).collect();
    let worst = worst_delta(&together, &scaled) / peak_of(&together);
    assert!(worst < IN_STEP_TOLERANCE, "wanted {COPIES} undetuned copies with no scatter to render as one copy scaled, got a worst gap of {worst} of the peak");
}

#[test]
fn scattered_copies_do_not_start_in_step() {
    let lone = held(sine(QUARTER_TURN, 1, 0.0, 1.0));
    let apart = held(sine(QUARTER_TURN, COPIES, 0.0, 1.0));
    let ratio = peak_of(&apart) / peak_of(&lone);
    let scaled: Vec<f32> = lone.iter().map(|sample| sample * ratio).collect();
    let worst = worst_delta(&apart, &scaled) / peak_of(&apart);
    assert!(worst > IN_STEP_TOLERANCE, "wanted {COPIES} scattered copies to render unlike one copy scaled, got a worst gap of {worst} of the peak");
}

#[test]
fn a_stack_with_no_scatter_starts_every_strike_alike() {
    let (first, again) = struck_twice(stack(COPIES, 0.0), A3);
    assert!(peak_of(&first) > AUDIBLE, "wanted the stack to sound at its onset, got a peak of {}", peak_of(&first));
    let worst = worst_delta(&first, &again);
    assert!(worst == 0.0, "wanted a stack with no scatter to begin its second strike as it began its first, got a worst gap of {worst}");
}

#[test]
fn a_lone_oscillator_starts_every_note_from_zero() {
    let (first, again) = struck_twice(stack(1, DEFAULT_SCATTER), A1);
    assert!(peak_of(&first) > AUDIBLE, "wanted the lone saw to sound at its onset, got a peak of {}", peak_of(&first));
    assert_eq!(same_bytes(&first, &again), Ok(()), "wanted a second strike of a lone saw to begin as the first did, first differing at frame");
}

#[test]
fn a_stack_draws_new_copy_phases_at_every_strike() {
    let (first, again) = struck_twice(stack(COPIES, DEFAULT_SCATTER), A1);
    assert!(peak_of(&first) > AUDIBLE, "wanted the stack to sound at its onset, got a peak of {}", peak_of(&first));
    assert!(same_bytes(&first, &again).is_err(), "wanted a second strike of a {COPIES}-copy stack to begin with its copies elsewhere, got the same {ONSET_SECONDS} s");
}

#[test]
fn a_stack_struck_at_the_same_frame_renders_bit_for_bit() {
    let (first, again) = (struck_once(stack(COPIES, DEFAULT_SCATTER), A1), struck_once(stack(COPIES, DEFAULT_SCATTER), A1));
    assert_eq!(same_bytes(&first, &again), Ok(()), "wanted the same stack at the same frame to render the same bytes, first differing at frame");
}
