use bp::app::{App, Opened};
use bp::arp::Arp;
use bp::fx::{Kind, Params, Slot};
use bp::json::{self, Json};
use bp::project::{Clip, Project, Row, TICKS_PER_BEAT, Track};
use bp::wav::Wave;
use std::f64::consts::TAU;
use support::{HEIGHT, LEVEL, SAMPLE_RATE, WIDTH, call, core, effect, frames, midi};

const SONG: &str = "/match/songs/song.json";
const NAME: &str = "strikes";
const FILE_SECONDS: f64 = 4.0;
const STRUCK_SECONDS: [f64; 3] = [0.5, 1.0, 1.5];
const SOUNDING: usize = 4_800;
const STRIKE_HZ: f64 = 1_000.0;
const STRIKE_PEAK: f64 = 16_384.0;
const LIKE: usize = 0;
const ALIKE: usize = 1;
const LONG: usize = 2;
const LATER_BY: f64 = STRUCK_SECONDS[1] - STRUCK_SECONDS[0];
const RISE: (f64, f64) = (0.5, 0.51);
const FALL: (f64, f64) = (0.55, 0.6);
const SEMITONES: f64 = 3.0;
const SOURCE: usize = 0;
const OTHER_SOURCE: usize = 1;
const DUCK_ROW: usize = 2;
const DUCK_FROM_DB: f64 = -40.0;
const DUCK_RATIO: f64 = 20.0;

fn strikes() -> Wave {
    let mut pcm = vec![0i16; frames(FILE_SECONDS)];
    for second in STRUCK_SECONDS {
        for at in 0..SOUNDING {
            pcm[frames(second) + at] = (STRIKE_PEAK * (TAU * STRIKE_HZ * at as f64 / SAMPLE_RATE).sin()).round() as i16;
        }
    }
    Wave::sound(String::from(NAME), SAMPLE_RATE, 1, pcm)
}

fn strike(beat: i32, beats: i32, head: f64) -> Clip {
    let mut clip = Clip::audio(beat * TICKS_PER_BEAT, beats * TICKS_PER_BEAT, String::from(NAME), String::from(NAME));
    if let Some(file) = &mut clip.file {
        file.head = head;
    }
    clip
}

fn struck() -> App {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0] = Track { name: String::from("AUDIO 1"), ..Track::audio() };
    project.tracks[0].clips = vec![strike(0, 1, STRUCK_SECONDS[0]), strike(1, 1, STRUCK_SECONDS[1]), strike(2, 6, STRUCK_SECONDS[0])];
    project.tracks.push(Track { name: String::from("MIDI 2"), ..Track::blank() });
    project.tracks[1].clips.push(Clip::chord(0, String::new(), &[60, 64, 67]));
    let mut document = project.to_json();
    if let Json::Object(fields) = &mut document {
        fields.push((String::from("waves"), Json::Array(vec![strikes().to_json()])));
    }
    let mut app = App::new();
    app.draw(WIDTH, HEIGHT);
    let opened = app.opened(String::from(SONG), Some(&document.write()), None);
    assert_eq!(opened, Opened::Project, "wanted the song of three strikes to open, got {opened:?}");
    app.take_requests();
    app
}

fn ducking(source: usize, shaped: bool) -> Row {
    let mut slot = Slot::new(Kind::Sidechain);
    slot.wire(Some(source), false);
    if let (Params::Sidechain(sidechain), true) = (&mut slot.params, shaped) {
        sidechain.threshold_db = DUCK_FROM_DB;
        sidechain.ratio = DUCK_RATIO;
    }
    Row::Effect(slot)
}

