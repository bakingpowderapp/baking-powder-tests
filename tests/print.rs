use bp::arp::{Arp, Articulation, Style};
use bp::division::Division;
use bp::project::{DEFAULT_BPM, SongNote, VELOCITY};
use support::{CHORD, arp_print};

const BEATS: f64 = 4.0;
const GATE: f64 = 0.5;

#[test]
fn printing_a_held_chord_through_a_quarter_arp_lays_a_note_on_every_beat() {
    let song: Vec<SongNote> = CHORD.iter().map(|&pitch| SongNote { start: 0.0, end: BEATS, pitch, velocity: VELOCITY, articulation: Articulation::PLAIN }).collect();
    let arp = Arp { on: true, rate: Division::Quarter, gate: GATE, style: Style::Up, ..Arp::DEFAULT };
    let printed = arp_print(&song, arp, DEFAULT_BPM);
    let starts: Vec<f64> = printed.iter().map(|note| note.start).collect();
    let pitches: Vec<u8> = printed.iter().map(|note| note.pitch).collect();
    let lengths: Vec<f64> = printed.iter().map(|note| note.end - note.start).collect();
    assert_eq!(starts, [0.0, 1.0, 2.0, 3.0], "wanted a step on every beat of the held chord, got {printed:?}");
    assert_eq!(pitches, [60, 64, 67, 60], "wanted the chord climbing and wrapping, got {printed:?}");
    assert!(lengths.iter().all(|length| (length - GATE).abs() < 1e-9), "wanted every step to last the gate {GATE}, got {lengths:?}");
}
