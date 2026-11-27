use bp::app::{App, Event, Opened, Request};
use bp::draw::{Fill, Rect, Tone};
use bp::face::{FRAME, H};
use bp::grid::{BAR_W, BEAT_W, PLAYHEAD_W};
use bp::json::{self, Json};
use bp::keyboard::Key;
use bp::project::{LONGEST_TICKS, Project, beats};
use bp::ruler;
use support::{BLOCK, HEIGHT, SAMPLE_RATE, WIDTH, beat_frame, lane_rows, painted};

const PATH: &str = "/tmp/transport-under-test.json";
const MAX_FRAMES: usize = 4_096;
const BORDER: i32 = 8;
const FILE_CELL_CENTRE: (i32, i32) = (60, 26);
const SAVE_AS_DEFAULT_ITEM_CENTRE: (i32, i32) = (60, 261);
const EXPORT_ITEM_CENTRE: (i32, i32) = (60, 325);
const WITHOUT_BOTH_ITEM_CENTRE: (i32, i32) = (404, 389);
const PLACED: &str = ",\"playheadBeat\":12,\"loopStartBeat\":8,\"loopEndBeat\":16";
const SECOND_BAR: &str = ",\"playheadBeat\":4,\"loopStartBeat\":4,\"loopEndBeat\":8";
const QUARTERS: &str = ",\"snap\":\"1/4\"";
const WANTED: Transport = (Some(12.0), Some(8.0), Some(16.0));
const NOWHERE: Transport = (None, None, None);
const PLAYED_BEATS: usize = 6;
const WRAPPED_BEATS: usize = 2;

type Transport = (Option<f64>, Option<f64>, Option<f64>);

fn resumed(extra: &str) -> App {
    let session = format!("{{\"project\":{},\"automation\":false,\"keyboard\":false,\"palette\":false{extra}}}", Project::new().to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.draw(WIDTH, HEIGHT);
    app.take_requests();
    app
}

fn parsed(text: &str) -> Json {
    json::parse(text).expect("wanted the payload to parse as JSON")
}

fn transport(document: &Json) -> Transport {
    let beat = |key: &str| document.get(key).and_then(Json::as_f64);
    (beat("playheadBeat"), beat("loopStartBeat"), beat("loopEndBeat"))
}

fn key(app: &mut App, key: Key, command: bool) {
    app.event(Event::Key { key, shift: false, command, repeat: false });
}

fn saved(app: &mut App) -> (String, u64) {
    key(app, Key::Char('s'), true);
    let mut payloads: Vec<(String, u64)> = app.take_requests().into_iter().filter_map(|request| match request {
        Request::Write { json, save, .. } | Request::SaveAs { json, save } => Some((json, save)),
        _ => None,
    }).collect();
    assert_eq!(payloads.len(), 1, "wanted exactly one save request per press, got {}", payloads.len());
    payloads.remove(0)
}

fn remembered(app: &mut App) -> Json {
    let text = app.take_requests().into_iter().rev().find_map(|request| if let Request::Remember { json } = request { Some(json) } else { None }).expect("wanted the session remembered");
    parsed(&text)
}

fn opened(app: &mut App, text: &str) -> Json {
    assert_eq!(app.opened(String::from(PATH), Some(text), None), Opened::Project, "wanted the file to open as a project");
    remembered(app)
}

fn clicked(app: &mut App, path: &[(i32, i32)]) -> Vec<Request> {
    for &(x, y) in path {
        app.event(Event::Move { x, y });
        app.event(Event::Down { x, y, clicks: 1 });
        app.event(Event::Up { x, y });
        app.draw(WIDTH, HEIGHT);
    }
    app.take_requests()
}

fn versions(file: &Json) -> &[Json] {
    file.get("versions").and_then(Json::as_array).unwrap_or(&[])
}

fn beat_x(rows: &[Rect], beat: f64) -> i32 {
    rows.iter().map(|row| row.x).min().expect("wanted a lane row") - BAR_W + (beat * BEAT_W) as i32
}

fn ruler_middle() -> i32 {
    HEIGHT - (H - 2 * FRAME + BORDER) - BORDER - ruler::H / 2
}

fn playhead_lines(playing: &[Fill], stopped: &[Fill]) -> Vec<Rect> {
    playing.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.w == PLAYHEAD_W && fill.rect.h > ruler::H && !stopped.iter().any(|other| other.rect == fill.rect)).map(|fill| fill.rect).collect()
}

