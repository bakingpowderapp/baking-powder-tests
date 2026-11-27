use bp::app::{App, Event, Request};
use bp::arp::Articulation;
use bp::draw::{Fill, Rect, Tone};
use bp::face::{FRAME, PITCH};
use bp::json;
use bp::project::{Clip, Note, Project, TICKS_PER_BAR, VELOCITY};
use support::{clip_block, painted};

const KEYS: i32 = 2 * PITCH;
const BAR_BEATS: f32 = 4.0;
const BARS: i32 = 2;
const SIXTEENTH: &str = "1/16";
const EIGHTH: &str = "1/8";
const HALF: &str = "1/2";
const LONG: Note = Note { pitch: 60, start: 0.0, length: 2.5, velocity: VELOCITY, articulation: Articulation::PLAIN };

fn open(snap: &str, notes: Vec<Note>) -> App {
    let mut project = Project::new();
    project.tracks[0].clips.push(Clip { start: 0, length: BARS * TICKS_PER_BAR, label: String::from("bars"), notes, file: None });
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false,\"snap\":\"{snap}\"}}", project.to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    let clip = clip_block(&painted(&mut app));
    click(&mut app, clip.x + clip.w / 2, clip.y + clip.h / 2, 1);
    app
}

fn click(app: &mut App, x: i32, y: i32, clicks: i32) {
    app.event(Event::Down { x, y, clicks });
    app.event(Event::Up { x, y });
    app.event(Event::Leave);
}

fn bar_lines(fills: &[Fill]) -> (Rect, Rect) {
    let tallest = fills.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.w == FRAME).map(|fill| fill.rect.h).max().expect("wanted the roll's bar lines painted");
    let mut lines: Vec<Rect> = fills.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.w == FRAME && fill.rect.h == tallest).map(|fill| fill.rect).collect();
    lines.sort_by_key(|line| line.x);
    let keys = lines.iter().filter(|line| line.x < lines[0].x + KEYS).count();
    assert!(lines.len() >= keys + 2, "wanted the bar lines at both ends of the first bar after the key lines, got lines at {lines:?}");
    (lines[keys], lines[keys + 1])
}

fn beat_x(first: Rect, last: Rect, beat: f32) -> i32 {
    first.x + (beat * (last.x - first.x) as f32 / BAR_BEATS).round() as i32
}

fn notes_in(app: &mut App) -> Vec<(f32, f32)> {
    let text = app.take_requests().into_iter().rev().find_map(|request| if let Request::Remember { json } = request { Some(json) } else { None }).expect("wanted the session remembered after the press");
    let session = json::parse(&text).expect("wanted the remembered session to parse");
    let project = Project::from_json(session.get("project").expect("wanted the remembered session to hold the project")).expect("wanted the remembered project to read");
    project.tracks[0].clips[0].notes.iter().map(|note| (note.start, note.length)).collect()
}

fn drawn(snap: &str, beat: f32) -> (f32, f32) {
    let mut app = open(snap, Vec::new());
    let (first, last) = bar_lines(&painted(&mut app));
    click(&mut app, beat_x(first, last, beat), first.y + first.h / 2, 1);
    let notes = notes_in(&mut app);
    assert_eq!(notes.len(), 1, "wanted one note drawn by the tap at beat {beat} on a {snap} grid, got {notes:?}");
    notes[0]
}

#[test]
fn a_tap_draws_one_cell_of_the_grid_wherever_it_lands() {
    assert_eq!(drawn(SIXTEENTH, 2.6), (2.5, 0.25), "wanted a tap at 2.6 on a sixteenth grid to draw the cell from 2.5 to 2.75, never a whole beat");
    assert_eq!(drawn(EIGHTH, 1.3), (1.0, 0.5), "wanted a tap at 1.3 on an eighth grid to draw the cell from 1 to 1.5");
    assert_eq!(drawn(HALF, 0.1), (0.0, 2.0), "wanted a tap on a half-note grid to draw the whole two-beat cell, the grid and not a beat deciding");
}

#[test]
fn a_drag_while_drawing_stretches_the_note_by_cells() {
    let mut app = open(SIXTEENTH, Vec::new());
    let (first, last) = bar_lines(&painted(&mut app));
    let y = first.y + first.h / 2;
    let (from, to) = (beat_x(first, last, 0.1), beat_x(first, last, 1.1));
    app.event(Event::Down { x: from, y, clicks: 1 });
    app.event(Event::Move { x: to, y });
    app.event(Event::Up { x: to, y });
    app.event(Event::Leave);
    assert_eq!(notes_in(&mut app), vec![(0.0, 1.0)], "wanted the drag to stretch the sixteenth to end on the line at 1, four cells long");
}

#[test]
fn a_double_click_resets_a_note_to_one_cell() {
    let mut app = open(SIXTEENTH, vec![LONG]);
    let fills = painted(&mut app);
    let (first, _) = bar_lines(&fills);
    let note = fills.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.x == first.x + FRAME && fill.rect.w > FRAME && fill.rect.y >= first.y && fill.rect.y + fill.rect.h <= first.y + first.h).map(|fill| fill.rect).next().expect("wanted the long note painted at the bar's start");
    click(&mut app, note.x + note.w / 2, note.y + note.h / 2, 1);
    click(&mut app, note.x + note.w / 2, note.y + note.h / 2, 2);
    assert_eq!(notes_in(&mut app), vec![(0.0, 0.25)], "wanted a double-click to shrink the long note to the first sixteenth, one cell");
}
