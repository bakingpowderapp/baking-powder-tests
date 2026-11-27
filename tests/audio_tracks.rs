use bp::app::{App, Event, Request};
use bp::keyboard::Key;
use bp::browser::{Found, Item};
use bp::draw::{Fill, Rect, Tone};
use bp::face::{CELL, PITCH};
use bp::grid::{BAR_W, BEAT_W};
use bp::json::{self, Json};
use bp::project::{BEATS_PER_BAR, Clip, DEFAULT_BPM, Project, TICKS_PER_BEAT, Track};
use support::{RATE, WIDTH, browser_rect, clip_block, painted};

const LINE: i32 = 2;
const LANE_LEAST_H: i32 = 40;
const LINES_PER_LANE: usize = 8;
const KICK: &str = "/x/kick.wav";
const ROOT: &str = "/x/Samples";
const KICK_SECONDS: f64 = 0.75;
const KICK_TICKS: i32 = (KICK_SECONDS * DEFAULT_BPM / 60.0 * TICKS_PER_BEAT as f64) as i32;
fn kick() -> Vec<u8> {
    let samples = (KICK_SECONDS * f64::from(RATE)) as usize;
    let data: Vec<u8> = (0..samples).flat_map(|_| 16_384i16.to_le_bytes()).collect();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&((36 + data.len()) as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&RATE.to_le_bytes());
    bytes.extend_from_slice(&(RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&data);
    bytes
}

fn open(project: &Project, browsing: bool) -> App {
    let folders = if browsing { format!(",\"aside\":\"browser\",\"folders\":[\"{ROOT}\"],\"unfolded\":[\"{ROOT}\"]") } else { String::new() };
    let origins: Vec<String> = project.tracks.iter().flat_map(|track| &track.clips).filter_map(|clip| clip.file.as_ref()).map(|file| format!("{{\"name\":\"{}\",\"from\":\"{KICK}\"}}", file.name)).collect();
    let origins = if origins.is_empty() { String::new() } else { format!(",\"origins\":[{}]", origins.join(",")) };
    let session = format!("{{\"project\":{},\"keyboard\":false{folders}{origins}}}", project.to_json().write());
    let mut app = App::new();
    app.resume(&session);
    app
}

fn serve(app: &mut App) -> Option<Project> {
    let mut remembered = None;
    loop {
        let requests = app.take_requests();
        if requests.is_empty() {
            break;
        }
        for request in requests {
            match request {
                Request::Read { path } => app.read(path, Some(kick())),
                Request::Remember { json } => remembered = Some(json),
                _ => {}
            }
        }
    }
    let session = json::parse(&remembered?).expect("wanted the remembered session to parse");
    Project::from_json(session.get("project").expect("wanted the remembered session to hold the project"))
}

fn lane_rows(fills: &[Fill]) -> Vec<Rect> {
    let mut lines: Vec<Rect> = fills.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.w == LINE && fill.rect.h >= LANE_LEAST_H).map(|fill| fill.rect).collect();
    lines.sort_by_key(|line| (line.y, line.h, line.x));
    let mut rows: Vec<Rect> = Vec::new();
    for line in lines {
        let alike = |row: &Rect| row.y == line.y && row.h == line.h;
        match rows.iter_mut().find(|row| alike(row)) {
            Some(row) if line.x > row.x + row.w => row.w = line.x - row.x,
            Some(_) => {}
            None => rows.push(Rect::new(line.x, line.y, LINE, line.h)),
        }
    }
    rows.retain(|row| row.w >= LINES_PER_LANE as i32 * BAR_W);
    rows
}

fn column(rows: &[Rect]) -> i32 {
    rows.iter().map(|row| row.x).min().expect("wanted a lane row") - BAR_W
}

fn beat_x(rows: &[Rect], beat: i32) -> i32 {
    column(rows) + (f64::from(beat) * BEAT_W) as i32
}

fn middle(row: Rect) -> i32 {
    row.y + row.h / 2
}

fn lattice_rows(fills: &[Fill]) -> usize {
    fills.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.h <= LINE && fill.rect.w >= WIDTH * 3 / 4).count()
}

