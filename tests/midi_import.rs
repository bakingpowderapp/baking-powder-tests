use bp::arp::Articulation;
use bp::project::{Clip, LONGEST_TICKS, Note, TICKS_PER_BAR, TICKS_PER_BEAT, beats};
use support::NO_END;

const START: i32 = TICKS_PER_BEAT;
const FAR: f32 = 1e12;

fn note(start: f32, length: f32) -> Note {
    Note { pitch: 60, start, length, velocity: 100, articulation: Articulation::PLAIN }
}

#[test]
fn an_imported_clip_that_outruns_the_song_is_cut_at_the_songs_end() {
    let clip = Clip::midi(START, String::from("long"), vec![note(0.0, f32::INFINITY), note(FAR, 1.0)], NO_END);
    let room = beats(LONGEST_TICKS - START) as f32;
    assert!(clip.length > 0 && clip.end() <= LONGEST_TICKS, "wanted the clip to end inside the song, got {} to {} against {LONGEST_TICKS}", clip.start, clip.end());
    assert_eq!(clip.notes.len(), 1, "wanted the note starting past the song's end dropped");
    assert_eq!(clip.notes[0].length, room, "wanted the endless note cut at the song's end");
}

#[test]
fn an_imported_clip_within_the_song_keeps_its_bars() {
    let clip = Clip::midi(0, String::from("short"), vec![note(0.0, 5.0)], NO_END);
    assert_eq!(clip.length, 2 * TICKS_PER_BAR, "wanted a five-beat note rounded up to two bars");
    assert_eq!(clip.notes[0].length, 5.0, "wanted the note untouched");
}
