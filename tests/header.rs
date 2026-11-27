use bp::app::{App, Event, Request};
use bp::draw::{Fill, Rect, Tone};
use bp::json::Json;
use bp::project::{Clip, DEFAULT_BPM, LEAST_BPM, MOST_BPM, Project};
use bp::theory::{Root, ScaleName};
use support::{CHORD, HEIGHT, centre, remembered, session};

const WORDS_W: i32 = 1600;
const RAIL: i32 = 45;
const STEP: usize = 2;
const BUTTONS: usize = 18;
const SERVING_BUTTONS: usize = 19;
const SETTINGS_BUTTONS: usize = 2;
const SETTINGS_ITEM: i32 = 10;
const SECOND: usize = 1;
const SIXTEENTH: usize = 11;
const ITEM_H: i32 = 32;
const MENU_INSET: i32 = 12;
const TEMPO_LIFT: i32 = 40;
const TEMPO_BUTTON: usize = 6;
const PAST_EITHER_END: i32 = 1200;
const FAST_BPM: f64 = 280.0;
const ONE_STEP: i32 = 2;
const EDGE: i32 = 40;
const PAST_THE_WINDOW: i32 = 400;
const BORDER: i32 = 8;
const LOOPED: &str = ",\"loopStartBeat\":0,\"loopEndBeat\":4";
const SUITE_PORT: u16 = 59_393;

fn open() -> App {
    open_with("")
}

fn open_serving() -> App {
    open_with(&format!(",\"mcp\":true,\"mcpPort\":{SUITE_PORT}"))
}

fn open_with(settings: &str) -> App {
    let session = format!("{{\"project\":{},\"automation\":false,\"keyboard\":false,\"palette\":false{settings}}}", Project::new().to_json().write());
    let mut app = App::new();
    app.resume(&session);
    app
}

fn open_song(settings: &str, clip: bool) -> App {
    let mut project = Project::new();
    if clip {
        project.tracks[0].clips.push(Clip::chord(0, String::new(), &CHORD));
    }
    let session = format!("{{\"project\":{},\"automation\":false,\"keyboard\":false,\"palette\":false{settings}}}", project.to_json().write());
    let mut app = App::new();
    app.resume(&session);
    app
}

fn buttons_at(app: &mut App, width: i32) -> Vec<Rect> {
    let mut found: Vec<Rect> = Vec::new();
    for x in (0..width + PAST_THE_WINDOW).step_by(STEP) {
        app.event(Event::Move { x, y: RAIL / 2 });
        if let Some(rect) = rail_wash(app.draw(width, HEIGHT), Tone::Ink25)
            && !found.last().is_some_and(|last| same(*last, rect))
        {
            found.push(rect);
        }
    }
    found
}

fn painted(app: &mut App) -> Vec<Fill> {
    app.draw(WORDS_W, HEIGHT).to_vec()
}

fn same(a: Rect, b: Rect) -> bool {
    (a.x, a.y, a.w, a.h) == (b.x, b.y, b.w, b.h)
}

fn shown(rect: Rect) -> String {
    format!("{}x{} at {},{}", rect.w, rect.h, rect.x, rect.y)
}

fn rail_wash(fills: &[Fill], tone: Tone) -> Option<Rect> {
    fills.iter().find(|fill| fill.tone == tone && fill.rect.y < RAIL && fill.rect.h >= 30).map(|fill| fill.rect)
}

fn buttons(app: &mut App) -> Vec<Rect> {
    let mut found: Vec<Rect> = Vec::new();
    for x in (0..WORDS_W).step_by(STEP) {
        app.event(Event::Move { x, y: RAIL / 2 });
        if let Some(rect) = rail_wash(&painted(app), Tone::Ink25)
            && !found.last().is_some_and(|last| same(*last, rect))
        {
            found.push(rect);
        }
    }
    found
}

fn file_button(app: &mut App) -> Rect {
    (0..WORDS_W)
        .step_by(STEP)
        .find_map(|x| {
            app.event(Event::Move { x, y: RAIL / 2 });
            rail_wash(&painted(app), Tone::Ink25)
        })
        .expect("wanted the File button at the left of the rail")
}

fn click(app: &mut App, x: i32, y: i32) {
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
}

fn press(app: &mut App, rect: Rect) {
    let (x, y) = centre(rect);
    click(app, x, y);
}

fn lit(app: &mut App, rect: Rect) -> bool {
    painted(app).iter().any(|fill| fill.tone == Tone::Ink100 && same(fill.rect, rect))
}

fn flag(app: &mut App, key: &str, default: bool) -> bool {
    session(app).get(key).and_then(Json::as_bool).unwrap_or(default)
}

fn choose(app: &mut App, menu: Rect, item: usize) {
    click(app, menu.x + MENU_INSET, menu.y + menu.h + item as i32 * ITEM_H + ITEM_H / 2);
}

