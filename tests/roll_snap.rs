use bp::app::{App, Event, Request};
use bp::arp::Articulation;
use bp::draw::{Fill, Rect, Tone};
use bp::face::{FRAME, PITCH};
use bp::json;
use bp::keyboard::Key;
use bp::project::{Clip, Note, Project, TICKS_PER_BAR, VELOCITY};
use support::{clip_block, painted};

const KEYS: i32 = 2 * PITCH;
const BAR_BEATS: f32 = 4.0;
const TWO_BARS: f32 = 2.0 * BAR_BEATS;
const PRESSED_BEAT: f32 = 1.6;
const SECOND_BAR_BEAT: f32 = 4.5;
const STEP_LINES: usize = 12;
const SIXTEENTH: Note = Note { pitch: 60, start: 2.0, length: 0.25, velocity: VELOCITY, articulation: Articulation::PLAIN };
const SIXTEENTH_STEP: f32 = 0.25;
const BEAT_NOTE: Note = Note { start: 4.0, length: 1.0, ..SIXTEENTH };

fn open(snap: &str) -> App {
    open_with(snap, 1, Vec::new())
}

fn open_with(snap: &str, bars: i32, notes: Vec<Note>) -> App {
    let mut project = Project::new();
    project.tracks[0].clips.push(Clip { start: 0, length: bars * TICKS_PER_BAR, label: String::from("bar"), notes, file: None });
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false,\"snap\":\"{snap}\"}}", project.to_json().write());
    let mut app = App::new();
    app.resume(&session);
    let clip = clip_block(&painted(&mut app));
    click(&mut app, clip.x + clip.w / 2, clip.y + clip.h / 2);
    app
}

fn click(app: &mut App, x: i32, y: i32) {
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    app.event(Event::Leave);
}

fn tall_lines(fills: &[Fill]) -> (Rect, Rect) {
    let tallest = fills.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.w == FRAME).map(|fill| fill.rect.h).max().expect("wanted the roll's tall lines painted");
    let mut lines: Vec<Rect> = fills.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.w == FRAME && fill.rect.h == tallest).map(|fill| fill.rect).collect();
    lines.sort_by_key(|line| line.x);
    let keys = lines.iter().filter(|line| line.x < lines[0].x + KEYS).count();
    assert!(lines.len() >= keys + 2, "wanted two tall lines after the key lines, the first at the clip's start, got lines at {lines:?}");
    (lines[keys], lines[keys + 1])
}

fn first_note(app: &mut App) -> (f32, f32) {
    let text = app.take_requests().into_iter().rev().find_map(|request| if let Request::Remember { json } = request { Some(json) } else { None }).expect("wanted the session remembered after the press");
    let session = json::parse(&text).expect("wanted the remembered session to parse");
    let project = Project::from_json(session.get("project").expect("wanted the remembered session to hold the project")).expect("wanted the remembered project to read");
    let note = project.tracks[0].clips[0].notes.first().copied().expect("wanted a note in the clip");
    (note.start, note.length)
}

fn draw_note_at(app: &mut App, beat: f32, beats_apart: f32) -> (f32, f32) {
    let (first, last) = tall_lines(&painted(app));
    let beat_w = (last.x - first.x) as f32 / beats_apart;
    click(app, first.x + (beat * beat_w).round() as i32, first.y + first.h / 2);
    first_note(app)
}

fn draw_note(app: &mut App) -> (f32, f32) {
    draw_note_at(app, PRESSED_BEAT, BAR_BEATS)
}

fn arrow(app: &mut App, key: Key, shift: bool) -> (f32, f32) {
    app.event(Event::Key { key, shift, command: false, repeat: false });
    app.event(Event::KeyUp { key });
    first_note(app)
}

fn step_lines(fills: &[Fill]) -> usize {
    let (first, _) = tall_lines(fills);
    fills.iter().filter(|fill| fill.tone == Tone::Ink50 && fill.rect.w == 1 && fill.rect.h == first.h && fill.rect.y == first.y).count()
}

