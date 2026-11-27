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
const CLIP_BEATS: f32 = BARS as f32 * BAR_BEATS;
const DOTTED_EIGHTH: &str = "1/8.";
const DOTTED_LINES: [f32; 10] = [0.75, 1.5, 2.25, 3.0, 3.75, 4.5, 5.25, 6.0, 6.75, 7.5];
const SHORT: Note = Note { pitch: 60, start: 0.0, length: 0.3, velocity: VELOCITY, articulation: Articulation::PLAIN };

fn open(notes: Vec<Note>) -> App {
    let mut project = Project::new();
    project.tracks[0].clips.push(Clip { start: 0, length: BARS * TICKS_PER_BAR, label: String::from("bars"), notes, file: None });
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false,\"snap\":\"{DOTTED_EIGHTH}\"}}", project.to_json().write());
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

fn clip_edges(fills: &[Fill]) -> (Rect, Rect) {
    let tallest = fills.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.w == FRAME).map(|fill| fill.rect.h).max().expect("wanted the roll's edge lines painted");
    let mut lines: Vec<Rect> = fills.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.w == FRAME && fill.rect.h == tallest).map(|fill| fill.rect).collect();
    lines.sort_by_key(|line| line.x);
    let keys = lines.iter().filter(|line| line.x < lines[0].x + KEYS).count();
    assert!(lines.len() >= keys + 2, "wanted a line at each end of the clip after the key lines, got lines at {lines:?}");
    (lines[keys], lines[keys + 1])
}

fn beat_x(first: Rect, last: Rect, beat: f32) -> i32 {
    first.x + (beat * (last.x - first.x) as f32 / CLIP_BEATS).round() as i32
}

fn grid_lines(fills: &[Fill], first: Rect) -> Vec<i32> {
    let mut lines: Vec<i32> = fills.iter().filter(|fill| matches!(fill.tone, Tone::Ink50 | Tone::Ink100) && fill.rect.y == first.y && fill.rect.h == first.h && fill.rect.x >= first.x).map(|fill| fill.rect.x).collect();
    lines.sort_unstable();
    lines.dedup();
    lines
}

fn notes_in(app: &mut App) -> Vec<(f32, f32)> {
    let text = app.take_requests().into_iter().rev().find_map(|request| if let Request::Remember { json } = request { Some(json) } else { None }).expect("wanted the session remembered after the press");
    let session = json::parse(&text).expect("wanted the remembered session to parse");
    let project = Project::from_json(session.get("project").expect("wanted the remembered session to hold the project")).expect("wanted the remembered project to read");
    project.tracks[0].clips[0].notes.iter().map(|note| (note.start, note.length)).collect()
}

fn tap_at(app: &mut App, beat: f32) -> (i32, i32) {
    let (first, last) = clip_edges(&painted(app));
    let at = (beat_x(first, last, beat), first.y + first.h / 2);
    click(app, at.0, at.1, 1);
    at
}

fn drawn(beat: f32) -> (f32, f32) {
    let mut app = open(Vec::new());
    tap_at(&mut app, beat);
    let notes = notes_in(&mut app);
    assert_eq!(notes.len(), 1, "wanted one note drawn by the tap at beat {beat}, got {notes:?}");
    notes[0]
}

#[test]
fn a_tap_on_a_dotted_grid_draws_a_note_that_ends_on_a_line() {
    assert_eq!(drawn(0.1), (0.0, 0.75), "wanted a tap at 0.1 to draw one dotted cell, from the clip's start to the line at 0.75, never a whole beat");
    assert_eq!(drawn(3.1), (3.0, 0.75), "wanted a tap at 3.1 to draw the cell from the line at 3 to the line at 3.75");
    assert_eq!(drawn(3.8), (3.75, 0.75), "wanted a tap just before the second bar to draw a whole cell across it, from 3.75 to the line at 4.5, the grid not stopping at the bar");
    assert_eq!(drawn(4.6), (4.5, 0.75), "wanted a tap at 4.6 to draw the cell from the line at 4.5, the grid counted on from the clip's start through the second bar");
    assert_eq!(drawn(7.7), (7.5, 0.5), "wanted a tap past the last line to draw the half beat left, from 7.5 to the clip's end");
}

#[test]
fn the_drawn_note_is_painted_up_to_a_grid_line() {
    let mut app = open(Vec::new());
    let (x, y) = tap_at(&mut app, 0.1);
    let fills = painted(&mut app);
    let (first, _) = clip_edges(&fills);
    let note = fills.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.w > FRAME).map(|fill| fill.rect).find(|rect| rect.contains(x, y)).expect("wanted the drawn note painted under the press");
    let lines = grid_lines(&fills, first);
    assert_eq!(note.x, first.x + FRAME, "wanted the note's left edge a frame inside the clip's first line at {}", first.x);
    assert!(lines.contains(&(note.x + note.w)), "wanted the note's right edge at {} on a painted grid line, the lines standing at {lines:?}", note.x + note.w);
}

#[test]
fn the_dotted_grid_runs_unbroken_from_the_clips_start() {
    let mut app = open(Vec::new());
    let fills = painted(&mut app);
    let (first, last) = clip_edges(&fills);
    let lines = grid_lines(&fills, first);
    let within: Vec<i32> = lines.iter().copied().filter(|x| *x > first.x && *x < last.x).collect();
    let wanted: Vec<i32> = DOTTED_LINES.iter().map(|&beat| beat_x(first, last, beat)).collect();
    assert_eq!(within.len(), wanted.len(), "wanted ten dotted lines between the clip's edges near {wanted:?}, counted on through the second bar with no bar line among them, got {within:?}");
    for (line, want) in within.iter().zip(&wanted) {
        assert!((line - want).abs() <= 1, "wanted a dotted line at {want}, got {line}, the lines standing at {within:?}");
    }
}

#[test]
fn a_double_click_resets_a_note_to_end_on_a_line() {
    let mut app = open(vec![SHORT]);
    let fills = painted(&mut app);
    let (first, _) = clip_edges(&fills);
    let note = fills.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.x == first.x + FRAME && fill.rect.w > FRAME && fill.rect.y >= first.y && fill.rect.y + fill.rect.h <= first.y + first.h).map(|fill| fill.rect).next().expect("wanted the short note painted at the clip's start");
    click(&mut app, note.x + note.w / 2, note.y + note.h / 2, 1);
    click(&mut app, note.x + note.w / 2, note.y + note.h / 2, 2);
    assert_eq!(notes_in(&mut app), vec![(0.0, 0.75)], "wanted a double-click to reset the note to one dotted cell, ending on the first line at 0.75, never on a beat between lines");
}
