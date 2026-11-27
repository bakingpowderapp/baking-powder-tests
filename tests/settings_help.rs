use bp::app::{App, Event, Request};
use bp::draw::{Fill, Rect, Tone};
use bp::json::{self, Json};
use bp::project::Project;
use support::{HEIGHT, WIDTH, centre, painted};

const RAIL: i32 = 45;
const STEP: usize = 2;
const SETTINGS_ITEM: i32 = 10;
const ITEM_H: i32 = 32;
const MENU_INSET: i32 = 12;
const COLUMN_X: i32 = 40;
const ROW: i32 = 33;
const BORDER: i32 = 8;
const PORT_ROW: usize = 3;
const PORT_KEY: &str = "mcpPort";
const SET_PORT: i64 = 47_213;
const DEFAULT_PORT: i64 = 9_393;

fn click(app: &mut App, (x, y): (i32, i32)) {
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    app.event(Event::Leave);
}

fn rail_wash(fills: &[Fill]) -> Option<Rect> {
    fills.iter().find(|fill| fill.tone == Tone::Ink25 && fill.rect.y < RAIL && fill.rect.h >= 30).map(|fill| fill.rect)
}

fn file_button(app: &mut App) -> Rect {
    for x in (0..WIDTH).step_by(STEP) {
        app.event(Event::Move { x, y: RAIL / 2 });
        if let Some(rect) = rail_wash(&painted(app)) {
            return rect;
        }
    }
    panic!("wanted the File button at the left of the rail");
}

fn open_settings() -> App {
    open_settings_with("")
}

fn open_settings_with(settings: &str) -> App {
    let session = format!("{{\"project\":{},\"automation\":false,\"keyboard\":false,\"palette\":false{settings}}}", Project::new().to_json().write());
    let mut app = App::new();
    app.resume(&session);
    let file = file_button(&mut app);
    click(&mut app, centre(file));
    click(&mut app, (file.x + MENU_INSET, file.y + file.h + SETTINGS_ITEM * ITEM_H + ITEM_H / 2));
    app
}

fn column(app: &mut App) -> Vec<Rect> {
    let mut washes: Vec<Rect> = Vec::new();
    for y in (RAIL..HEIGHT).step_by(STEP) {
        app.event(Event::Move { x: COLUMN_X, y });
        if let Some(rect) = painted(app).iter().find(|fill| fill.tone == Tone::Ink25 && fill.rect.y >= RAIL && fill.rect.contains(COLUMN_X, y)).map(|fill| fill.rect)
            && washes.last() != Some(&rect)
        {
            washes.push(rect);
        }
    }
    assert!(!washes.is_empty(), "wanted the settings controls to wash under the pointer");
    washes
}

fn readout(fills: &[Fill], low: Rect) -> Vec<Fill> {
    let left = low.x + low.w + 2 * BORDER;
    fills.iter().filter(|fill| fill.rect.y >= RAIL && fill.rect.x >= left && fill.rect.x < left + low.w + 2 * BORDER).cloned().collect()
}

fn explained(app: &mut App, low: Rect) -> bool {
    let shown = readout(&painted(app), low);
    shown.iter().any(|fill| fill.tone == Tone::Ink100) && shown.iter().any(|fill| fill.tone == Tone::Ink75)
}

#[test]
fn nothing_hovered_paints_no_readout() {
    let mut app = open_settings();
    let low = *column(&mut app).last().expect("wanted a lowest control");
    app.event(Event::Leave);
    let shown: Vec<Rect> = readout(&painted(&mut app), low).iter().map(|fill| fill.rect).collect();
    assert!(shown.is_empty(), "wanted nothing in the help column with nothing hovered, got {shown:?}");
}

#[test]
fn every_control_explains_itself_while_hovered() {
    let mut app = open_settings();
    let controls = column(&mut app);
    let low = *controls.last().expect("wanted a lowest control");
    for (index, control) in controls.iter().enumerate() {
        let (x, y) = centre(*control);
        app.event(Event::Move { x, y });
        assert!(explained(&mut app, low), "wanted control {} of the column to paint its readout in the help column while hovered, got nothing", index + 1);
    }
    app.event(Event::Leave);
    assert!(!explained(&mut app, low), "wanted the readout gone once the pointer leaves the column");
}

#[test]
fn the_counters_explain_themselves_too() {
    let mut app = open_settings();
    let low = *column(&mut app).last().expect("wanted a lowest control");
    for (counter, name) in ["LOAD", "DROPOUTS"].iter().enumerate() {
        let y = low.y + (counter as i32 + 1) * ROW + low.h / 2;
        app.event(Event::Move { x: COLUMN_X, y });
        assert!(explained(&mut app, low), "wanted {name} to paint its readout while hovered, got nothing");
    }
}

fn port(app: &mut App, last: &mut Option<Json>) -> i64 {
    app.event(Event::Quit);
    if let Some(text) = app.take_requests().into_iter().rev().find_map(|request| if let Request::Remember { json } = request { Some(json) } else { None }) {
        *last = json::parse(&text);
    }
    last.as_ref().and_then(|session| session.get(PORT_KEY)).and_then(Json::as_i64).unwrap_or(DEFAULT_PORT)
}

#[test]
fn a_double_press_on_the_port_row_resets_the_port() {
    let mut app = open_settings_with(&format!(",\"{PORT_KEY}\":{SET_PORT}"));
    let mut last = None;
    assert_eq!(port(&mut app, &mut last), SET_PORT, "wanted the session's port taken before the row is pressed");
    let (x, y) = centre(column(&mut app)[PORT_ROW]);
    for clicks in [1, 2] {
        app.event(Event::Down { x, y, clicks });
        app.event(Event::Up { x, y });
    }
    let reset = port(&mut app, &mut last);
    assert_eq!(reset, DEFAULT_PORT, "wanted a double-press on the port row to reset it to {DEFAULT_PORT}, as the row's help says, got {reset}");
}