#[test]
fn a_note_drawn_in_the_roll_lands_on_the_headers_snap() {
    let mut app = open("1/4");
    assert_eq!(draw_note(&mut app), (1.0, 1.0), "wanted a press at beat {PRESSED_BEAT} to draw a beat-long note on beat 1 with the snap at 1/4");
    let mut app = open("1/16");
    assert_eq!(draw_note(&mut app), (1.5, 0.25), "wanted a press at beat {PRESSED_BEAT} to draw a sixteenth on beat 1.5 with the snap at 1/16, one cell and not a beat");
    let mut app = open("1/1");
    assert_eq!(draw_note(&mut app), (0.0, BAR_BEATS), "wanted a press at beat {PRESSED_BEAT} to draw a bar-long note from the bar's start with the snap at 1/1");
}

#[test]
fn the_roll_draws_the_snaps_finer_than_a_beat() {
    let mut app = open("1/16");
    let lines = step_lines(&painted(&mut app));
    assert_eq!(lines, STEP_LINES, "wanted a thin line at each sixteenth between the beats, {STEP_LINES} over a bar, got {lines}");
    let mut app = open("1/4");
    let lines = step_lines(&painted(&mut app));
    assert_eq!(lines, 0, "wanted no step lines with the snap on the beat, got {lines}");
}

#[test]
fn a_dotted_snap_runs_on_across_the_bar_line() {
    let mut app = open_with("1/2.", 2, Vec::new());
    assert_eq!(draw_note_at(&mut app, SECOND_BAR_BEAT, TWO_BARS), (3.0, 3.0), "wanted a press at beat {SECOND_BAR_BEAT}, between the clip's edge lines two bars apart, to land on the dotted line at 3, the grid counted on from the clip's start and not started over at the bar, and run to the next line at 6");
    let mut app = open_with("1/2.", 2, vec![Note { start: 3.0, length: 1.0, ..SIXTEENTH }]);
    app.event(Event::Key { key: Key::Char('a'), shift: false, command: true, repeat: false });
    assert_eq!(arrow(&mut app, Key::Right, false), (6.0, 1.0), "wanted a right arrow to carry the note from the dotted line at 3 over the bar to the next line at 6, not to 4");
}

#[test]
fn shift_with_right_and_left_stretches_the_picked_note_from_its_end() {
    let mut app = open_with("1/16", 4, vec![BEAT_NOTE]);
    app.event(Event::Key { key: Key::Char('a'), shift: false, command: true, repeat: false });
    let longer = arrow(&mut app, Key::Right, true);
    assert_eq!(longer, (BEAT_NOTE.start, BEAT_NOTE.length + SIXTEENTH_STEP), "wanted shift-right to lengthen the note by {SIXTEENTH_STEP} from its end, got start {} and length {}", longer.0, longer.1);
    arrow(&mut app, Key::Left, true);
    let shorter = arrow(&mut app, Key::Left, true);
    assert_eq!(shorter, (BEAT_NOTE.start, BEAT_NOTE.length - SIXTEENTH_STEP), "wanted two shift-lefts to shorten the note to {} from its end, got start {} and length {}", BEAT_NOTE.length - SIXTEENTH_STEP, shorter.0, shorter.1);
}

#[test]
fn shortening_a_note_finer_than_the_snap_never_grows_it() {
    let mut app = open_with("1/4", 1, vec![SIXTEENTH]);
    app.event(Event::Key { key: Key::Char('a'), shift: false, command: true, repeat: false });
    assert_eq!(arrow(&mut app, Key::Left, true), (SIXTEENTH.start, SIXTEENTH.length), "wanted shift-left to leave a sixteenth note as it is with the snap on the beat, never lengthening it");
    assert_eq!(arrow(&mut app, Key::Right, false), (3.0, SIXTEENTH.length), "wanted a right arrow to move the sixteenth to the next beat and keep its length");
}

#[test]
fn a_dotted_stretch_passes_the_bar_and_comes_back_to_the_line_it_left() {
    let mut app = open_with("1/2.", 2, vec![Note { start: 0.0, length: 3.0, ..SIXTEENTH }]);
    app.event(Event::Key { key: Key::Char('a'), shift: false, command: true, repeat: false });
    assert_eq!(arrow(&mut app, Key::Right, true), (0.0, 6.0), "wanted shift-right to run the note's end from the dotted line at 3 over the bar to the next line at 6, not to stop at 4");
    assert_eq!(arrow(&mut app, Key::Left, true), (0.0, 3.0), "wanted shift-left to bring the end back to the dotted line at 3, not to the bar at 4");
}
