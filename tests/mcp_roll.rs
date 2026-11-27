use bp::app::App;
use bp::arp::Articulation;
use bp::json::{self, Json};
use bp::project::{Clip, Note, Project, TICKS_PER_BEAT, Track, VELOCITY};
use support::call;

const BAR: i32 = 4 * TICKS_PER_BEAT;
const LOOSE: usize = 0;
const HEMMED: usize = 1;
const SHORT: usize = 2;
const LINE: usize = 3;
const AUDIO_TRACK: usize = 1;
const HALF_BEAT: f32 = 0.5;
const OFF_THE_GRID: [(u8, f32, f32); 4] = [(60, 0.1, 0.5), (64, 0.9, 0.5), (67, 2.0, 1.0), (72, 3.3, 0.5)];
const ON_THE_GRID: [(u8, f32, f32); 4] = [(60, 0.0, 0.5), (64, 1.0, 0.5), (67, 2.0, 1.0), (72, 3.5, 0.5)];
const FIRST_BEAT_ON_THE_GRID: [(u8, f32, f32); 4] = [(60, 0.0, 0.5), (64, 1.0, 0.5), (67, 2.0, 1.0), (72, 3.3, 0.5)];
const DOUBLED: [(u8, f32, f32); 4] = [(60, 0.2, 1.0), (64, 1.8, 1.0), (67, 4.0, 2.0), (72, 6.6, 1.0)];
const HALVED: [(u8, f32, f32); 4] = [(60, 0.05, 0.25), (64, 0.45, 0.25), (67, 1.0, 0.5), (72, 1.65, 0.25)];
const REVERSED: [(u8, f32, f32); 4] = [(72, 0.2, 0.5), (67, 1.0, 1.0), (64, 2.6, 0.5), (60, 3.4, 0.5)];
const ARPED: [(u8, f32, f32); 3] = [(60, 0.0, 1.0), (64, 1.0, 1.0), (67, 2.0, 1.0)];

fn notes(written: &[(u8, f32, f32)]) -> Vec<Note> {
    written.iter().map(|&(pitch, start, length)| Note { pitch, start, length, velocity: VELOCITY, articulation: Articulation::PLAIN }).collect()
}

fn clip(start: i32, length: i32, written: &[(u8, f32, f32)]) -> Clip {
    Clip { start, length, label: String::new(), notes: notes(written), file: None }
}

fn song() -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].clips = vec![clip(0, BAR, &OFF_THE_GRID), clip(2 * BAR, BAR, &OFF_THE_GRID), clip(3 * BAR, TICKS_PER_BEAT, &[(60, 0.0, 0.5)]), clip(4 * BAR, BAR, &ARPED)];
    let mut take = Track::audio();
    take.name = String::from("AUDIO 2");
    take.clips.push(Clip::audio(0, BAR, String::from("take"), String::from("take")));
    project.tracks.push(take);
    let text = project.to_json().write();
    Project::from_json(&json::parse(&text).expect("wanted the song to parse")).expect("wanted the song to read")
}

fn open() -> App {
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false}}", song().to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.take_requests();
    app
}

fn at(slot: usize) -> String {
    format!("tracks/0/clips/{slot}")
}

fn did(app: &mut App, args: &str) -> String {
    let reply = call(app, "write", args);
    assert!(!reply.failed, "wanted the write taken: {args}, got {}", reply.text);
    reply.text
}

fn refused(app: &mut App, args: &str) -> String {
    let reply = call(app, "write", args);
    assert!(reply.failed, "wanted the write refused: {args}, got {}", reply.text);
    reply.text
}

fn tick(beats: f64) -> i32 {
    (beats * f64::from(TICKS_PER_BEAT)).round() as i32
}

fn sorted(mut notes: Vec<(u8, i32, i32)>) -> Vec<(u8, i32, i32)> {
    notes.sort_unstable_by_key(|&(pitch, start, _)| (start, pitch));
    notes
}

fn sounding(clip: &Clip) -> (i32, Vec<(u8, i32, i32)>) {
    (clip.length, sorted(clip.notes.iter().map(|note| (note.pitch, tick(f64::from(note.start)), tick(f64::from(note.length)))).collect()))
}