fn click(app: &mut App, x: i32, y: i32, clicks: i32) {
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks });
    app.event(Event::Up { x, y });
}

fn key(app: &mut App, key: Key, shift: bool) {
    app.event(Event::Key { key, shift, command: false, repeat: false });
    app.event(Event::KeyUp { key });
}

fn play_from(app: &mut App) -> i32 {
    let json = app.take_requests().into_iter().rev().find_map(|request| if let Request::Remember { json } = request { Some(json) } else { None }).expect("wanted the session remembered after the key");
    json::parse(&json).expect("wanted the remembered session to parse").get("playheadBeat").and_then(Json::as_i64).unwrap_or(0) as i32
}

fn kinds(project: &Project) -> Vec<(&str, bool, usize)> {
    project.tracks.iter().map(|track| (track.name.as_str(), track.audio, track.clips.len())).collect()
}

fn name_cell(browser: Rect, depth: i32, row: i32) -> Rect {
    let mark = Rect::new(browser.x + depth * PITCH, browser.y + row * PITCH, CELL, CELL);
    Rect::new(mark.x + PITCH, mark.y, browser.x + browser.w - mark.x - PITCH, CELL)
}

#[test]
fn a_wave_dropped_on_a_midi_lane_makes_an_audio_track_after_it_at_the_beat_under_the_pointer() {
    let mut app = open(&Project::new(), false);
    let rows = lane_rows(&painted(&mut app));
    assert_eq!(rows.len(), 2, "wanted the two MIDI lanes painted, got {}", rows.len());
    app.dropped(String::from(KICK), bp::browser::Kind::Wave, beat_x(&rows, 8), middle(rows[1]));
    let project = serve(&mut app).expect("wanted the drop remembered");
    assert_eq!(kinds(&project), vec![("MIDI 1", false, 0), ("MIDI 2", false, 0), ("AUDIO 3", true, 1)], "wanted an audio track after MIDI 2 holding the clip, got {:?}", kinds(&project));
    let clip = &project.tracks[2].clips[0];
    assert_eq!((clip.start, clip.length, clip.label.as_str(), clip.file.as_ref().map(|file| (file.name.as_str(), file.offset))), (8 * TICKS_PER_BEAT, KICK_TICKS, "kick", Some(("kick", 0.0))), "wanted the clip at beat 8, {KICK_TICKS} ticks long, named for the file, got {clip:?}");
    app.dropped(String::from(KICK), bp::browser::Kind::Wave, beat_x(&rows, 4), middle(rows[0]));
    let project = serve(&mut app).expect("wanted the second drop remembered");
    assert_eq!(kinds(&project), vec![("MIDI 1", false, 0), ("AUDIO 4", true, 1), ("MIDI 2", false, 0), ("AUDIO 3", true, 1)], "wanted the second wave on a new audio track right after MIDI 1, got {:?}", kinds(&project));
}

#[test]
fn a_wave_dropped_on_an_audio_lane_joins_that_track() {
    let mut project = Project::new();
    project.add_track(2, Track::audio());
    let mut app = open(&project, false);
    let rows = lane_rows(&painted(&mut app));
    assert_eq!(rows.len(), 3, "wanted three lanes painted, got {}", rows.len());
    app.dropped(String::from(KICK), bp::browser::Kind::Wave, beat_x(&rows, 4), middle(rows[2]));
    app.dropped(String::from(KICK), bp::browser::Kind::Wave, beat_x(&rows, 12), middle(rows[2]));
    let project = serve(&mut app).expect("wanted the drops remembered");
    assert_eq!(kinds(&project), vec![("MIDI 1", false, 0), ("MIDI 2", false, 0), ("AUDIO 3", true, 2)], "wanted both waves on the audio track, got {:?}", kinds(&project));
    let starts: Vec<i32> = project.tracks[2].clips.iter().map(|clip| clip.start).collect();
    assert_eq!(starts, vec![4 * TICKS_PER_BEAT, 12 * TICKS_PER_BEAT], "wanted the clips at beats 4 and 12, got {starts:?}");
    let further = serve(&mut app);
    assert!(further.is_none(), "wanted nothing more to remember once the drops were served");
}