#[test]
fn the_header_holds_eighteen_buttons_left_to_right_and_the_robot_joins_with_the_mcp_on() {
    let mut app = open();
    let found = buttons(&mut app);
    let listed: Vec<String> = found.iter().map(|rect| shown(*rect)).collect();
    assert_eq!(found.len(), BUTTONS, "wanted {BUTTONS} buttons across the header with the MCP off, found {}: {listed:?}", found.len());
    for pair in found.windows(2) {
        assert!(pair[0].x + pair[0].w <= pair[1].x, "buttons overlap: {} and {}", shown(pair[0]), shown(pair[1]));
    }
    let last = found[BUTTONS - 1];
    assert!(last.x + last.w > WORDS_W - EDGE, "the last button is not at the right edge: {}", shown(last));
    let mut serving = open_serving();
    let served = buttons(&mut serving).len();
    assert_eq!(served, SERVING_BUTTONS, "wanted the robot joining the {BUTTONS} buttons with the MCP on, found {served}");
}

#[test]
fn each_header_button_does_its_own_thing() {
    let mut app = open_serving();
    let buttons = buttons(&mut app);
    let &[file, play, stop, record, metronome, keyboard, tempo, snap, key, scale, higher, lower, chord, hold, automation, lock, robot, big, feedback] = buttons.as_slice() else {
        panic!("wanted {SERVING_BUTTONS} buttons across the header with the MCP on, found {}", buttons.len());
    };
    press(&mut app, file);
    assert!(lit(&mut app, file), "pressing File did not open its menu");
    press(&mut app, file);
    assert!(!lit(&mut app, file), "pressing File again did not close its menu");
    press(&mut app, play);
    assert!(lit(&mut app, play), "pressing Play did not light it");
    press(&mut app, stop);
    assert!(!lit(&mut app, play), "pressing Stop did not stop the transport");
    press(&mut app, record);
    assert!(lit(&mut app, play), "pressing Record did not start the transport");
    press(&mut app, stop);
    press(&mut app, metronome);
    assert!(remembered(&mut app).metronome, "pressing Metronome did not turn the metronome on");
    press(&mut app, keyboard);
    assert!(flag(&mut app, "keyboard", true), "pressing Keyboard did not turn the keyboard on");
    press(&mut app, keyboard);
    assert!(!flag(&mut app, "keyboard", true), "pressing Keyboard again did not turn it off");
    let (x, y) = centre(tempo);
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Move { x, y: y - TEMPO_LIFT });
    app.event(Event::Up { x, y: y - TEMPO_LIFT });
    let bpm = remembered(&mut app).bpm;
    assert!(bpm > DEFAULT_BPM, "dragging Tempo up {TEMPO_LIFT} px left the tempo at {bpm}");
    press(&mut app, snap);
    assert!(lit(&mut app, snap), "pressing the snap did not open its menu");
    choose(&mut app, snap, SIXTEENTH);
    let chosen = session(&mut app).get("snap").and_then(Json::as_str).map(String::from);
    assert_eq!(chosen.as_deref(), Some("1/16"), "choosing the twelfth snap left the session at {chosen:?}");
    press(&mut app, hold);
    assert!(lit(&mut app, hold), "pressing Hold did not light it");
    press(&mut app, key);
    assert!(lit(&mut app, key), "pressing the key did not open its menu");
    choose(&mut app, key, SECOND);
    let root = remembered(&mut app).root;
    assert!(root == Root::ALL[SECOND], "choosing the second key left the root at {root:?}");
    press(&mut app, scale);
    assert!(lit(&mut app, scale), "pressing the scale did not open its menu");
    choose(&mut app, scale, SECOND);
    let name = remembered(&mut app).scale;
    assert!(name == ScaleName::ALL[SECOND], "choosing the second scale left the scale at {name:?}");
    press(&mut app, higher);
    let raised = remembered(&mut app).root;
    assert!(raised == Root::ALL[SECOND + 1], "pressing the up arrow did not move the key up a semitone, got {raised:?}");
    press(&mut app, lower);
    let lowered = remembered(&mut app).root;
    assert!(lowered == Root::ALL[SECOND], "pressing the down arrow did not move the key back down, got {lowered:?}");
    press(&mut app, chord);
    assert!(flag(&mut app, "palette", false), "pressing Chord did not show the palette");
    press(&mut app, automation);
    let auto = session(&mut app).get("auto").and_then(Json::as_str).map(String::from);
    assert_eq!(auto.as_deref(), Some("track"), "pressing AUTO did not show the selected track's lanes, got {auto:?}");
    press(&mut app, lock);
    assert!(remembered(&mut app).lock, "pressing Lock did not lock the project");
    press(&mut app, big);
    assert!(flag(&mut app, "big", false), "pressing BIG did not turn it on");
    press(&mut app, big);
    assert!(!flag(&mut app, "big", false), "pressing BIG again did not turn it off");
    press(&mut app, robot);
    let copied = app.take_requests().into_iter().any(|request| matches!(request, Request::Copy { text } if text.contains("mcp")));
    assert!(copied, "pressing the robot did not copy the MCP configuration");
    assert!(lit(&mut app, robot), "pressing the robot did not light it while COPIED! shows");
    press(&mut app, feedback);
    let opened = app.take_requests().into_iter().any(|request| matches!(request, Request::OpenLink { .. }));
    assert!(opened, "pressing Feedback did not open the feedback link");
}

