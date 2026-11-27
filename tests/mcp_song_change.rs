use bp::app::{App, Event, Opened, Request};
use bp::arp::Articulation;
use bp::draw::{Fill, Rect, Tone};
use bp::face::{CELL, FRAME, PITCH};
use bp::json::{self, Json};
use bp::keyboard::Key;
use bp::mcp::Call;
use bp::project::{Clip, Note, Project, TICKS_PER_BAR, TICKS_PER_BEAT, Track, VELOCITY};
use bp::wav::Wave;
use std::f64::consts::TAU;
use std::time::Duration;
use support::{SAMPLE_RATE, call, clip_block, frames, painted, remembered, session};

const FIRST: &str = "/songs/first.json";
const SECOND: &str = "/songs/second.json";
const ROW: i32 = 12;
const NOTE_W_AT_LEAST: i32 = 30;
const MIDDLE_C: u8 = 60;
const LIFT: i32 = 3;
const SECOND_BPM: f64 = 90.0;
const TEMPO: &str = r#"{"path":"bpm","value":100}"#;
const BPM: &str = r#"{"path":"bpm"}"#;
const SAVE: &str = "{}";
const WHOLE_SONG: &str = "{}";
const REPLACED: &str = "project was replaced";
const WAVE: &str = "kick";
const LOW_HZ: f64 = 80.0;
const HIGH_HZ: f64 = 3_000.0;
const LOW_BAND: &str = "sub";
const HIGH_BAND: &str = "high";
const TONE_SECONDS: f64 = 0.5;
const TONE_PEAK: f64 = 8_192.0;
const PATIENCE: Duration = Duration::from_secs(120);
const TICK: f32 = 0.01;
const PAUSE: Duration = Duration::from_millis(2);

fn args(text: &str) -> Json {
    json::parse(text).expect("wanted the arguments to parse")
}

fn open() -> App {
    let mut project = Project::new();
    project.tracks[0].clips.push(Clip { start: 0, length: TICKS_PER_BAR, label: String::from("one"), notes: vec![Note { pitch: MIDDLE_C, start: 0.0, length: 2.0, velocity: VELOCITY, articulation: Articulation::PLAIN }], file: None });
    let session = format!("{{\"project\":{},\"path\":\"{FIRST}\",\"palette\":false,\"keyboard\":false}}", project.to_json().write());
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

fn dragging(app: &mut App) -> (i32, i32) {
    let note = note_block(&painted(app));
    let (x, y) = (note.x + note.w / 2, note.y + note.h / 2);
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Move { x, y: y - LIFT * ROW });
    (x, y - LIFT * ROW)
}

fn release(app: &mut App, at: (i32, i32)) {
    app.event(Event::Up { x: at.0, y: at.1 });
    app.event(Event::Leave);
}

fn slower_song() -> String {
    let mut project = Project::new();
    project.bpm = SECOND_BPM;
    project.to_json().write()
}

fn held(text: &str) -> Project {
    Project::from_json(&json::parse(text).expect("wanted the song's text to parse")).expect("wanted the song's text to read as a project")
}

fn opened(app: &mut App, path: &str, text: &str) {
    let opened = app.opened(String::from(path), Some(text), None);
    assert_eq!(opened, Opened::Project, "wanted {path} to open as a song, got {opened:?}");
}

fn tempo(app: &mut App) -> f64 {
    let reply = call(app, "read", BPM);
    json::parse(&reply.text).and_then(|tempo| tempo.as_f64()).unwrap_or_else(|| panic!("wanted the tempo read as a number, got {}", reply.text))
}

fn tone(hz: f64) -> Vec<i16> {
    (0..frames(TONE_SECONDS)).map(|frame| (TONE_PEAK * (TAU * hz * frame as f64 / SAMPLE_RATE).sin()).round() as i16).collect()
}

fn song(hz: f64) -> String {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0] = Track::audio();
    project.tracks[0].clips.push(Clip::audio(0, TICKS_PER_BEAT, String::from(WAVE), String::from(WAVE)));
    let mut document = project.to_json();
    if let Json::Object(fields) = &mut document {
        fields.push((String::from("waves"), Json::Array(vec![Wave::sound(String::from(WAVE), SAMPLE_RATE, 1, tone(hz)).to_json()])));
    }
    document.write()
}

fn launched() -> App {
    let mut app = App::new();
    painted(&mut app);
    app.take_requests();
    app
}

fn listened(app: &mut App) -> String {
    let (listen, waiting) = Call::new("listen", args(WHOLE_SONG));
    app.deliver(listen);
    let answer = std::thread::spawn(move || waiting.reply(PATIENCE));
    while !answer.is_finished() {
        app.tick(TICK);
        std::thread::sleep(PAUSE);
    }
    let reply = answer.join().expect("wanted the listen answered");
    assert!(!reply.failed, "wanted the listen answered, got {}", reply.text);
    reply.text
}

fn bands(heard: &str) -> Vec<(String, f64)> {
    let answer = json::parse(heard).unwrap_or_else(|| panic!("wanted listen to answer JSON, got {heard}"));
    let Some(Json::Object(bands)) = answer.get("bandsDb") else {
        panic!("wanted the listen to tell its bands, got {heard}");
    };
    bands.iter().filter_map(|(name, level)| Some((name.clone(), level.as_f64()?))).collect()
}

fn loudest(heard: &str) -> String {
    bands(heard).into_iter().max_by(|a, b| a.1.total_cmp(&b.1)).map(|(name, _)| name).unwrap_or_else(|| panic!("wanted a level for each band, got {heard}"))
}