fn landed(app: &mut App, slot: usize) -> (i32, Vec<(u8, i32, i32)>) {
    let reply = call(app, "read", &format!(r#"{{"path":"{}"}}"#, at(slot)));
    let clip = json::parse(&reply.text).unwrap_or_else(|| panic!("wanted {} read, got {}", at(slot), reply.text));
    let number = |json: &Json, key: &str| json.get(key).and_then(Json::as_f64).unwrap_or_else(|| panic!("wanted {key} in {}", json.write()));
    let notes = clip.get("notes").and_then(Json::as_array).unwrap_or(&[]).iter().map(|note| (number(note, "pitch") as u8, tick(number(note, "startBeat")), tick(number(note, "lengthBeats"))));
    (tick(number(&clip, "lengthBeats")), sorted(notes.collect()))
}

fn operation(reply: &str) -> &str {
    reply.lines().skip_while(|line| *line != "Operations:").nth(1).map_or("", str::trim)
}

fn assert_left(app: &mut App, slot: usize, wanted: &Clip, what: &str) {
    assert_eq!(landed(app, slot), sounding(wanted), "wanted {} left as the piano roll's own {what} leaves it, its length and its notes as pitch, start and length in ticks", at(slot));
}

#[test]
fn quantize_snaps_a_clips_notes_starts_and_ends_to_the_grid_and_tells_how_many_moved() {
    let mut app = open();
    let reply = did(&mut app, &format!(r#"{{"path":"{}","quantize":{HALF_BEAT}}}"#, at(LOOSE)));
    assert_eq!(operation(&reply), format!("Quantized {} to {HALF_BEAT} beats: 3 of 4 notes moved onto the grid", at(LOOSE)), "wanted the reply to count the three notes moved, got {reply}");
    assert_left(&mut app, LOOSE, &clip(0, BAR, &ON_THE_GRID), "quantize");
    let again = did(&mut app, &format!(r#"{{"path":"{}","quantize":{HALF_BEAT}}}"#, at(LOOSE)));
    assert!(again.contains("every note sits on the grid already"), "wanted a second quantize to say nothing moved, got {again}");
}

#[test]
fn quantize_takes_a_span_of_beats_a_track_or_the_whole_song() {
    let mut app = open();
    let reply = did(&mut app, &format!(r#"{{"path":"{}","quantize":{HALF_BEAT},"fromBeat":0,"untilBeat":1}}"#, at(LOOSE)));
    assert!(reply.contains("2 of 2 notes"), "wanted the two notes starting in the first beat chosen and moved, got {reply}");
    assert_left(&mut app, LOOSE, &clip(0, BAR, &FIRST_BEAT_ON_THE_GRID), "quantize of the notes picked");
    did(&mut app, &format!(r#"{{"quantize":{HALF_BEAT}}}"#));
    assert_left(&mut app, LOOSE, &clip(0, BAR, &ON_THE_GRID), "quantize with no path, the whole song");
    assert_left(&mut app, HEMMED, &clip(2 * BAR, BAR, &ON_THE_GRID), "quantize with no path, the whole song");
    let mut app = open();
    did(&mut app, &format!(r#"{{"path":"tracks/0","quantize":{HALF_BEAT}}}"#));
    assert_left(&mut app, HEMMED, &clip(2 * BAR, BAR, &ON_THE_GRID), "quantize at its track");
    let none = refused(&mut app, &format!(r#"{{"path":"tracks/{AUDIO_TRACK}","quantize":{HALF_BEAT}}}"#));
    assert!(none.contains("has no notes"), "wanted a track of audio refused for want of notes, got {none}");
    let grid = refused(&mut app, &format!(r#"{{"path":"{}","quantize":0}}"#, at(LOOSE)));
    assert!(grid.contains("grid in beats"), "wanted a grid of nothing refused, got {grid}");
}

#[test]
fn double_stretches_a_clip_and_its_notes_and_is_refused_where_the_next_clip_leaves_no_room() {
    let mut app = open();
    let reply = did(&mut app, &format!(r#"{{"path":"{}","double":true}}"#, at(LOOSE)));
    assert_eq!(operation(&reply), format!("Doubled {} to 8 beats, bars 1–2: 4 notes stretched with it", at(LOOSE)), "wanted the reply to give the new length and the notes stretched, got {reply}");
    assert_left(&mut app, LOOSE, &clip(0, 2 * BAR, &DOUBLED), "times two");
    let hemmed = refused(&mut app, &format!(r#"{{"path":"{}","double":true}}"#, at(HEMMED)));
    assert!(hemmed.contains("has no room to double") && hemmed.contains("the next clip starts at beat 12"), "wanted a clip with the next one at its end refused, naming where that starts, got {hemmed}");
    assert_left(&mut app, HEMMED, &clip(2 * BAR, BAR, &OFF_THE_GRID), "refusal");
}

#[test]
fn halve_shrinks_a_clip_and_its_notes_and_leaves_one_under_two_beats() {
    let mut app = open();
    let reply = did(&mut app, &format!(r#"{{"path":"{}","halve":true}}"#, at(LOOSE)));
    assert_eq!(operation(&reply), format!("Halved {} to 2 beats, bar 1: 4 notes shrunk with it", at(LOOSE)), "wanted the reply to give the new length and the notes shrunk, got {reply}");
    assert_left(&mut app, LOOSE, &clip(0, BAR / 2, &HALVED), "halve");
    let short = refused(&mut app, &format!(r#"{{"path":"{}","halve":true}}"#, at(SHORT)));
    assert!(short.contains("is 1 beat long"), "wanted a clip of one beat refused, its length said in the singular, got {short}");
}

#[test]
fn reverse_turns_a_clips_notes_back_to_front_as_the_roll_does() {
    let mut app = open();
    let reply = did(&mut app, &format!(r#"{{"path":"{}","reverse":true}}"#, at(LOOSE)));
    assert_eq!(operation(&reply), format!("Reversed {}: 4 notes turned back to front in time", at(LOOSE)), "wanted the reply to count the notes turned, got {reply}");
    assert_left(&mut app, LOOSE, &clip(0, BAR, &REVERSED), "reverse");
}

#[test]
fn chords_gathers_a_line_into_the_chords_it_spells_as_the_roll_does() {
    let mut app = open();
    let line = song().tracks[0].clips[LINE].clone();
    let chords = line.bar_chords();
    assert!(chords.len() == ARPED.len() && chords.iter().all(|note| note.start == 0.0 && note.length == 4.0), "wanted the line's three notes to spell one chord held for its bar, got {chords:?}");
    let wanted = Clip { notes: chords, ..line };
    let reply = did(&mut app, &format!(r#"{{"path":"{}","chords":true}}"#, at(LINE)));
    assert_eq!(operation(&reply), format!("Turned {} into chords: 3 notes gathered into 1 chord of 3 notes", at(LINE)), "wanted the reply to count the chord gathered, in the singular, got {reply}");
    assert_left(&mut app, LINE, &wanted, "chords");
    let held = refused(&mut app, &format!(r#"{{"path":"{}","chords":true}}"#, at(LINE)));
    assert!(held.contains("holds those chords already"), "wanted a clip that is its chords already refused, got {held}");
}

#[test]
fn the_roll_edits_refuse_an_audio_clip_a_span_of_notes_and_a_path_that_is_no_clip() {
    let mut app = open();
    for edit in ["double", "halve", "reverse", "chords"] {
        let wave = refused(&mut app, &format!(r#"{{"path":"tracks/{AUDIO_TRACK}/clips/0","{edit}":true}}"#));
        assert!(wave.contains(&format!("plays a wave; {edit} takes a MIDI clip")), "wanted {edit} refused at an audio clip, got {wave}");
        let spanned = refused(&mut app, &format!(r#"{{"path":"{}","{edit}":true,"fromBeat":0,"untilBeat":1}}"#, at(LOOSE)));
        assert!(spanned.contains(&format!("{edit} takes the whole clip")), "wanted {edit} refused over a span of notes, got {spanned}");
        let track = refused(&mut app, &format!(r#"{{"path":"tracks/0","{edit}":true}}"#));
        assert!(track.contains(&format!("{edit} takes a MIDI clip like tracks/N/clips/K")), "wanted {edit} refused at a track, got {track}");
    }
}
