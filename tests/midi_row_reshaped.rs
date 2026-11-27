use bp::arp::{Arp, Articulation};
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::core::Params;
use bp::core::params::Envelope;
use bp::division::Division;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{AUDIBLE, BLOCK, CHORD, SAMPLE_RATE, SILENT, beat_frame, core_layer, midi, peak_of};

const CHORD_BEATS: f32 = 2.0;
const CLIP_BEATS: i32 = 4;
const RESHAPE_BEAT: usize = 1;
const QUIET_FROM: usize = 3;
const END_BEAT: usize = 4;

fn arped() -> Row {
    midi(Arp { on: true, rate: Division::Sixteenth, ..Arp::DEFAULT })
}

fn core() -> Row {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    Row::Core(core_layer(1.0, patch))
}

fn project(rack: Vec<Row>) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = rack;
    let notes = CHORD.iter().map(|&pitch| Note { pitch, start: 0.0, length: CHORD_BEATS, velocity: VELOCITY, articulation: Articulation::PLAIN }).collect();
    project.tracks[0].clips.push(Clip { start: 0, length: CLIP_BEATS * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn render(renderer: &mut Renderer, frames: usize) -> Vec<f32> {
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(frames);
    while out.len() < frames {
        let n = BLOCK.min(frames - out.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    out
}

fn reshaped(first: Vec<Row>, then: Vec<Row>) -> (Vec<f32>, Vec<f32>) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(&project(first), None, &[], &[]);
    audio.play(0.0);
    let before = render(&mut renderer, beat_frame(RESHAPE_BEAT));
    audio.sync(&project(then), None, &[], &[]);
    let after = render(&mut renderer, beat_frame(END_BEAT) - beat_frame(RESHAPE_BEAT));
    (before, after)
}

fn expect_let_go(first: Vec<Row>, then: Vec<Row>, what: &str) {
    let (before, after) = reshaped(first, then);
    let heard = peak_of(&before);
    assert!(heard > AUDIBLE, "wanted the chord heard before {what}, got a peak of {heard}");
    let left = peak_of(&after[beat_frame(QUIET_FROM) - beat_frame(RESHAPE_BEAT)..]);
    assert!(left <= SILENT, "wanted silence from beat {QUIET_FROM} once {what} at beat {RESHAPE_BEAT} and the chord ended at beat {CHORD_BEATS}, got a peak of {left}");
}

#[test]
fn an_arp_row_inserted_above_a_sounding_core_lets_the_chord_go() {
    expect_let_go(vec![midi(Arp::DEFAULT), core()], vec![midi(Arp::DEFAULT), arped(), core()], "an arp row was inserted above the Core");
}

#[test]
fn an_arp_row_moved_above_a_sounding_core_lets_the_chord_go() {
    expect_let_go(vec![midi(Arp::DEFAULT), core(), arped()], vec![midi(Arp::DEFAULT), arped(), core()], "an arp row was moved above the Core");
}

#[test]
fn an_arp_row_removed_from_above_a_core_lets_its_copies_go() {
    expect_let_go(vec![midi(Arp::DEFAULT), arped(), core()], vec![midi(Arp::DEFAULT), core()], "the arp row above the Core was removed");
}
