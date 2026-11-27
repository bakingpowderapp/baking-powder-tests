use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::core::params::Envelope;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{BLOCK, FRAMES_PER_BEAT, SAMPLE_RATE, amplitude, core_layer, midi};

const MIDI: u8 = 57;
const FIFTH: u8 = 64;
const STACK: usize = 2;
const STACK_CENTS: f64 = 50.0;
const SETTLED: usize = 24_000;
const HEARD: usize = 48_000;
const SETTLE: usize = 4_800;
const BEATS: usize = 3;
const SECOND_BEAT: f32 = 1.0;
const APART_DB: f64 = 20.0;
const GONE_DB: f64 = 40.0;

fn hz(midi: u8, cents: f64) -> f64 {
    440.0 * ((f64::from(midi) - 69.0) / 12.0 + cents / 1200.0).exp2()
}

fn db(ratio: f64) -> f64 {
    20.0 * ratio.log10()
}

fn project(voices: usize, notes: &[(u8, f32)]) -> Project {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.voices = voices;
    patch.stack = STACK;
    patch.stack_cents = STACK_CENTS;
    patch.stack_spread = 1.0;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(0.25, patch))];
    let notes = notes.iter().map(|&(pitch, start)| Note { pitch, start, length: BEATS as f32 + 1.0 - start, velocity: VELOCITY, articulation: Articulation::PLAIN }).collect();
    track.clips.push(Clip { start: 0, length: (BEATS as i32 + 1) * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn render(project: &Project, frames: usize) -> (Vec<f32>, Vec<f32>) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(project, None, &[], &[]);
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let (mut left, mut right) = (Vec::with_capacity(frames), Vec::with_capacity(frames));
    while left.len() < frames {
        let n = BLOCK.min(frames - left.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        left.extend_from_slice(&l[..n]);
        right.extend_from_slice(&r[..n]);
    }
    (left, right)
}

#[test]
fn a_stack_of_two_spreads_its_copies_in_cents_and_across_the_field() {
    let (left, right) = render(&project(16, &[(MIDI, 0.0)]), SETTLED + HEARD);
    let (left, right) = (&left[SETTLED..], &right[SETTLED..]);
    let (low, high) = (hz(MIDI, -STACK_CENTS), hz(MIDI, STACK_CENTS));
    let left_apart = db(amplitude(left, low) / amplitude(left, high));
    let right_apart = db(amplitude(right, high) / amplitude(right, low));
    assert!(left_apart > APART_DB, "wanted the left to carry the copy {STACK_CENTS} cents under the note alone, got the lower copy only {left_apart:.1} dB over the upper");
    assert!(right_apart > APART_DB, "wanted the right to carry the copy {STACK_CENTS} cents over the note alone, got the upper copy only {right_apart:.1} dB over the lower");
}

#[test]
fn a_stack_counts_against_the_voices() {
    let frames = (BEATS as f64 * FRAMES_PER_BEAT) as usize;
    let after = (f64::from(SECOND_BEAT) * FRAMES_PER_BEAT) as usize + SETTLE;
    let first = hz(MIDI, -STACK_CENTS);
    let heard = |voices: usize| {
        let (left, _) = render(&project(voices, &[(MIDI, 0.0), (FIFTH, SECOND_BEAT)]), frames);
        amplitude(&left[after..], first)
    };
    let (kept, stolen) = (heard(4), heard(2));
    let gone = db(kept / stolen);
    assert!(gone > GONE_DB, "wanted two voices spent by a stack of two to leave the first note stolen when a second lands at beat {SECOND_BEAT}, got it only {gone:.1} dB under the four-voice render after frame {after}");
}
