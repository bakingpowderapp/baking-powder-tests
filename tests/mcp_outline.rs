use bp::app::App;
use bp::arp::Articulation;
use bp::fx::Kind;
use bp::json::{self, Json};
use bp::project::{Clip, Note, Project, TICKS_PER_BAR, VELOCITY};
use support::{call, effect};

const TWO_BARS: i32 = 2 * TICKS_PER_BAR;
const FOLDED_NOTES: &str = "folded 3 notes, C4 to G4 at tracks/0/clips/0/notes";
const FOLDED_RACK: &str = "folded 3 rows, midi, core, delay at tracks/0/rack";
const FOLDED_CLIPS: &str = "folded 1 clip, beats 0 to 8 at tracks/0/clips";

fn note(pitch: u8, start: f32, length: f32) -> Note {
    Note { pitch, start, length, velocity: VELOCITY, articulation: Articulation::PLAIN }
}

fn open() -> App {
    let mut project = Project::new();
    project.tracks[0].rack.push(effect(Kind::Delay));
    project.tracks[0].clips.push(Clip { start: 0, length: TWO_BARS, label: String::from("phrase"), notes: vec![note(60, 0.0, 2.0), note(64, 2.0, 2.0), note(67, 4.0, 4.0)], file: None });
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false}}", project.to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.take_requests();
    app
}

fn answered(app: &mut App, tool: &str, args: &str) -> Json {
    let reply = call(app, tool, args);
    assert!(!reply.failed, "wanted {tool} {args} answered, got {}", reply.text);
    json::parse(&reply.text).unwrap_or_else(|| panic!("wanted {tool} {args} to answer JSON, got {}", reply.text))
}

fn written(app: &mut App, args: &str) -> String {
    let reply = call(app, "write", args);
    assert!(!reply.failed, "wanted write {args} taken, got {}", reply.text);
    reply.text
}

fn first<'a>(json: &'a Json, key: &str) -> &'a Json {
    json.get(key).and_then(Json::as_array).and_then(<[Json]>::first).unwrap_or_else(|| panic!("wanted {key} to hold a first item in {}", json.write()))
}

fn second<'a>(json: &'a Json, key: &str) -> &'a Json {
    json.get(key).and_then(Json::as_array).and_then(|items| items.get(1)).unwrap_or_else(|| panic!("wanted {key} to hold a second item in {}", json.write()))
}

