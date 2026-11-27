use bp::app::{App, Event, Request};
use bp::arp::{Arp, Articulation};
use bp::audio::Tail;
use bp::core::Params;
use bp::draw::{Fill, Rect, Tone};
use bp::json::{self, Json};
use bp::keyboard::Key;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, Track, VELOCITY, beats};
use bp::wav::Wave;
use std::time::{Duration, Instant};
use support::{AUDIBLE, FRAMES_PER_BEAT, HEIGHT, bounce, centre, core_layer, midi, painted, peak_of};

const RAIL: i32 = 45;
const STEP: usize = 2;
const RAIL_W: i32 = 1900;
const TICK: f32 = 1.0 / 60.0;
const PATIENCE: Duration = Duration::from_secs(60);
const LANE_X: i32 = 156;
const BEAT_PX: i32 = 30;
const SECOND_LANE_Y: i32 = 215;
const HELD_BEATS: f32 = 8.0;
const LOOP: (i32, i32) = (2 * TICKS_PER_BEAT, 6 * TICKS_PER_BEAT);
const SWEPT: (i32, i32) = (0, 8 * TICKS_PER_BEAT);
const NAME: &str = "Master Bounce";

fn song() -> Project {
    let mut patch = Params::DEFAULT;
    patch.amp.sustain = 1.0;
    let note = Note { pitch: 60, start: 0.0, length: HELD_BEATS, velocity: VELOCITY, articulation: Articulation::PLAIN };
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(0.8, patch))];
    project.tracks[0].clips = vec![Clip { start: 0, length: HELD_BEATS as i32 * TICKS_PER_BEAT, label: String::new(), notes: vec![note], file: None }];
    assert!(project.add_track_outside(1, Track::blank()), "wanted an empty second track");
    project
}

fn open(project: &Project, range: Option<(i32, i32)>) -> App {
    let range = range.map_or_else(String::new, |(start, end)| format!(",\"loopStartBeat\":{},\"loopEndBeat\":{}", beats(start), beats(end)));
    let mut app = App::new();
    app.resume(&format!("{{\"project\":{}{range}}}", project.to_json().write()));
    painted(&mut app);
    app.take_requests();
    app
}

fn rail_wash(fills: &[Fill]) -> Option<Rect> {
    fills.iter().find(|fill| fill.tone == Tone::Ink25 && fill.rect.y < RAIL && fill.rect.h >= 30).map(|fill| fill.rect)
}

fn buttons(app: &mut App) -> Vec<Rect> {
    let mut found: Vec<Rect> = Vec::new();
    for x in (0..RAIL_W).step_by(STEP) {
        app.event(Event::Move { x, y: RAIL / 2 });
        if let Some(rect) = rail_wash(app.draw(RAIL_W, HEIGHT))
            && !found.last().is_some_and(|last| same(*last, rect))
        {
            found.push(rect);
        }
    }
    found
}

fn same(a: Rect, b: Rect) -> bool {
    (a.x, a.y, a.w, a.h) == (b.x, b.y, b.w, b.h)
}

fn bounce_cell(app: &mut App, usual: &[Rect]) -> Rect {
    let extra: Vec<Rect> = buttons(app).into_iter().filter(|rect| !usual.iter().any(|known| same(*known, *rect))).collect();
    assert!(!extra.is_empty() && extra.len() <= 2, "wanted the BOUNCE cell, with FLATTEN moved beside it at most, beyond the usual buttons, found {}", extra.len());
    extra.into_iter().min_by_key(|rect| rect.x).expect("the leftmost extra cell")
}

fn press(app: &mut App, rect: Rect) {
    let (x, y) = centre(rect);
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
}

fn sweep(app: &mut App, from_beat: f64, to_beat: f64, y: i32) {
    let (from, to) = (LANE_X + (from_beat * f64::from(BEAT_PX)).round() as i32, LANE_X + (to_beat * f64::from(BEAT_PX)).round() as i32);
    app.event(Event::Down { x: from, y, clicks: 1 });
    app.event(Event::Move { x: to, y });
    app.event(Event::Up { x: to, y });
}

fn project_in(text: &str) -> Project {
    Project::from_json(json::parse(text).expect("wanted the remembered session to parse").get("project").expect("wanted the project in the session")).expect("wanted the remembered project to read")
}