fn keyed() -> App {
    let mut project = Project::new();
    project.tracks.truncate(1);
    for at in 1..5 {
        project.add_midi_track(at);
    }
    let duck = |source: usize, shaped: bool, wet: f64, bypass: bool| {
        let mut row = ducking(source, shaped);
        row.set_mix(wet);
        row.set_bypass(bypass);
        vec![midi(Arp::DEFAULT), core(LEVEL), row]
    };
    project.tracks[2].rack = duck(SOURCE, true, 1.0, false);
    project.tracks[3].rack = duck(SOURCE, false, 0.5, true);
    project.tracks[4].rack = duck(OTHER_SOURCE, false, 1.0, false);
    project.master.rack = vec![ducking(SOURCE, false), effect(Kind::Delay)];
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false}}", project.to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.take_requests();
    app
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

fn read(app: &mut App, path: &str) -> Json {
    let reply = call(app, "read", &format!(r#"{{"path":"{path}"}}"#));
    assert!(!reply.failed, "wanted {path} read, got {}", reply.text);
    json::parse(&reply.text).unwrap_or_else(|| panic!("wanted {path} read as JSON, got {}", reply.text))
}

fn operation(reply: &str) -> &str {
    reply.lines().skip_while(|line| *line != "Operations:").nth(1).map_or("", str::trim)
}

fn fade(from: f64, to: f64) -> String {
    format!(r#"{{"fromSeconds":{from},"toSeconds":{to}}}"#)
}

fn ends(wave: &Json, key: &str) -> Option<(f64, f64)> {
    let fade = wave.get(key)?;
    Some((fade.get("fromSeconds").and_then(Json::as_f64)?, fade.get("toSeconds").and_then(Json::as_f64)?))
}

fn near(got: Option<(f64, f64)>, wanted: (f64, f64)) -> bool {
    got.is_some_and(|got| (got.0 - wanted.0).abs() < 1e-9 && (got.1 - wanted.1).abs() < 1e-9)
}

fn shape(row: &Json) -> (Option<f64>, Option<f64>, Option<f64>, Option<bool>) {
    (row.get("thresholdDb").and_then(Json::as_f64), row.get("ratio").and_then(Json::as_f64), row.get("wet").and_then(Json::as_f64), row.get("bypass").and_then(Json::as_bool))
}

#[test]
fn match_at_an_audio_clip_gives_its_fades_and_pitch_to_the_clips_that_sound_like_it() {
    let mut app = struck();
    let wave = format!("tracks/0/clips/{LIKE}/wave");
    did(&mut app, &format!(r#"{{"edits":[{{"path":"{wave}/fadeIn","value":{}}},{{"path":"{wave}/fadeOut","value":{}}},{{"path":"{wave}/semitones","value":{SEMITONES}}}]}}"#, fade(RISE.0, RISE.1), fade(FALL.0, FALL.1)));
    let reply = did(&mut app, &format!(r#"{{"path":"tracks/0/clips/{LIKE}","match":true}}"#));
    assert_eq!(operation(&reply), format!("Matched 1 clip of tracks/0 to clips/{LIKE}, its fadeIn, fadeOut, level spans, semitones and cents: clips/{ALIKE}"), "wanted the reply to name the one clip that took them, got {reply}");
    let alike = read(&mut app, &format!("tracks/0/clips/{ALIKE}/wave"));
    assert!(near(ends(&alike, "fadeIn"), (RISE.0 + LATER_BY, RISE.1 + LATER_BY)) && near(ends(&alike, "fadeOut"), (FALL.0 + LATER_BY, FALL.1 + LATER_BY)), "wanted both fades on the clip that sounds alike, each as far into its own strike, got {}", alike.write());
    assert_eq!(alike.get("semitones").and_then(Json::as_f64), Some(SEMITONES), "wanted the pitch given too, got {}", alike.write());
    assert_eq!(alike.get("headSeconds").and_then(Json::as_f64), Some(STRUCK_SECONDS[1]), "wanted the matched clip to keep its own start in the file, got {}", alike.write());
    let long = read(&mut app, &format!("tracks/0/clips/{LONG}/wave"));
    assert!(long.get("fadeIn").is_none() && long.get("fadeOut").is_none() && long.get("semitones").is_none(), "wanted the clip six times as long left alone, got {}", long.write());
}

#[test]
fn match_at_a_clip_with_none_like_it_changes_nothing_and_says_so() {
    let mut app = struck();
    let before = read(&mut app, "tracks/0/clips");
    let reply = did(&mut app, &format!(r#"{{"path":"tracks/0/clips/{LONG}","match":true}}"#));
    assert_eq!(operation(&reply), format!("Matched nothing to tracks/0/clips/{LONG}: no other clip on its track sounds like it"), "wanted the reply to say no other clip sounds like it, got {reply}");
    assert!(read(&mut app, "tracks/0/clips") == before, "wanted every clip left as it was");
}

#[test]
fn match_at_a_sidechain_row_gives_its_settings_to_every_sidechain_keyed_from_the_same_track() {
    let mut app = keyed();
    let reply = did(&mut app, &format!(r#"{{"path":"tracks/2/rack/{DUCK_ROW}","match":true}}"#));
    assert_eq!(operation(&reply), format!("Matched 2 sidechain rows taking the signal from tracks/{SOURCE} to tracks/2/rack/{DUCK_ROW}: tracks/3/rack/{DUCK_ROW}, master/rack/0"), "wanted the reply to name the two rows taking the same signal, got {reply}");
    let other = read(&mut app, &format!("tracks/3/rack/{DUCK_ROW}"));
    assert_eq!(shape(&other), (Some(DUCK_FROM_DB), Some(DUCK_RATIO), Some(0.5), Some(true)), "wanted the threshold and ratio given and the row's own wet and bypass kept, got {}", other.write());
    let master = read(&mut app, "master/rack/0");
    assert_eq!((master.get("thresholdDb").and_then(Json::as_f64), master.get("ratio").and_then(Json::as_f64)), (Some(DUCK_FROM_DB), Some(DUCK_RATIO)), "wanted the master's sidechain keyed from the same track matched too, got {}", master.write());
    let apart = read(&mut app, &format!("tracks/4/rack/{DUCK_ROW}"));
    let plain = bp::fx::params::Sidechain::DEFAULT;
    assert_eq!((apart.get("thresholdDb").and_then(Json::as_f64), apart.get("ratio").and_then(Json::as_f64)), (Some(plain.threshold_db), Some(plain.ratio)), "wanted the sidechain keyed from another track left at its own settings, got {}", apart.write());
    let none = did(&mut app, &format!(r#"{{"path":"tracks/4/rack/{DUCK_ROW}","match":true}}"#));
    assert_eq!(operation(&none), format!("Matched nothing to tracks/4/rack/{DUCK_ROW}: no other sidechain row is taking the signal from tracks/{OTHER_SOURCE}"), "wanted the reply to say no other row takes its signal, got {none}");
}

#[test]
fn match_refuses_a_midi_clip_a_row_that_is_no_sidechain_and_any_other_path() {
    let mut app = struck();
    let notes = refused(&mut app, r#"{"path":"tracks/1/clips/0","match":true}"#);
    assert!(notes.contains("holds notes; match takes an audio clip or a sidechain row"), "wanted a MIDI clip refused, got {notes}");
    let missing = refused(&mut app, r#"{"path":"tracks/0/clips/9","match":true}"#);
    assert!(missing.contains("No clip at tracks/0/clips/9"), "wanted a clip that is not there refused, got {missing}");
    let mut app = keyed();
    let delay = refused(&mut app, r#"{"path":"master/rack/1","match":true}"#);
    assert!(delay.contains("is a delay row; match takes a sidechain row or an audio clip"), "wanted a delay row refused, got {delay}");
    let track = refused(&mut app, r#"{"path":"tracks/2","match":true}"#);
    assert!(track.contains("match takes an audio clip like tracks/N/clips/K or a sidechain row like tracks/N/rack/R"), "wanted a track's own path refused, got {track}");
}