#[test]
fn an_outline_folds_clips_and_racks_to_lines_naming_their_paths_and_a_plain_read_folds_notes() {
    let mut app = open();
    let outline = answered(&mut app, "read", r#"{"outline":true}"#);
    let track = first(&outline, "tracks");
    assert_eq!(track.get("clips").and_then(Json::as_str), Some(FOLDED_CLIPS), "wanted the clips folded to a line counting them, spanning their beats and naming their path");
    assert_eq!(track.get("rack").and_then(Json::as_str), Some(FOLDED_RACK), "wanted the rack folded to a line counting its rows and naming their kinds");
    assert_eq!(outline.get("selectedTrack").and_then(Json::as_f64), Some(0.0), "wanted the selected track in the document");
    assert_eq!(outline.get("endBeat").and_then(Json::as_f64), Some(8.0), "wanted endBeat at the clip's end");
    let with_notes = answered(&mut app, "read", r#"{"outline":true,"notes":true}"#);
    let clip = first(first(&with_notes, "tracks"), "clips");
    assert_eq!(first(clip, "notes").get("pitch").and_then(Json::as_f64), Some(60.0), "wanted notes true to keep the clips and their notes whole under an outline, got {}", clip.write());
    let plain = answered(&mut app, "read", "{}");
    assert_eq!(first(first(&plain, "tracks"), "clips").get("notes").and_then(Json::as_str), Some(FOLDED_NOTES), "wanted a plain read's notes folded to a line counting them and naming their path");
    assert!(first(&plain, "tracks").get("rack").and_then(Json::as_array).is_some(), "wanted the rack whole without outline");
}

#[test]
fn a_folded_line_written_back_copies_what_it_names() {
    let mut app = open();
    let reply = written(&mut app, &format!(r#"{{"path":"tracks/0/clips/1","value":{{"startBeat":8,"lengthBeats":8,"name":"again","notes":"{FOLDED_NOTES}"}}}}"#));
    assert!(reply.starts_with("Written at tracks/0/clips/1") && !reply.contains("Kept differently"), "wanted the copy written as sent, got {reply}");
    let copy = answered(&mut app, "read", r#"{"path":"tracks/0/clips/1/notes"}"#);
    let original = answered(&mut app, "read", r#"{"path":"tracks/0/clips/0/notes"}"#);
    assert_eq!(copy, original, "wanted the copy's notes to be the original's");
}

#[test]
fn a_line_whose_content_has_changed_is_refused() {
    let mut app = open();
    written(&mut app, r#"{"path":"tracks/0/clips/0/notes","value":[{"pitch":60,"startBeat":0,"lengthBeats":8}]}"#);
    let stale = call(&mut app, "write", &format!(r#"{{"path":"tracks/0/clips/1","value":{{"startBeat":8,"lengthBeats":8,"notes":"{FOLDED_NOTES}"}}}}"#));
    assert!(stale.failed && stale.text.contains("3 notes, C4 to G4") && stale.text.contains("1 note, C4 to C4"), "wanted the stale line refused naming what it said and what is there, got {}", stale.text);
}

#[test]
fn a_name_that_begins_with_folded_stays_a_name() {
    let mut app = open();
    written(&mut app, &format!(r#"{{"path":"tracks/0/clips/0/name","value":"{FOLDED_NOTES}"}}"#));
    let name = answered(&mut app, "read", r#"{"path":"tracks/0/clips/0/name"}"#);
    assert_eq!(name.as_str(), Some(FOLDED_NOTES), "wanted the name kept as text, not taken for a reference");
}

#[test]
fn the_manual_answers_at_a_clips_path_and_an_unwritten_key_reads_as_its_default() {
    let mut app = open();
    let clip = answered(&mut app, "manual", r#"{"path":"tracks/0/clips/0"}"#);
    assert!(clip.get("startBeat").is_some() && clip.get("notes").is_some(), "wanted the clip's shape at the clip's own path, got {}", clip.write());
    let mute = call(&mut app, "read", r#"{"path":"tracks/0/mute"}"#);
    assert!(!mute.failed && mute.text.contains("default false"), "wanted an unwritten mute read as its default, got {}", mute.text);
    let whole = answered(&mut app, "manual", "{}");
    let rack = second(second(&whole, "tracks"), "rack").as_str().expect("wanted the manual's rack shapes folded to one line");
    assert!(rack.starts_with("folded ") && rack.contains("midi") && rack.contains("core") && rack.contains("delay"), "wanted the folded rack shapes to name their kinds, the MIDI row among them, got {rack}");
}

#[test]
fn a_write_warns_of_clips_overlapping_and_notes_past_the_end() {
    let mut app = open();
    let reply = written(&mut app, r#"{"path":"tracks/0/clips/1","value":{"startBeat":4,"lengthBeats":8,"notes":[{"pitch":60,"startBeat":9,"lengthBeats":1}]}}"#);
    assert!(reply.contains("both play at beats 4–8") && reply.contains("its note starts outside lengthBeats 8"), "wanted the reply to name the overlap and the note past the end, got {reply}");
}

fn rack_kinds(app: &mut App, track: usize) -> Vec<String> {
    let rack = answered(app, "read", &format!(r#"{{"path":"tracks/{track}/rack"}}"#));
    rack.as_array().unwrap_or(&[]).iter().map(|row| String::from(row.get("kind").and_then(Json::as_str).unwrap_or("?"))).collect()
}

#[test]
fn a_folded_line_written_as_the_whole_value_at_its_own_key_copies_what_it_names() {
    let mut app = open();
    written(&mut app, r#"{"path":"tracks/0/clips/1","value":{"startBeat":8,"lengthBeats":8,"name":"again","notes":[]}}"#);
    let reply = written(&mut app, &format!(r#"{{"path":"tracks/0/clips/1/notes","value":"{FOLDED_NOTES}"}}"#));
    assert!(!reply.contains("Kept differently"), "wanted the line taken as the notes it names, got {reply}");
    let copy = answered(&mut app, "read", r#"{"path":"tracks/0/clips/1/notes"}"#);
    let original = answered(&mut app, "read", r#"{"path":"tracks/0/clips/0/notes"}"#);
    assert!(copy == original && copy.as_array().is_some_and(|notes| notes.len() == 3), "wanted the second clip's notes to be the first's three, got {}", copy.write());
    written(&mut app, &format!(r#"{{"path":"tracks/1/rack","value":"{FOLDED_RACK}"}}"#));
    assert_eq!(rack_kinds(&mut app, 1), ["midi", "core", "delay"], "wanted the second track's rack to be the first's three rows");
}

#[test]
fn a_folded_line_written_at_a_key_it_does_not_stand_for_is_refused_and_clears_nothing() {
    let mut app = open();
    let notes = call(&mut app, "write", &format!(r#"{{"path":"tracks/0/clips/0/notes","value":"{FOLDED_RACK}"}}"#));
    assert!(notes.failed && notes.text.contains("is not notes"), "wanted a rack's line written at a clip's notes refused, got {}", notes.text);
    let rack = call(&mut app, "write", &format!(r#"{{"path":"tracks/0/rack","value":"{FOLDED_NOTES}"}}"#));
    assert!(rack.failed && rack.text.contains("is not rack"), "wanted a clip's notes line written at a rack refused, got {}", rack.text);
    let kept = answered(&mut app, "read", r#"{"path":"tracks/0/clips/0/notes"}"#);
    assert_eq!(kept.as_array().map(<[Json]>::len), Some(3), "wanted the clip's three notes left, got {}", kept.write());
    assert_eq!(rack_kinds(&mut app, 0), ["midi", "core", "delay"], "wanted the rack's three rows left");
}