fn landed(app: &mut App, before: usize) -> Project {
    let started = Instant::now();
    loop {
        app.tick(TICK);
        let requests = app.take_requests();
        assert!(!requests.iter().any(|request| matches!(request, Request::Export { .. })), "wanted no save dialog asked for by a bounce");
        let remembered = requests.into_iter().filter_map(|request| if let Request::Remember { json } = request { Some(json) } else { None }).next_back();
        if let Some(project) = remembered.map(|text| project_in(&text)).filter(|project| project.tracks.len() > before) {
            return project;
        }
        assert!(started.elapsed() < PATIENCE, "wanted the bounce landed within {PATIENCE:?}");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn saved(app: &mut App) -> Json {
    app.event(Event::Key { key: Key::Char('s'), shift: false, command: true, repeat: false });
    let text = app
        .take_requests()
        .into_iter()
        .find_map(|request| match request {
            Request::Write { json, .. } | Request::SaveAs { json, .. } => Some(json),
            _ => None,
        })
        .expect("wanted a save payload after the press");
    json::parse(&text).expect("wanted the save payload to parse")
}

fn pooled(file: &Json, name: &str) -> Wave {
    let entry = file.get("waves").and_then(Json::as_array).expect("wanted a wave pool in the file").iter().find(|entry| entry.get("name").and_then(Json::as_str) == Some(name)).unwrap_or_else(|| panic!("wanted {name} in the pool"));
    Wave::from_json(entry).unwrap_or_else(|| panic!("wanted {name} to read from the pool"))
}

fn exported(project: &Project, span: (i32, i32)) -> Wave {
    let bytes = bounce(project, &[], span, Tail::Cut).expect("wanted the span exported");
    Wave::parse(String::from("export"), String::new(), &bytes).expect("wanted the export to parse as a wave")
}

fn only_clip(track: &Track) -> &Clip {
    assert_eq!(track.clips.len(), 1, "wanted one clip on the bounced track, got {}", track.clips.len());
    &track.clips[0]
}

#[test]
fn the_bounce_cell_shows_only_while_a_loop_or_a_time_selection_is_set() {
    let song = song();
    let usual = buttons(&mut open(&song, None));
    let mut looped = open(&song, Some(LOOP));
    assert_eq!(buttons(&mut looped).len(), usual.len() + 1, "wanted the BOUNCE cell with a loop set beside the usual {} buttons", usual.len());
    let mut swept = open(&song, None);
    sweep(&mut swept, 0.03, 7.5, SECOND_LANE_Y);
    assert_eq!(buttons(&mut swept).len(), usual.len() + 1, "wanted the BOUNCE cell with a time selection drawn");
    swept.event(Event::Key { key: Key::Escape, shift: false, command: false, repeat: false });
    assert_eq!(buttons(&mut swept).len(), usual.len(), "wanted the BOUNCE cell gone once Escape dropped the selection");
}

#[test]
fn pressing_bounce_lands_the_master_over_the_loop_as_an_audio_track_at_the_bottom() {
    let song = song();
    let usual = buttons(&mut open(&song, None));
    let mut app = open(&song, Some(LOOP));
    let cell = bounce_cell(&mut app, &usual);
    press(&mut app, cell);
    let project = landed(&mut app, song.tracks.len());
    assert_eq!(project.tracks.len(), song.tracks.len() + 1, "wanted one new track, got {}", project.tracks.len());
    let track = project.tracks.last().expect("the bounced track");
    assert!(track.audio && track.name == NAME, "wanted an audio track named {NAME} at the bottom, got {} {}", track.kind(), track.name);
    let clip = only_clip(track);
    assert_eq!((clip.start, clip.length), (LOOP.0, LOOP.1 - LOOP.0), "wanted the clip over the loop, got start {} length {}", clip.start, clip.length);
    assert_eq!(clip.file.as_ref().map(|file| file.name.as_str()), Some(NAME), "wanted the clip to name the pooled wave");
    let wave = pooled(&saved(&mut app), NAME);
    let export = exported(&song, LOOP);
    assert_eq!((wave.channels, wave.rate, wave.path.as_deref()), (2, 48_000.0, None), "wanted a stereo 48 kHz wave with no file behind it");
    assert_eq!(wave.frames(), (FRAMES_PER_BEAT * beats(LOOP.1 - LOOP.0)).round() as usize, "wanted the wave as long as the loop");
    assert!(wave.pcm == export.pcm, "wanted the bounce to carry exactly what Export Audio renders over the loop");
}

#[test]
fn a_time_selection_over_an_empty_lane_bounces_the_whole_master_across_it() {
    let song = song();
    let usual = buttons(&mut open(&song, None));
    let mut app = open(&song, None);
    sweep(&mut app, 0.03, 7.5, SECOND_LANE_Y);
    let cell = bounce_cell(&mut app, &usual);
    press(&mut app, cell);
    let project = landed(&mut app, song.tracks.len());
    let track = project.tracks.last().expect("the bounced track");
    let clip = only_clip(track);
    assert_eq!((clip.start, clip.length), (SWEPT.0, SWEPT.1 - SWEPT.0), "wanted the clip over the swept beats, got start {} length {}", clip.start, clip.length);
    let wave = pooled(&saved(&mut app), NAME);
    let samples: Vec<f32> = wave.pcm.iter().map(|sample| f32::from(*sample) / f32::from(i16::MAX)).collect();
    assert!(peak_of(&samples) > AUDIBLE, "wanted the master's sound in the bounce though the swept lane is empty");
    assert!(wave.pcm == exported(&song, SWEPT).pcm, "wanted the bounce to match the export of the swept beats");
}
