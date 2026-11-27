use bp::app::{App, Event, Request};
use bp::arp::Articulation;
use bp::draw::{Fill, Rect, Tone};
use bp::face::{CELL, FRAME, PITCH};
use bp::json::{self, Json};
use bp::keyboard::Key;
use bp::mcp::Call;
use bp::project::{Clip, DEFAULT_BPM, Note, Project, TICKS_PER_BAR, VELOCITY};
use std::time::Duration;
use support::{clip_block, painted};

const ROW: i32 = 12;
const NOTE_W_AT_LEAST: i32 = 30;
const MIDDLE_C: u8 = 60;
const FIRST_LIFT: i32 = 3;
const SECOND_LIFT: i32 = 5;
const NOTES: &str = r#"{"path":"tracks/0/clips/0/notes"}"#;
const TEMPO: &str = r#"{"path":"bpm","value":100}"#;

fn open() -> App {
    let mut project = Project::new();
    project.tracks[0].clips.push(Clip { start: 0, length: TICKS_PER_BAR, label: String::from("one"), notes: vec![Note { pitch: MIDDLE_C, start: 0.0, length: 2.0, velocity: VELOCITY, articulation: Articulation::PLAIN }], file: None });
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false}}", project.to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    let block = clip_block(&painted(&mut app));
    let (x, y) = (block.x + block.w / 2, block.y + block.h / 2);
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    app.event(Event::Leave);
    app.take_requests();
    app
}

fn roll_top(fills: &[Fill]) -> i32 {
    let tallest = fills.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.w == FRAME).map(|fill| fill.rect.h).max().expect("wanted the roll's lines painted");
    let line = fills.iter().find(|fill| fill.tone == Tone::Ink100 && fill.rect.w == FRAME && fill.rect.h == tallest).expect("wanted a line of the roll").rect;
    line.y - PITCH
}

fn note_block(fills: &[Fill]) -> Rect {
    let top = roll_top(fills);
    fills.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.y >= top + CELL && fill.rect.h == ROW - FRAME && fill.rect.w >= NOTE_W_AT_LEAST).map(|fill| fill.rect).next().expect("wanted the note painted on the roll")
}

fn pitches(reply: &str) -> Vec<i64> {
    json::parse(reply).expect("wanted the notes to parse").as_array().expect("wanted a list of notes").iter().filter_map(|note| note.get("pitch")).filter_map(Json::as_i64).collect()
}

fn remembered_pitches(app: &mut App) -> Vec<u8> {
    let text = app.take_requests().into_iter().rev().find_map(|request| if let Request::Remember { json } = request { Some(json) } else { None }).expect("wanted the session remembered");
    let session = json::parse(&text).expect("wanted the remembered session to parse");
    let project = Project::from_json(session.get("project").expect("wanted the session to hold the project")).expect("wanted the project to read");
    project.tracks[0].clips[0].notes.iter().map(|note| note.pitch).collect()
}

fn args(text: &str) -> Json {
    json::parse(text).expect("wanted the arguments to parse")
}

#[test]
fn a_read_during_a_drag_answers_without_splitting_the_gesture_and_an_edit_waits_for_it() {
    let mut app = open();
    let note = note_block(&painted(&mut app));
    let (x, y) = (note.x + note.w / 2, note.y + note.h / 2);
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Move { x, y: y - FIRST_LIFT * ROW });
    let read = app.serve_call("read", &args(NOTES)).expect("wanted a read answered during the drag");
    assert!(!read.failed && pitches(&read.text) == vec![i64::from(MIDDLE_C) + i64::from(FIRST_LIFT)], "wanted the read to see the note where the drag holds it, got {}", read.text);
    assert!(app.serve_call("write", &args(TEMPO)).is_none(), "wanted an editing call to wait while the drag is in progress");
    app.event(Event::Move { x, y: y - SECOND_LIFT * ROW });
    app.event(Event::Up { x, y: y - SECOND_LIFT * ROW });
    app.event(Event::Leave);
    assert_eq!(remembered_pitches(&mut app), vec![MIDDLE_C + SECOND_LIFT as u8], "wanted the drag to land the note five rows up");
    let write = app.serve_call("write", &args(TEMPO)).expect("wanted the editing call served once the drag ended");
    assert!(!write.failed, "wanted the tempo written, got {}", write.text);
    app.take_requests();
    app.event(Event::Key { key: Key::Char('z'), shift: false, command: true, repeat: false });
    app.event(Event::Key { key: Key::Char('z'), shift: false, command: true, repeat: false });
    assert_eq!(remembered_pitches(&mut app), vec![MIDDLE_C], "wanted one undo for the tempo and one for the whole drag, the read in the middle never a step of its own");
}