#[test]
fn a_write_held_by_a_drag_is_refused_when_another_song_is_opened_and_never_lands_in_it() {
    let mut app = open();
    let at = dragging(&mut app);
    let (write, waiting) = Call::new("write", args(TEMPO));
    assert!(!app.deliver(write), "wanted the write held while the drag is in progress");
    let file = slower_song();
    opened(&mut app, SECOND, &file);
    release(&mut app, at);
    app.tick(TICK);
    let reply = waiting.reply(Duration::ZERO);
    let tempo = tempo(&mut app);
    assert!(reply.failed && reply.text.contains(REPLACED), "wanted the waiting write refused and its client told the project was replaced, got {:?} with the opened song's tempo at {tempo}", reply.text);
    assert_eq!(tempo, SECOND_BPM, "wanted the opened song's tempo as its file holds it");
    let kept = remembered(&mut app);
    assert!(kept == held(&file), "wanted the opened song left just as its file holds it, got one of {} tracks at tempo {}", kept.tracks.len(), kept.bpm);
}

#[test]
fn a_read_waiting_behind_that_write_is_answered_from_the_song_that_was_opened() {
    let mut app = open();
    let at = dragging(&mut app);
    let (write, _writing) = Call::new("write", args(TEMPO));
    assert!(!app.deliver(write), "wanted the write held while the drag is in progress");
    let (read, reading) = Call::new("read", args(BPM));
    assert!(!app.deliver(read), "wanted the read to wait its turn behind the held write");
    opened(&mut app, SECOND, &slower_song());
    release(&mut app, at);
    app.tick(TICK);
    let reply = reading.reply(Duration::ZERO);
    assert!(!reply.failed, "wanted the read behind the refused write answered, not refused with it, got {}", reply.text);
    assert_eq!(json::parse(&reply.text).and_then(|tempo| tempo.as_f64()), Some(SECOND_BPM), "wanted the read answered from the opened song, its tempo as its file holds it, got {}", reply.text);
}

#[test]
fn a_save_held_by_a_drag_is_refused_when_another_song_is_opened_and_writes_no_file() {
    let mut app = open();
    let at = dragging(&mut app);
    let (save, waiting) = Call::new("save", args(SAVE));
    assert!(!app.deliver(save), "wanted the save held while the drag is in progress");
    opened(&mut app, SECOND, &slower_song());
    release(&mut app, at);
    app.tick(TICK);
    let reply = waiting.reply(Duration::ZERO);
    let written: Vec<String> = app.take_requests().into_iter().filter_map(|request| if let Request::Write { path, .. } = request { Some(path) } else { None }).collect();
    assert!(reply.failed && reply.text.contains(REPLACED), "wanted the waiting save refused and its client told the project was replaced, got {:?} with files written at {written:?}", reply.text);
    assert!(written.is_empty(), "wanted no file written for a save asked of the song that was left, got {written:?}");
}

#[test]
fn a_song_opened_after_another_of_the_same_text_is_heard_with_its_own_audio() {
    let (low, high) = (song(LOW_HZ), song(HIGH_HZ));
    assert!(held(&low) == held(&high), "wanted the two files to hold one project, apart only in the audio under {WAVE}");
    let mut app = launched();
    opened(&mut app, FIRST, &low);
    let first = listened(&mut app);
    assert_eq!(loudest(&first), LOW_BAND, "wanted the first song heard with its low tone, loudest in {LOW_BAND}, got {:?}", bands(&first));
    opened(&mut app, SECOND, &high);
    let second = listened(&mut app);
    assert_eq!(loudest(&second), HIGH_BAND, "wanted the second song heard with its own high tone, loudest in {HIGH_BAND}, not answered with what the first was told, {:?}; got {:?}", bands(&first), bands(&second));
}

#[test]
fn a_listen_asked_twice_of_an_unchanged_song_is_answered_the_same_with_no_second_render() {
    let mut app = launched();
    opened(&mut app, FIRST, &song(LOW_HZ));
    let first = listened(&mut app);
    let (listen, waiting) = Call::new("listen", args(WHOLE_SONG));
    app.deliver(listen);
    let again = waiting.reply(Duration::ZERO);
    assert!(!again.failed, "wanted the repeat answered at once from what the first listen kept, got {}", again.text);
    assert_eq!(again.text, first, "wanted the repeat told what the first listen was told");
}

#[test]
fn undo_back_to_the_song_before_is_heard_with_its_own_audio_not_what_was_kept_for_the_song_left() {
    let mut app = launched();
    opened(&mut app, FIRST, &song(LOW_HZ));
    opened(&mut app, SECOND, &song(HIGH_HZ));
    let second = listened(&mut app);
    assert_eq!(loudest(&second), HIGH_BAND, "wanted the second song heard with its high tone, loudest in {HIGH_BAND}, got {:?}", bands(&second));
    app.event(Event::Key { key: Key::Char('z'), shift: false, command: true, repeat: false });
    let back = session(&mut app);
    assert_eq!(back.get("path").and_then(Json::as_str), Some(FIRST), "wanted undo to return to the first song's file");
    let first = listened(&mut app);
    assert_eq!(loudest(&first), LOW_BAND, "wanted the first song, returned to by undo, heard with its own low tone, loudest in {LOW_BAND}, not answered with what the second was told, {:?}; got {:?}", bands(&second), bands(&first));
}