#[test]
fn the_arrows_step_the_playhead_by_a_bar_and_by_a_beat_with_shift() {
    let mut app = open(&Project::new(), false);
    app.take_requests();
    key(&mut app, Key::Right, false);
    assert_eq!(play_from(&mut app), BEATS_PER_BAR, "wanted the right arrow to step to the next bar line");
    key(&mut app, Key::Right, false);
    assert_eq!(play_from(&mut app), 2 * BEATS_PER_BAR, "wanted a second press at the second bar line");
    key(&mut app, Key::Right, true);
    assert_eq!(play_from(&mut app), 2 * BEATS_PER_BAR + 1, "wanted shift and the right arrow to step one beat");
    key(&mut app, Key::Left, false);
    assert_eq!(play_from(&mut app), 2 * BEATS_PER_BAR, "wanted the left arrow back to the bar line just behind");
    key(&mut app, Key::Left, false);
    assert_eq!(play_from(&mut app), BEATS_PER_BAR, "wanted the left arrow back one whole bar");
    key(&mut app, Key::Left, false);
    key(&mut app, Key::Left, false);
    assert_eq!(play_from(&mut app), 0, "wanted the playhead held at the song's start");
}

#[test]
fn an_audio_clip_never_opens_the_roll_and_steps_under_the_arrows_as_any_clip_does() {
    let mut project = Project::new();
    project.add_track(2, Track::audio());
    project.tracks[2].clips.push(Clip::audio(0, 8 * TICKS_PER_BEAT, String::from("kick"), String::from("kick")));
    let mut app = open(&project, false);
    serve(&mut app);
    let fills = painted(&mut app);
    let clip = clip_block(&fills);
    click(&mut app, clip.x + clip.w / 2, clip.y + clip.h / 2, 1);
    let rows = lattice_rows(&painted(&mut app));
    assert_eq!(rows, 0, "wanted no roll after selecting the audio clip, got {rows} lattice rows");
    app.take_requests();
    key(&mut app, Key::Right, false);
    let session = app.take_requests().into_iter().rev().find_map(|request| if let Request::Remember { json } = request { Some(json) } else { None }).expect("wanted the session remembered after the key");
    let session = json::parse(&session).expect("wanted the remembered session to parse");
    let start = session.get("project").and_then(Project::from_json).and_then(|project| project.tracks[2].clips.first().map(|clip| clip.start));
    let play_from = session.get("playheadBeat").and_then(Json::as_i64).unwrap_or(0);
    assert_eq!((start, play_from), (Some(TICKS_PER_BEAT), 0), "wanted the right arrow to nudge the selected audio clip a beat and leave the playhead, got a clip at {start:?} and the playhead at {play_from}");
}

#[test]
fn a_wave_in_the_browser_double_pressed_makes_an_audio_track_at_the_playhead() {
    let mut app = open(&Project::new(), true);
    app.take_requests();
    app.listed(ROOT, vec![Found { path: format!("{ROOT}/kick.wav"), name: String::from("kick.wav"), item: Item::Wave(u64::MAX) }]);
    app.take_requests();
    let fills = painted(&mut app);
    let row = name_cell(browser_rect(&fills), 1, 1);
    let (x, y) = (row.x + row.w / 2, row.y + row.h / 2);
    click(&mut app, x, y, 1);
    click(&mut app, x, y, 2);
    let project = serve(&mut app).expect("wanted the double press remembered");
    assert_eq!(kinds(&project), vec![("MIDI 1", false, 0), ("MIDI 2", false, 0), ("AUDIO 3", true, 1)], "wanted a new audio track holding the browser's wave, got {:?}", kinds(&project));
    let clip = &project.tracks[2].clips[0];
    assert_eq!((clip.start, clip.length, clip.label.as_str()), (0, KICK_TICKS, "kick"), "wanted the clip at the playhead, got {clip:?}");
}