fn remembered_bpm(app: &mut App) -> f64 {
    let text = app.take_requests().into_iter().rev().find_map(|request| if let Request::Remember { json } = request { Some(json) } else { None }).expect("wanted the session remembered");
    let session = json::parse(&text).expect("wanted the remembered session to parse");
    Project::from_json(session.get("project").expect("wanted the session to hold the project")).expect("wanted the project to read").bpm
}

fn dragging(app: &mut App) -> (i32, i32) {
    let note = note_block(&painted(app));
    let (x, y) = (note.x + note.w / 2, note.y + note.h / 2);
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Move { x, y: y - FIRST_LIFT * ROW });
    (x, y - FIRST_LIFT * ROW)
}

fn release(app: &mut App, at: (i32, i32)) {
    app.event(Event::Up { x: at.0, y: at.1 });
    app.event(Event::Leave);
}

#[test]
fn a_write_that_arrives_during_a_drag_lands_after_it_and_answers_its_client() {
    let mut app = open();
    let at = dragging(&mut app);
    let (write, waiting) = Call::new("write", args(TEMPO));
    assert!(!app.deliver(write), "wanted the write held while the drag is in progress");
    release(&mut app, at);
    assert_eq!(remembered_bpm(&mut app), DEFAULT_BPM, "wanted the tempo untouched at the drag's end, the write still waiting");
    let (read, reading) = Call::new("read", args(r#"{"path":"tracks/0/clips/0/notes"}"#));
    assert!(app.deliver(read), "wanted the held write and the read served once the drag ended");
    assert!(waiting.reply(Duration::ZERO).text.starts_with("Written at bpm"), "wanted the held write answered to its client");
    assert!(!reading.reply(Duration::ZERO).failed, "wanted the read behind it answered too");
    assert_eq!(remembered_bpm(&mut app), 100.0, "wanted the tempo written once the drag ended");
}

#[test]
fn a_write_whose_client_gave_up_never_lands_and_leaves_no_undo_step() {
    let mut app = open();
    let at = dragging(&mut app);
    let (write, waiting) = Call::new("write", args(TEMPO));
    assert!(!app.deliver(write), "wanted the write held while the drag is in progress");
    assert!(waiting.reply(Duration::from_millis(1)).failed, "wanted the client's patience to run out while the drag holds the write");
    release(&mut app, at);
    let (read, reading) = Call::new("read", args(r#"{"path":"bpm"}"#));
    assert!(app.deliver(read), "wanted the read served after the drag");
    let tempo = reading.reply(Duration::ZERO).text;
    assert!(tempo.contains("120"), "wanted the tempo still at its default after the given-up write was dropped, got {tempo}");
    assert_eq!(remembered_bpm(&mut app), DEFAULT_BPM, "wanted no write from a client that gave up");
    app.event(Event::Key { key: Key::Char('z'), shift: false, command: true, repeat: false });
    assert_eq!(remembered_pitches(&mut app), vec![MIDDLE_C], "wanted one undo to take back the whole drag, the dropped write leaving no step of its own");
}