fn playhead_line(app: &mut App, stopped: &[Fill], when: &str) -> Rect {
    let lines = playhead_lines(&painted(app), stopped);
    let [line] = lines.as_slice() else {
        panic!("wanted one playhead line {when}, found {}: {lines:?}", lines.len());
    };
    *line
}

#[test]
fn a_saved_project_writes_its_playhead_and_loop_ahead_of_its_tracks() {
    let file = parsed(&saved(&mut resumed(PLACED)).0);
    assert_eq!(transport(&file), WANTED, "wanted the playhead at beat 12 and the loop from 8 to 16 in the file, got {:?}", transport(&file));
    let Json::Object(fields) = &file else {
        panic!("wanted the file to be an object");
    };
    let keys: Vec<&str> = fields.iter().map(|(key, _)| key.as_str()).collect();
    let at = |key: &str| keys.iter().position(|known| *known == key).unwrap_or_else(|| panic!("wanted {key} among {keys:?}"));
    assert!(at("playheadBeat") < at("tracks") && at("loopEndBeat") < at("tracks"), "wanted the playhead and loop written ahead of the tracks, got {keys:?}");
}

#[test]
fn opening_the_file_puts_the_playhead_and_loop_back() {
    let text = saved(&mut resumed(PLACED)).0;
    let session = opened(&mut resumed(""), &text);
    assert_eq!(transport(&session), WANTED, "wanted the opened project back on its playhead and loop, got {:?}", transport(&session));
}

#[test]
fn a_project_resting_at_bar_one_writes_neither_and_opens_there() {
    let text = saved(&mut resumed("")).0;
    assert_eq!(transport(&parsed(&text)), NOWHERE, "wanted no playhead or loop written for a project at bar one, got {:?}", transport(&parsed(&text)));
    let session = opened(&mut resumed(PLACED), &text);
    assert_eq!(transport(&session), NOWHERE, "wanted a file with neither to open at bar one with no loop, got {:?}", transport(&session));
}

#[test]
fn a_loop_painted_on_the_ruler_is_in_the_next_save() {
    let mut app = resumed(QUARTERS);
    let rows = lane_rows(&painted(&mut app));
    let (from, to, y) = (beat_x(&rows, 4.0) + 1, beat_x(&rows, 7.0) + 1, ruler_middle());
    app.event(Event::Move { x: from, y });
    app.event(Event::Down { x: from, y, clicks: 1 });
    app.event(Event::Move { x: to, y });
    app.event(Event::Up { x: to, y });
    let file = parsed(&saved(&mut app).0);
    assert_eq!(transport(&file), (Some(4.0), Some(4.0), Some(8.0)), "wanted a loop painted across the second bar saved with the playhead at its start, got {:?}", transport(&file));
}

#[test]
fn moving_the_playhead_rewrites_the_file_without_minting_a_version() {
    let mut app = resumed("");
    let (first, save) = saved(&mut app);
    app.saved(String::from(PATH), save);
    app.take_requests();
    key(&mut app, Key::Right, false);
    let (first, second) = (parsed(&first), parsed(&saved(&mut app).0));
    assert_eq!(transport(&second), (Some(4.0), None, None), "wanted the playhead stepped to the second bar in the rewritten file, got {:?}", transport(&second));
    assert_eq!((versions(&first).len(), versions(&second).len()), (1, 1), "wanted the one version kept when only the playhead moved");
    let kept = versions(&second)[0].get("project").expect("wanted the version to wrap a project");
    assert_eq!(transport(kept), NOWHERE, "wanted the version to hold the project without a playhead or loop, got {:?}", transport(kept));
}

