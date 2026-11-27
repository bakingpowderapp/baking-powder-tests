use bp::arp::{Arp, Articulation, Lane, Pattern, Style, self};
use bp::core::Params;
use bp::division::Division;
use bp::json;
use bp::project::{DEFAULT_BPM, Project, Row, SongNote, VELOCITY};
use support::{CHORD, arp_print, core_layer, midi, unseeded};

const BAR: f64 = 4.0;
const TWO_BARS: f64 = 8.0;
const GATE: f64 = 0.5;

fn held(beats: f64) -> Vec<SongNote> {
    CHORD.iter().map(|&pitch| SongNote { start: 0.0, end: beats, pitch, velocity: VELOCITY, articulation: Articulation::PLAIN }).collect()
}

fn arp(pattern: Pattern, style: Style) -> Arp {
    Arp { on: true, rate: Division::Quarter, gate: GATE, style, pattern, ..Arp::DEFAULT }
}

fn steps(notes: &[i8]) -> Pattern {
    let mut pattern = Pattern::DEFAULT;
    pattern.set_length(notes.len());
    for (index, note) in notes.iter().enumerate() {
        pattern.set(Lane::Note, index, *note);
    }
    pattern
}

fn played(pattern: Pattern, style: Style, beats: f64) -> (Vec<f64>, Vec<u8>) {
    let printed = arp_print(&held(beats), arp(pattern, style), DEFAULT_BPM);
    (printed.iter().map(|note| note.start).collect(), printed.iter().map(|note| note.pitch).collect())
}

#[test]
fn numbered_steps_count_up_from_the_lowest_note_of_the_chord() {
    let (starts, pitches) = played(steps(&[1, 2, 3, 2]), Style::Up, BAR);
    assert_eq!(starts, [0.0, 1.0, 2.0, 3.0], "wanted a step on every beat of the bar, got {starts:?}");
    assert_eq!(pitches, [60, 64, 67, 64], "wanted the lowest, the second, the highest and the second again, got {pitches:?}");
    assert_eq!(steps(&[-1]).get(Lane::Note, 0), arp::RUN, "wanted no count below the lowest note, a step set under it left to run on");
}

#[test]
fn run_steps_follow_the_style_and_a_rest_pauses_it() {
    let mut pattern = steps(&[arp::RUN; 4]);
    pattern.set_rest(1, true);
    let (starts, pitches) = played(pattern, Style::Up, TWO_BARS);
    assert_eq!(starts, [0.0, 2.0, 3.0, 4.0, 6.0, 7.0], "wanted the second step of each pass silent, got {starts:?}");
    assert_eq!(pitches, [60, 64, 67, 60, 64, 67], "wanted the climb to pause over the rest rather than skip a note, got {pitches:?}");
}

#[test]
fn numbers_past_the_chord_stop_at_its_highest_note_and_octaves_move_the_note() {
    let mut pattern = steps(&[4, 1]);
    pattern.set(Lane::Octave, 0, 1);
    pattern.set(Lane::Octave, 1, -1);
    let (starts, pitches) = played(pattern, Style::Up, BAR);
    assert_eq!(starts, [0.0, 1.0, 2.0, 3.0], "wanted the two steps looping across the bar, got {starts:?}");
    assert_eq!(pitches, [79, 48, 79, 48], "wanted note 4 stopping at the highest note, an octave up, and note 1 the lowest, an octave down, got {pitches:?}");
    let (_, far) = played(steps(&[3, 5, 9, 16]), Style::Up, BAR);
    assert_eq!(far, [67, 67, 67, 67], "wanted every count past a three-note chord to stay on its highest note, got {far:?}");
}

#[test]
fn a_pattern_survives_a_save_and_a_plain_one_writes_nothing() {
    let mut pattern = steps(&[1, 3, 2]);
    pattern.set_rest(2, true);
    pattern.set(Lane::Octave, 1, 2);
    let mut project = Project::new();
    project.tracks[0].rack = vec![midi(arp(pattern, Style::Down)), Row::Core(core_layer(1.0, Params::DEFAULT))];
    let text = project.to_json().write();
    let reopened = Project::from_json(&json::parse(&text).expect("a saved project is valid JSON")).expect("a saved project opens");
    assert!(unseeded(&reopened.tracks[0].rack) == project.tracks[0].rack, "wanted the pattern back exactly as saved from {text}");
    let plain = Project::new().to_json().write();
    assert!(!plain.contains("pattern"), "wanted a plain pattern left out of the save, got {plain}");
}
