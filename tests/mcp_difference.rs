use bp::app::{App, Opened, Request};
use bp::json::{self, Json};
use bp::mcp::{Call, Reply};
use bp::project::{Clip, DEFAULT_BPM, Project, TICKS_PER_BEAT, Track};
use bp::wav::Wave;
use std::f64::consts::TAU;
use std::time::Duration;
use support::{HEIGHT, SAMPLE_RATE, WIDTH, call};

const SONG: &str = "/changed/songs/song.json";
const NAME: &str = "strikes";
const BEATS: i32 = 4;
const FRAMES_PER_BEAT: usize = (SAMPLE_RATE * 60.0 / DEFAULT_BPM) as usize;
const KEPT_BEAT: usize = 1;
const DROPPED_BEAT: usize = 3;
const CUT_BEAT: usize = 2;
const SOUNDING: usize = 4_800;
const STRIKE_HZ: f64 = 1_000.0;
const STRIKE_PEAK: f64 = 16_384.0;
const FULL_SCALE: f64 = 32_768.0;
const HEADER: usize = 68;
const BYTES_PER_FRAME: usize = 6;
const SCALE: f64 = 8_388_608.0;
const ALIKE: f64 = 1e-4;
const PATIENCE: Duration = Duration::from_secs(120);
const TICK: f32 = 0.01;
const PAUSE: Duration = Duration::from_millis(2);
const WHOLE: &str = "{}";
const DROP_THE_SECOND_HALF: &str = r#"{"edits":[{"path":"tracks/0","cut":2},{"path":"tracks/0/clips/1","value":null}]}"#;

fn strikes() -> Vec<i16> {
    let mut pcm = vec![0i16; BEATS as usize * FRAMES_PER_BEAT];
    for beat in [KEPT_BEAT, DROPPED_BEAT] {
        for at in 0..SOUNDING {
            pcm[beat * FRAMES_PER_BEAT + at] = (STRIKE_PEAK * (TAU * STRIKE_HZ * at as f64 / SAMPLE_RATE).sin()).round() as i16;
        }
    }
    pcm
}

fn song() -> String {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0] = Track::audio();
    project.tracks[0].clips.push(Clip::audio(0, BEATS * TICKS_PER_BEAT, String::from(NAME), String::from(NAME)));
    let mut document = project.to_json();
    if let Json::Object(fields) = &mut document {
        fields.push((String::from("waves"), Json::Array(vec![Wave::sound(String::from(NAME), SAMPLE_RATE, 1, strikes()).to_json()])));
    }
    document.write()
}

fn folder(name: &str) -> String {
    let root = std::env::temp_dir().join(format!("bp-difference-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&root).expect("wanted a folder for the difference to land in");
    root.to_string_lossy().into_owned()
}

fn struck(root: &str) -> App {
    let mut app = App::new();
    app.draw(WIDTH, HEIGHT);
    app.add_folder(String::from(root));
    let opened = app.opened(String::from(SONG), Some(&song()), None);
    assert_eq!(opened, Opened::Project, "wanted the song of two strikes to open, got {opened:?}");
    app.take_requests();
    app
}

fn listened(app: &mut App, args: &str) -> (Reply, Vec<(String, Vec<u8>)>) {
    let (listen, waiting) = Call::new("listen", json::parse(args).unwrap_or_else(|| panic!("wanted {args} to parse")));
    app.deliver(listen);
    let answer = std::thread::spawn(move || waiting.reply(PATIENCE));
    let mut files = Vec::new();
    while !answer.is_finished() {
        app.tick(TICK);
        for request in app.take_requests() {
            if let Request::Exported { path, bytes } = request {
                app.exported(path.clone(), true);
                files.push((path, bytes));
            }
        }
        std::thread::sleep(PAUSE);
    }
    (answer.join().expect("wanted the listen answered"), files)
}

fn heard(app: &mut App, args: &str) -> (Json, Vec<(String, Vec<u8>)>) {
    let (reply, files) = listened(app, args);
    assert!(!reply.failed, "wanted listen {args} answered, got {}", reply.text);
    (json::parse(&reply.text).unwrap_or_else(|| panic!("wanted listen to answer JSON, got {}", reply.text)), files)
}

fn left(wav: &[u8]) -> Vec<f64> {
    wav[HEADER..].as_chunks::<BYTES_PER_FRAME>().0.iter().map(|frame| f64::from(i32::from_le_bytes([0, frame[0], frame[1], frame[2]]) >> 8) / SCALE).collect()
}

#[test]
fn difference_keeps_this_render_less_the_revisions_as_a_wave_holding_what_was_taken_out() {
    let root = folder("kept");
    let mut app = struck(&root);
    let (before, none) = heard(&mut app, WHOLE);
    assert!(none.is_empty(), "wanted a plain listen to keep no file");
    let revision = before.get("revision").and_then(Json::as_f64).expect("wanted the first listen to tell its revision");
    let cut = call(&mut app, "write", DROP_THE_SECOND_HALF);
    assert!(!cut.failed, "wanted the second half cut away, got {}", cut.text);
    let path = format!("{root}/removed.wav");
    let (after, files) = heard(&mut app, &format!("{{\"fromBeat\":0,\"untilBeat\":{BEATS},\"against\":{revision},\"difference\":\"{path}\"}}"));
    let _ = std::fs::remove_dir_all(&root);
    let null = after.get("null").unwrap_or_else(|| panic!("wanted the listen to tell its null, got {}", after.write()));
    assert_eq!(null.get("wav").and_then(Json::as_str), Some(path.as_str()), "wanted the kept difference named under null, got {}", null.write());
    assert_eq!(after.get("wav"), None, "wanted no render kept beside the difference, got {}", after.write());
    let [(landed, bytes)] = files.as_slice() else {
        panic!("wanted one file handed to the shell, got {}", files.len());
    };
    assert_eq!(landed, &path, "wanted the difference handed over for the path asked");
    let (kept, source) = (left(bytes), strikes());
    assert_eq!(kept.len(), source.len(), "wanted the difference as long as the render");
    let apart = |frame: usize, wanted: f64| (kept[frame] - wanted).abs() > ALIKE;
    let moved = (0..CUT_BEAT * FRAMES_PER_BEAT).find(|&frame| apart(frame, 0.0));
    assert_eq!(moved, None, "wanted silence before the cut at beat {CUT_BEAT}, where nothing changed");
    let lost = (CUT_BEAT * FRAMES_PER_BEAT..source.len()).find(|&frame| apart(frame, -f64::from(source[frame]) / FULL_SCALE));
    assert_eq!(lost, None, "wanted the dropped strike after the cut, this render less the one before, so upside down");
}

#[test]
fn difference_needs_a_revision_to_take_away_and_a_listen_keeps_one_file() {
    let root = folder("refused");
    let mut app = struck(&root);
    let (alone, _) = listened(&mut app, &format!("{{\"difference\":\"{root}/removed.wav\"}}"));
    let (before, _) = heard(&mut app, WHOLE);
    let revision = before.get("revision").and_then(Json::as_f64).expect("wanted the listen to tell its revision");
    let (both, files) = listened(&mut app, &format!("{{\"against\":{revision},\"path\":\"{root}/render.wav\",\"difference\":\"{root}/removed.wav\"}}"));
    let _ = std::fs::remove_dir_all(&root);
    assert!(alone.failed && alone.text.contains("pass against"), "wanted difference with no revision refused, asking for against, got {}", alone.text);
    assert!(both.failed && both.text.contains("pass one"), "wanted path and difference together refused, got {}", both.text);
    assert!(files.is_empty(), "wanted the refused listen to keep no file");
}
