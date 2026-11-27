use bp::app::{App, Opened, Request};
use bp::json::{self, Json};
use bp::mcp::{Call, Reply};
use bp::project::{Clip, Project, TICKS_PER_BEAT, Track};
use bp::wav::Wave;
use std::f64::consts::TAU;
use std::thread::JoinHandle;
use std::time::Duration;
use support::{HEIGHT, SAMPLE_RATE, WIDTH, frames, wav};

const SONG: &str = "/heard/songs/song.json";
const NEXT: &str = "/heard/songs/next.json";
const TAKE: &str = "/heard/takes/kick.wav";
const NAME: &str = "kick";
const LOW_HZ: f64 = 300.0;
const HIGH_HZ: f64 = 1_200.0;
const TONE_SECONDS: f64 = 0.25;
const TONE_PEAK: f64 = 8_192.0;
const PATIENCE: Duration = Duration::from_secs(120);
const TICK: f32 = 0.01;
const PAUSE: Duration = Duration::from_millis(2);
const FIRST_BEAT: &str = r#"{"fromBeat":0,"untilBeat":1}"#;
const REVISION: &str = "\"revision\"";

fn launched() -> App {
    let mut app = App::new();
    app.draw(WIDTH, HEIGHT);
    app.take_requests();
    app
}

fn tone(hz: f64) -> Vec<i16> {
    (0..frames(TONE_SECONDS)).map(|frame| (TONE_PEAK * (TAU * hz * frame as f64 / SAMPLE_RATE).sin()).round() as i16).collect()
}

fn song(wave: Json) -> String {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0] = Track::audio();
    project.tracks[0].clips.push(Clip::audio(0, TICKS_PER_BEAT, String::from(NAME), String::from(NAME)));
    let mut document = project.to_json();
    if let Json::Object(fields) = &mut document {
        fields.push((String::from("waves"), Json::Array(vec![wave])));
    }
    document.write()
}

fn embedded(hz: f64) -> Json {
    Wave::sound(String::from(NAME), SAMPLE_RATE, 1, tone(hz)).to_json()
}

fn linked() -> Json {
    Json::Object(vec![(String::from("name"), Json::String(String::from(NAME))), (String::from("sourcePath"), Json::String(String::from(TAKE)))])
}

fn open(app: &mut App, path: &str, text: &str) {
    let opened = app.opened(String::from(path), Some(text), None);
    assert_eq!(opened, Opened::Project, "wanted {path} to open as a song, got {opened:?}");
}

fn asked(app: &mut App) -> JoinHandle<Reply> {
    let (listen, waiting) = Call::new("listen", json::parse(FIRST_BEAT).expect("wanted the arguments to parse"));
    app.deliver(listen);
    std::thread::spawn(move || waiting.reply(PATIENCE))
}

fn answered(app: &mut App, answer: JoinHandle<Reply>) -> Reply {
    while !answer.is_finished() {
        app.tick(TICK);
        std::thread::sleep(PAUSE);
    }
    answer.join().expect("wanted the listen answered")
}

fn figures(reply: &Reply) -> String {
    reply.text.lines().filter(|line| !line.trim_start().starts_with(REVISION)).collect::<Vec<_>>().join("\n")
}

fn listened(app: &mut App) -> String {
    let asked = asked(app);
    let reply = answered(app, asked);
    assert!(!reply.failed, "wanted the listen answered, got {}", reply.text);
    figures(&reply)
}

fn heard_alone(song: &str) -> String {
    let mut app = launched();
    open(&mut app, NEXT, song);
    listened(&mut app)
}

#[test]
fn a_listen_still_rendering_when_the_song_is_replaced_is_refused_and_the_next_song_hears_itself() {
    let (low, high) = (song(embedded(LOW_HZ)), song(embedded(HIGH_HZ)));
    let (of_low, of_high) = (heard_alone(&low), heard_alone(&high));
    assert_ne!(of_low, of_high, "wanted the low and the high kick to measure apart");
    let mut app = launched();
    open(&mut app, SONG, &low);
    let first = asked(&mut app);
    open(&mut app, NEXT, &high);
    let first = answered(&mut app, first);
    let second = listened(&mut app);
    assert_ne!(second, of_low, "wanted the second song's listen to hear its own high kick, got the first song's figures");
    assert_eq!(second, of_high, "wanted the second song's listen to tell what that song tells heard alone");
    assert!(first.failed, "wanted the listen still rendering when the song was replaced refused, got {}", first.text);
}

#[test]
fn a_listen_after_a_wave_arrives_hears_it() {
    let mut app = launched();
    open(&mut app, SONG, &song(linked()));
    let reads: Vec<Request> = app.take_requests().into_iter().filter(|request| matches!(request, Request::Read { .. })).collect();
    assert_eq!(reads, vec![Request::Read { path: String::from(TAKE) }], "wanted the kick's read still waiting at the first listen, got {reads:?}");
    let silent = listened(&mut app);
    app.read(String::from(TAKE), Some(wav(&tone(LOW_HZ))));
    let sounding = listened(&mut app);
    assert_ne!(sounding, silent, "wanted the listen after the kick loaded to hear it, got the answer kept from before it loaded");
    assert_eq!(sounding, heard_alone(&song(embedded(LOW_HZ))), "wanted the listen after the kick loaded to tell what the song tells with its kick embedded");
}
