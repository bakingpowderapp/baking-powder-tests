use bp::app::{App, Event, Request};
use bp::arp::{Arp, Articulation};
use bp::audio::Tail;
use bp::core::Params;
use bp::draw::{Fill, Rect, Tone};
use bp::json::{self, Json};
use bp::keyboard::Key;
use bp::project::{Clip, Note, Project, Return, Row, SEND_COUNT, TICKS_PER_BEAT, Track, VELOCITY};
use bp::wav::Wave;
use std::time::{Duration, Instant};
use support::{FRAMES_PER_BEAT, WIDTH, bounce, centre, core_layer, midi};

const TALL: i32 = 1400;
const RAIL: i32 = 45;
const STEP: usize = 2;
const RAIL_W: i32 = 1900;
const TICK: f32 = 1.0 / 60.0;
const PATIENCE: Duration = Duration::from_secs(120);
const TILE_PITCH: i32 = 112;
const TILE_X: i32 = 60;
const TILE_DY: i32 = 40;
const QUIET: usize = 0;
const LEAD: usize = 1;
const BAND: usize = 2;
const KICK: usize = 3;
const SNARE: usize = 4;
const LEAD_GAIN: f32 = 0.5;
const LEAD_SEND: f32 = 0.3;
const SPAN: (i32, i32) = (0, 4 * TICKS_PER_BEAT);

fn note(start: f32, length: f32) -> Note {
    Note { pitch: 60, start, length, velocity: VELOCITY, articulation: Articulation::PLAIN }
}

fn sounding(name: &str, notes: Vec<Note>) -> Track {
    let mut patch = Params::DEFAULT;
    patch.amp.sustain = 1.0;
    let end = notes.iter().map(|note| note.start + note.length).fold(0.0, f32::max);
    let clip = Clip { start: 0, length: end.ceil() as i32 * TICKS_PER_BEAT, label: String::new(), notes, file: None };
    Track { name: String::from(name), rack: vec![midi(Arp::DEFAULT), Row::Core(core_layer(0.8, patch))], clips: vec![clip], ..Track::blank() }
}

fn song() -> Project {
    let mut project = Project::new();
    project.tracks.clear();
    project.add_track(QUIET, Track { name: String::from("quiet"), ..Track::blank() });
    let mut sends = [0.0; SEND_COUNT];
    sends[0] = LEAD_SEND;
    project.add_track(LEAD, Track { gain: LEAD_GAIN, sends, ..sounding("lead", vec![note(0.0, 4.0)]) });
    project.add_track(BAND, Track { name: String::from("band"), ..Track::group() });
    project.add_track(KICK, sounding("kick", vec![note(0.0, 2.0)]));
    project.add_track(SNARE, sounding("snare", vec![note(2.0, 2.0)]));
    project
}

fn open(project: &Project) -> App {
    let mut app = App::new();
    app.resume(&format!("{{\"project\":{}}}", project.to_json().write()));
    painted(&mut app);
    app.take_requests();
    app
}

fn painted(app: &mut App) -> Vec<Fill> {
    app.draw(WIDTH, TALL).to_vec()
}

fn same(a: Rect, b: Rect) -> bool {
    (a.x, a.y, a.w, a.h) == (b.x, b.y, b.w, b.h)
}

fn rail_wash(fills: &[Fill]) -> Option<Rect> {
    fills.iter().find(|fill| fill.tone == Tone::Ink25 && fill.rect.y < RAIL && fill.rect.h >= 30).map(|fill| fill.rect)
}

fn buttons(app: &mut App) -> Vec<Rect> {
    let mut found: Vec<Rect> = Vec::new();
    for x in (0..RAIL_W).step_by(STEP) {
        app.event(Event::Move { x, y: RAIL / 2 });
        if let Some(rect) = rail_wash(app.draw(RAIL_W, TALL))
            && !found.last().is_some_and(|last| same(*last, rect))
        {
            found.push(rect);
        }
    }
    found
}

fn flatten_cell(app: &mut App, usual: &[Rect]) -> Rect {
    let extra: Vec<Rect> = buttons(app).into_iter().filter(|rect| !usual.iter().any(|known| same(*known, *rect))).collect();
    assert!(!extra.is_empty() && extra.len() <= 2, "wanted the FLATTEN cell, with GROUP's beside it at most, beyond the usual buttons, found {}", extra.len());
    extra.into_iter().max_by_key(|rect| rect.x).expect("the rightmost extra cell")
}

fn press(app: &mut App, rect: Rect) {
    let (x, y) = centre(rect);
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
}

fn tile_point(slot: usize) -> (i32, i32) {
    (TILE_X, RAIL + slot as i32 * TILE_PITCH + TILE_DY)
}

fn select_track(app: &mut App, slot: usize) {
    let (x, y) = tile_point(slot);
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
}

fn pick_track(app: &mut App, slot: usize) {
    let (x, y) = tile_point(slot);
    app.event(Event::Modifiers { shift: true, command: false });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    app.event(Event::Modifiers { shift: false, command: false });
}