#[test]
fn undo_leaves_the_playhead_and_loop_where_they_are() {
    let mut app = resumed(PLACED);
    let preset = "{\"kind\": \"rack\", \"rack\": [{\"kind\": \"core\"}, {\"kind\": \"freak\"}]}";
    assert_eq!(app.opened(String::from("marker.json"), Some(preset), None), Opened::Preset, "wanted the marker rack to load as a preset");
    let rows = |session: &Json| Project::from_json(session.get("project").expect("wanted the session to hold the project")).expect("wanted the remembered project to read").tracks[0].rack.len();
    let edited = remembered(&mut app);
    key(&mut app, Key::Char('z'), true);
    let undone = remembered(&mut app);
    assert_eq!((rows(&edited), rows(&undone)), (3, 2), "wanted the rack loaded under the MIDI row and then undone, got {} rows then {}", rows(&edited), rows(&undone));
    assert_eq!(transport(&undone), WANTED, "wanted undo to leave the playhead and loop alone, got {:?}", transport(&undone));
}

#[test]
fn save_as_default_leaves_the_playhead_and_loop_out() {
    let mut app = resumed(PLACED);
    let templates: Vec<String> = clicked(&mut app, &[FILE_CELL_CENTRE, SAVE_AS_DEFAULT_ITEM_CENTRE]).into_iter().filter_map(|request| match request {
        Request::Default { json: Some(json) } => Some(json),
        _ => None,
    }).collect();
    let [template] = templates.as_slice() else {
        panic!("wanted one default written after File then Save As Default, got {}", templates.len());
    };
    assert_eq!(transport(&parsed(template)), NOWHERE, "wanted the default project to start at bar one with no loop, got {:?}", transport(&parsed(template)));
}

#[test]
fn an_exported_project_carries_its_playhead_and_loop() {
    let mut app = resumed(PLACED);
    let exported: Vec<String> = clicked(&mut app, &[FILE_CELL_CENTRE, EXPORT_ITEM_CENTRE, WITHOUT_BOTH_ITEM_CENTRE]).into_iter().filter_map(|request| match request {
        Request::Preset { json, .. } => Some(json),
        _ => None,
    }).collect();
    let [file] = exported.as_slice() else {
        panic!("wanted one export after File, Export Project then Without Both, got {}", exported.len());
    };
    assert_eq!(transport(&parsed(file)), WANTED, "wanted the exported project on its playhead and loop, got {:?}", transport(&parsed(file)));
}

#[test]
fn a_backwards_loop_and_a_far_playhead_open_safely() {
    let mut document = Project::new().to_json();
    if let Json::Object(fields) = &mut document {
        for (key, beat) in [("playheadBeat", 1e12), ("loopStartBeat", 16.0), ("loopEndBeat", 8.0)] {
            fields.push((String::from(key), Json::Number(beat)));
        }
    }
    let mut app = resumed("");
    let session = opened(&mut app, &document.write());
    painted(&mut app);
    assert_eq!(transport(&session), (Some(beats(LONGEST_TICKS)), None, None), "wanted the playhead held to the longest song and the backwards loop dropped, got {:?}", transport(&session));
}

#[test]
fn playback_keeps_to_the_loop_an_opened_file_brings() {
    let text = saved(&mut resumed(SECOND_BAR)).0;
    let mut app = resumed("");
    let mut renderer = app.audio_started(SAMPLE_RATE, MAX_FRAMES, BLOCK as u32);
    opened(&mut app, &text);
    let stopped = painted(&mut app);
    key(&mut app, Key::Space, false);
    app.tick(BLOCK as f32 / SAMPLE_RATE as f32);
    let start = playhead_line(&mut app, &stopped, "the moment Play is pressed");
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut done = 0;
    while done < beat_frame(PLAYED_BEATS) {
        let frames = (beat_frame(PLAYED_BEATS) - done).min(BLOCK);
        renderer.render(&mut l[..frames], &mut r[..frames]);
        done += frames;
        app.tick(frames as f32 / SAMPLE_RATE as f32);
    }
    let wrapped = playhead_line(&mut app, &stopped, &format!("{PLAYED_BEATS} beats in"));
    let wanted = start.x + (WRAPPED_BEATS as f64 * BEAT_W).round() as i32;
    assert_eq!(wrapped.x, wanted, "wanted the line {WRAPPED_BEATS} beats into the one-bar loop after {PLAYED_BEATS} beats at x {wanted}, got x {}", wrapped.x);
}
