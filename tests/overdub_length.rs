use bp::arp::Articulation;
use bp::flatten;
use bp::project::{Clip, Note, SongNote, TICKS_PER_BEAT, VELOCITY};

const CLIP_BEATS: i32 = 4;
const PITCH: u8 = 60;

fn clip(notes: Vec<Note>) -> Clip {
    Clip { start: 0, length: CLIP_BEATS * TICKS_PER_BEAT, label: String::from("I"), notes, file: None }
}

fn played(start: f64, end: f64) -> SongNote {
    SongNote { start, end, pitch: PITCH, velocity: VELOCITY, articulation: Articulation::PLAIN }
}

fn note(start: f32, length: f32) -> Note {
    Note { pitch: PITCH, start, length, velocity: VELOCITY, articulation: Articulation::PLAIN }
}

#[test]
fn a_take_ending_inside_the_clip_leaves_the_clips_length_alone() {
    let joined = flatten::overdub(&[clip(Vec::new())], &[played(0.0, 1.0)]);
    assert_eq!(joined, vec![clip(vec![note(0.0, 1.0)])], "wanted the clip to keep its {CLIP_BEATS} beats with the one-beat take laid inside it");
}

#[test]
fn a_take_running_past_the_clip_stretches_it_to_the_beat_after_the_take() {
    let joined = flatten::overdub(&[clip(Vec::new())], &[played(3.0, 5.5)]);
    assert_eq!(joined, vec![Clip { length: 6 * TICKS_PER_BEAT, ..clip(vec![note(3.0, 2.5)]) }], "wanted the clip stretched to beat 6 to hold a take ending at beat 5.5");
}
