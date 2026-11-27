use bp::app::App;
use bp::json::{self, Json};
use bp::project::{Project, VELOCITY};
use support::call;

const PATTERN_VELOCITY: f64 = 100.0;

fn open() -> App {
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false}}", Project::new().to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.take_requests();
    app
}

fn clip(notes: &str) -> String {
    format!(r#"{{"path":"tracks/0/clips/0","value":{{"startBeat":0,"lengthBeats":1,"name":"one","notes":{notes}}}}}"#)
}

fn number(note: &Json, key: &str) -> f64 {
    note.get(key).and_then(Json::as_f64).unwrap_or_else(|| panic!("wanted {key} in {}", note.write()))
}

fn held(app: &mut App) -> Vec<(f64, f64, f64, f64, bool)> {
    let read = call(app, "read", r#"{"path":"tracks/0/clips/0/notes"}"#);
    let read = json::parse(&read.text).unwrap_or_else(|| panic!("wanted the notes read as JSON, got {}", read.text));
    read.as_array().unwrap_or(&[]).iter().map(|note| (number(note, "pitch"), number(note, "startBeat"), number(note, "lengthBeats"), number(note, "velocity"), note.get("slide").and_then(Json::as_bool).unwrap_or(false))).collect()
}

fn notes(notes: &str) -> Vec<(f64, f64, f64, f64, bool)> {
    let mut app = open();
    let reply = call(&mut app, "write", &clip(notes));
    assert!(!reply.failed, "wanted the clip written from {notes}, got {}", reply.text);
    held(&mut app)
}

#[test]
fn a_note_written_as_a_list_reads_back_as_a_note() {
    let made = notes("[[60, 0, 0.5, 90], [64, 0.5, 0.5]]");
    assert_eq!(made, vec![(60.0, 0.0, 0.5, 90.0, false), (64.0, 0.5, 0.5, f64::from(VELOCITY), false)], "wanted each list as pitch, start, length and velocity, the velocity left out at its default");
}

#[test]
fn an_x_in_capitals_strikes_at_the_accent() {
    let made = notes(r#"{"steps": "x.X.", "pitch": 36, "velocity": 80, "accent": 120}"#);
    assert_eq!(made, vec![(36.0, 0.0, 0.25, 80.0, false), (36.0, 0.5, 0.25, 120.0, false)], "wanted the x at its velocity and the X at the accent");
}

#[test]
fn a_tilde_slides_a_note_into_the_next_and_a_line_deals_its_pitches_in_turn() {
    let made = notes(r#"{"steps": "x~x..", "pitches": [45, 57], "arp": true}"#);
    assert_eq!(made, vec![(45.0, 0.0, 0.25, PATTERN_VELOCITY, false), (57.0, 0.25, 0.25, PATTERN_VELOCITY, true)], "wanted the first note ending where the second starts and the second sliding on from it, the line's pitches one per hit");
}

#[test]
fn swing_lays_every_second_step_late_and_keeps_its_end() {
    let made = notes(r#"{"steps": "xxxx", "pitch": 60, "swing": 0.5}"#);
    let wanted = vec![(60.0, 0.0, 0.25, PATTERN_VELOCITY, false), (60.0, 0.375, 0.125, PATTERN_VELOCITY, false), (60.0, 0.5, 0.25, PATTERN_VELOCITY, false), (60.0, 0.875, 0.125, PATTERN_VELOCITY, false)];
    assert_eq!(made, wanted, "wanted the second and fourth sixteenths half a step late, each ending where its step ends");
}

#[test]
fn a_tilde_after_a_rest_is_refused() {
    let mut app = open();
    let reply = call(&mut app, "write", &clip(r#"{"steps": ".~x.", "pitch": 60}"#));
    assert!(reply.failed && reply.text.contains("~ must follow x, X or -"), "wanted a slide with no note before it refused, got {}", reply.text);
}

#[test]
fn a_compact_note_written_at_a_notes_own_path_takes_that_notes_place_and_one_at_the_end_is_added() {
    let mut app = open();
    let reply = call(&mut app, "write", &clip("[[60, 0, 0.5, 90], [64, 0.5, 0.5]]"));
    assert!(!reply.failed, "wanted the clip written, got {}", reply.text);
    let over = call(&mut app, "write", r#"{"path":"tracks/0/clips/0/notes/1","value":[72, 0.25, 0.25, 80]}"#);
    assert!(!over.failed, "wanted a compact note written over the second note, got {}", over.text);
    assert_eq!(held(&mut app), vec![(60.0, 0.0, 0.5, 90.0, false), (72.0, 0.25, 0.25, 80.0, false)], "wanted the second note replaced by the compact note and the first left");
    let added = call(&mut app, "write", r#"{"path":"tracks/0/clips/0/notes/end","value":[67, 0.75, 0.25]}"#);
    assert!(!added.failed && added.text.starts_with("Written at tracks/0/clips/0/notes/2"), "wanted a compact note added at the end of the notes, got {}", added.text);
    assert_eq!(held(&mut app), vec![(60.0, 0.0, 0.5, 90.0, false), (72.0, 0.25, 0.25, 80.0, false), (67.0, 0.75, 0.25, f64::from(VELOCITY), false)], "wanted the third note added, its velocity left out at its default");
}

#[test]
fn a_pattern_with_arp_true_brings_no_warning_of_an_arp_that_is_off_and_a_rack_rows_arp_still_does() {
    let mut app = open();
    let reply = call(&mut app, "write", &clip(r#"{"steps": "x.x.", "pitches": [60, 64, 67], "arp": true}"#));
    assert!(!reply.failed && !reply.text.contains("is off") && !reply.text.contains("not heard"), "wanted the pattern's own arp flag to bring no warning, got {}", reply.text);
    let row = call(&mut app, "write", r#"{"path":"tracks/0/rack/0/arp","value":{"gate":0.25}}"#);
    assert!(!row.failed && row.text.contains("tracks/0/rack/0/arp is off; set on true"), "wanted a rack row's arp written without on told as off, got {}", row.text);
}
