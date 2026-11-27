use bp::arp::Articulation;
use bp::audio::Position;
use bp::project::{Clip, Note, TICKS_PER_BAR, VELOCITY};
use bp::record::{Take, lay};

const PITCH: u8 = 60;
const HELD_PITCH: u8 = 64;
const BAR: i32 = TICKS_PER_BAR;
const LOOP: Option<(f64, f64)> = Some((0.0, 4.0));
const GRID: f64 = 1.0;

fn at(beat: f64) -> Position {
    Position { beat, wraps: 0 }
}

fn bar(notes: Vec<Note>) -> Clip {
    Clip { start: 0, length: BAR, label: String::new(), notes, file: None }
}

fn note(pitch: u8, start: f32, length: f32) -> Note {
    Note { pitch, start, length, velocity: VELOCITY, articulation: Articulation::PLAIN }
}

#[test]
fn a_take_snaps_to_the_beat_and_lays_a_bar() {
    let mut take = Take::new(at(0.0), std::iter::empty());
    take.note_on(at(0.9), None, PITCH);
    take.note_off(at(1.6), None, PITCH);
    let laid = lay(&[], &take.close(at(4.2), None), GRID);
    assert_eq!(laid, vec![bar(vec![note(PITCH, 1.0, 1.0)])], "wanted one bar holding the note snapped to beat 1 for a beat");
}

#[test]
fn a_take_joins_the_clip_it_lands_in_and_trims_nothing() {
    let existing = Clip { label: String::from("I"), ..bar(vec![note(HELD_PITCH, 0.0, 4.0)]) };
    let mut take = Take::new(at(0.0), std::iter::empty());
    take.note_on(at(1.1), None, PITCH);
    take.note_off(at(1.9), None, PITCH);
    let laid = lay(&[existing], &take.close(at(4.0), None), GRID);
    assert_eq!(laid, vec![Clip { label: String::from("I"), ..bar(vec![note(HELD_PITCH, 0.0, 4.0), note(PITCH, 1.0, 1.0)]) }], "wanted the take's note beside the held one in the same clip");
}

#[test]
fn a_note_held_across_the_loop_end_is_split_at_the_wrap() {
    let take = Take::new(at(2.0), [PITCH]);
    let laid = lay(&[], &take.close(Position { beat: 1.0, wraps: 1 }, LOOP), GRID);
    assert_eq!(laid, vec![bar(vec![note(PITCH, 0.0, 1.0), note(PITCH, 2.0, 2.0)])], "wanted the held note closed at the loop end and reopened at its start");
}