fn project_in(text: &str) -> Project {
    Project::from_json(json::parse(text).expect("wanted the remembered session to parse").get("project").expect("wanted the project in the session")).expect("wanted the remembered project to read")
}

fn landed(app: &mut App, done: impl Fn(&Project) -> bool) -> Project {
    let started = Instant::now();
    loop {
        app.tick(TICK);
        let requests = app.take_requests();
        assert!(!requests.iter().any(|request| matches!(request, Request::Export { .. })), "wanted no save dialog asked for by a flatten");
        let remembered = requests.into_iter().filter_map(|request| if let Request::Remember { json } = request { Some(json) } else { None }).next_back();
        if let Some(project) = remembered.map(|text| project_in(&text)).filter(|project| done(project)) {
            return project;
        }
        assert!(started.elapsed() < PATIENCE, "wanted the flatten landed within {PATIENCE:?}");
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

fn alone(song: &Project, head: usize, whole: &[usize], span: (i32, i32)) -> Wave {
    let mut project = song.clone();
    project.master = Return::DEFAULT;
    for (index, track) in project.tracks.iter_mut().enumerate() {
        track.solo = whole.contains(&index);
        track.sends = [0.0; SEND_COUNT];
        if track.solo {
            track.mute = false;
        }
    }
    project.tracks[head].gain = 1.0;
    project.tracks[head].delay_ms = 0.0;
    let bytes = bounce(&project, &[], span, Tail::Rings).expect("wanted the track rendered alone");
    Wave::parse(String::from("alone"), String::new(), &bytes).expect("wanted the render to parse as a wave")
}

fn span_frames(span: (i32, i32)) -> usize {
    (FRAMES_PER_BEAT * f64::from(span.1 - span.0) / f64::from(TICKS_PER_BEAT)).round() as usize
}

fn only_clip(track: &Track) -> &Clip {
    assert_eq!(track.clips.len(), 1, "wanted one clip on {}, got {}", track.name, track.clips.len());
    &track.clips[0]
}

fn assert_flattened(track: &Track, name: &str, span: (i32, i32)) {
    assert!(track.audio && !track.group && track.rack.is_empty() && track.name == name, "wanted {name} as an audio track with no rack, got {} {} with {} rows", track.kind(), track.name, track.rack.len());
    let clip = only_clip(track);
    assert!(clip.start == span.0 && clip.length >= span.1 - span.0, "wanted the clip from the first note over at least the played beats, got start {} length {}", clip.start, clip.length);
    assert_eq!(clip.file.as_ref().map(|file| file.name.as_str()), Some(name), "wanted the clip to name the pooled wave");
}

fn assert_same_sound(wave: &Wave, expected: &Wave, span: (i32, i32)) {
    let samples = span_frames(span) * 2;
    assert!(wave.frames() >= span_frames(span) && expected.frames() >= span_frames(span), "wanted both waves at least as long as the played beats, got {} and {} frames", wave.frames(), expected.frames());
    assert!(wave.pcm[..samples] == expected.pcm[..samples], "wanted the flattened wave to carry the track's own render over the played beats");
}

#[test]
fn flatten_shows_for_a_picked_track_with_clips_and_turns_it_into_audio_keeping_its_mix() {
    let song = song();
    let mut app = open(&song);
    let usual = buttons(&mut app);
    select_track(&mut app, LEAD);
    let cell = flatten_cell(&mut app, &usual);
    press(&mut app, cell);
    let project = landed(&mut app, |project| project.tracks[LEAD].audio);
    assert_eq!(project.tracks.len(), song.tracks.len(), "wanted the track replaced in place, got {} tracks", project.tracks.len());
    let track = &project.tracks[LEAD];
    assert_flattened(track, "lead", SPAN);
    assert_eq!((track.gain, track.sends[0]), (LEAD_GAIN, LEAD_SEND), "wanted the gain and send kept on the audio track");
    assert_same_sound(&pooled(&saved(&mut app), "lead"), &alone(&song, LEAD, &[LEAD], SPAN), SPAN);
}

#[test]
fn two_picked_tracks_flatten_in_one_press() {
    let song = song();
    let mut app = open(&song);
    let usual = buttons(&mut app);
    select_track(&mut app, LEAD);
    pick_track(&mut app, KICK);
    let cell = flatten_cell(&mut app, &usual);
    press(&mut app, cell);
    let project = landed(&mut app, |project| project.tracks[LEAD].audio && project.tracks[KICK].audio);
    assert_eq!(project.tracks.len(), song.tracks.len(), "wanted both replaced in place, got {} tracks", project.tracks.len());
    assert_flattened(&project.tracks[LEAD], "lead", SPAN);
    assert!(project.tracks[KICK].audio && project.tracks[KICK].inside && project.tracks[BAND].group, "wanted the picked member flattened inside its group");
    assert!(!project.tracks[SNARE].audio, "wanted the unpicked member left alone");
}