#[test]
fn the_settings_screen_keeps_only_file_and_close() {
    let mut app = open();
    let file = file_button(&mut app);
    press(&mut app, file);
    choose(&mut app, file, SETTINGS_ITEM as usize);
    let settings = buttons(&mut app);
    assert_eq!(settings.len(), SETTINGS_BUTTONS, "wanted File and X on the settings screen, found {}", settings.len());
    press(&mut app, settings[SETTINGS_BUTTONS - 1]);
    let back = buttons(&mut app).len();
    assert_eq!(back, BUTTONS, "wanted the {BUTTONS} main buttons back after X, found {back}");
}

#[test]
fn the_header_folds_to_fit_the_window_rather_than_run_past_it() {
    let cases: [(&str, bool, &str, &[i32]); 4] = [("", false, "as a new song shows it", &[1280, 1300, 1351, 1366]), (LOOPED, false, "with BOUNCE showing", &[1280, 1366, 1440]), ("", true, "with FLATTEN showing", &[1280, 1440]), (LOOPED, true, "with BOUNCE and FLATTEN showing", &[1366, 1440])];
    for (settings, clip, state, widths) in cases {
        let left_of_feedback = |width: i32| -> Vec<Rect> { buttons_at(&mut open_song(settings, clip), width).into_iter().filter(|cell| cell.x + cell.w + BORDER != width).collect() };
        let every = left_of_feedback(WORDS_W).len();
        for &width in widths {
            let cells = left_of_feedback(width);
            let past: Vec<String> = cells.iter().filter(|cell| cell.x + cell.w + BORDER > width).map(|cell| shown(*cell)).collect();
            assert!(past.is_empty(), "wanted every header button inside a window {width} wide, {state}, got these past its edge: {past:?}");
            assert_eq!(cells.len(), every, "wanted the same {every} header buttons left of FEEDBACK at {width} wide as at {WORDS_W}, {state}, got {}", cells.len());
        }
    }
}

#[test]
fn lock_and_big_lit_side_by_side_are_divided_with_the_robot_hidden() {
    let mut app = open();
    let found = buttons(&mut app);
    let (lock, big) = (found[BUTTONS - 3], found[BUTTONS - 2]);
    press(&mut app, lock);
    press(&mut app, big);
    assert!(lit(&mut app, lock) && lit(&mut app, big), "wanted LOCK and BIG both lit once pressed");
    app.event(Event::Leave);
    let divider = Rect::new(big.x - BORDER, big.y, BORDER, big.h);
    let divided = painted(&mut app).iter().any(|fill| fill.tone == Tone::Ground && same(fill.rect, divider));
    assert!(divided, "wanted the two lit cells divided along their shared border, the hidden robot's cell not counting between them, at {}", shown(divider));
}

fn open_at(bpm: f64) -> App {
    let mut project = Project::new();
    project.bpm = bpm;
    let session = format!("{{\"project\":{},\"automation\":false,\"keyboard\":false,\"palette\":false}}", project.to_json().write());
    let mut app = App::new();
    app.resume(&session);
    app
}

fn tempo_dragged(app: &mut App, by: i32) -> f64 {
    let (x, y) = centre(buttons(app)[TEMPO_BUTTON]);
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Move { x, y: y - by });
    app.event(Event::Up { x, y: y - by });
    remembered(app).bpm
}

#[test]
fn the_tempo_drags_from_the_songs_slowest_to_its_fastest_and_a_fast_songs_tempo_stays_when_touched() {
    assert_eq!((LEAST_BPM, MOST_BPM), (20.0, 300.0), "wanted a song's tempo to run from 20 to 300");
    let mut app = open();
    assert_eq!(tempo_dragged(&mut app, PAST_EITHER_END), MOST_BPM, "wanted the tempo dragged far up to stop at the song's fastest");
    assert_eq!(tempo_dragged(&mut app, -PAST_EITHER_END), LEAST_BPM, "wanted the tempo dragged far down to stop at the song's slowest");
    let mut fast = open_at(FAST_BPM);
    assert_eq!(tempo_dragged(&mut fast, ONE_STEP), FAST_BPM + 1.0, "wanted a touch on a fast song's tempo to move it one step from where it stood, never down to a lower ceiling");
}
