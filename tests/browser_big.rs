use bp::app::{App, Event, Request};
use bp::browser::{Found, Item};
use bp::draw::{Fill, Rect, Tone};
use bp::face::{CELL, PITCH};
use bp::json::{self, Json};
use bp::project::Project;
use support::{FOLD_OPEN, FOLD_OPEN_INSET, HEIGHT, WIDTH, call, inked, painted, session};

const ROOT: &str = "/x/Sounds";
const SOUNDS: usize = 60;
const RAIL: i32 = 45;
const TILE: i32 = 120;
const BORDER: i32 = 8;
const SECOND_TRACK: usize = 1;
const BIG_SQUARE: i32 = 3;
const TARGET_SQUARE: i32 = 2;
const FRAME: i32 = 2;
const DEEP: usize = 40;
const PANEL_ROWS: i32 = 14;
const FAR_DOWN: i32 = HEIGHT - 3 * PITCH;
const LIFT: i32 = 200;
const NOTCH: i32 = 4 * PITCH;

fn sound(at: usize) -> String {
    format!("{ROOT}/s{at:02}.wav")
}

fn open(state: &str) -> App {
    let session = format!("{{\"project\":{},\"aside\":\"browser\",\"keyboard\":false,\"folders\":[\"{ROOT}\"],\"unfolded\":[\"{ROOT}\"]{state}}}", Project::new().to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.take_requests();
    app.listed(ROOT, (0..SOUNDS).map(|at| Found { path: sound(at), name: format!("s{at:02}.wav"), item: Item::Wave(u64::MAX) }).collect());
    painted(&mut app);
    app.take_requests();
    app
}

fn grown() -> App {
    open(",\"browser_big\":true")
}

fn root_cell(fills: &[Fill]) -> Rect {
    let open = |fill: &&Fill| fill.tone == Tone::Ink100 && fill.rect.y >= RAIL && (fill.rect.w, fill.rect.h) == FOLD_OPEN && inked(fills, Rect::new(fill.rect.x + 2, fill.rect.y + 2, 10, 2));
    let mark = fills.iter().filter(open).map(|fill| fill.rect).min_by_key(|rect| (rect.y, rect.x)).expect("wanted the root folder's open mark painted in the browser");
    Rect::new(mark.x - FOLD_OPEN_INSET.0, mark.y - FOLD_OPEN_INSET.1, CELL, CELL)
}

fn row_y(root: Rect, row: i32) -> i32 {
    root.y + row * PITCH + CELL / 2
}

fn row_at(root: Rect, y: i32) -> i32 {
    (y - root.y) / PITCH
}

fn top_box(root: Rect, back: i32) -> (i32, i32) {
    let right = if root.x < WIDTH / 2 { WIDTH - root.x } else { panic!("wanted the big browser's root at the left of the window") };
    (right - CELL - back * PITCH + CELL / 2, root.y - PITCH + CELL / 2)
}

fn big_box(root: Rect) -> (i32, i32) {
    top_box(root, BIG_SQUARE)
}

fn lit_row(fills: &[Fill], root: Rect) -> Option<i32> {
    (0..(HEIGHT - root.y) / PITCH).find(|&row| inked(fills, Rect::new(root.x + PITCH + FRAME, root.y + row * PITCH + FRAME, CELL - 2 * FRAME, CELL - 2 * FRAME)))
}

fn scroll(app: &mut App, rows: i32) {
    app.event(Event::Scroll { x: WIDTH / 2, y: HEIGHT / 2, dx: 0, dy: -rows * PITCH });
}

fn click(app: &mut App, (x, y): (i32, i32)) -> Vec<Request> {
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    painted(app);
    app.take_requests()
}

fn read_asked(requests: &[Request]) -> Vec<&str> {
    requests.iter().filter_map(|request| if let Request::Read { path } = request { Some(path.as_str()) } else { None }).collect()
}

fn remembered_big(requests: Vec<Request>) -> Option<bool> {
    let text = requests.into_iter().rev().find_map(|request| if let Request::Remember { json } = request { Some(json) } else { None })?;
    Some(json::parse(&text).expect("wanted the remembered session to parse").get("browser_big").and_then(Json::as_bool).unwrap_or(false))
}

fn selected_track(app: &mut App) -> Option<f64> {
    let reply = call(app, "read", r#"{"path":"selectedTrack"}"#);
    json::parse(&reply.text).and_then(|track| track.as_f64())
}

fn second_tile() -> (i32, i32) {
    (TILE / 2, RAIL + SECOND_TRACK as i32 * (TILE - BORDER) + 2 * BORDER)
}

#[test]
fn the_big_browser_fills_the_window_under_the_header_and_shows_as_many_rows_as_fit() {
    let mut app = grown();
    let root = root_cell(&painted(&mut app));
    assert_eq!((root.x, root.y), (BORDER - 2, RAIL + BORDER - 2 + PITCH), "wanted the big browser's first row at the window's left edge, right under the header and the browser's own top row");
    let row = row_at(root, FAR_DOWN);
    assert!(row > PANEL_ROWS, "wanted the press to land on a row past the {PANEL_ROWS} the panel shows, got row {row}");
    let requests = click(&mut app, (WIDTH / 2, FAR_DOWN));
    assert_eq!(read_asked(&requests), [sound(row as usize - 1)], "wanted the sound on row {row}, far down the window, heard on a press");
    let mut small = open("");
    let requests = click(&mut small, (WIDTH / 2, FAR_DOWN));
    assert!(read_asked(&requests).is_empty(), "wanted no sound under that press with the browser in its panel");
}

#[test]
fn a_press_on_big_grows_the_browser_another_puts_it_back_and_the_session_remembers_it() {
    let mut app = open("");
    let panel = support::browser_rect(&painted(&mut app));
    let big = (panel.x + panel.w - CELL - BIG_SQUARE * PITCH + CELL / 2, panel.y - PITCH + CELL / 2);
    assert_eq!(remembered_big(click(&mut app, big)), Some(true), "wanted a press on BIG to grow the browser and the session to remember it");
    let root = root_cell(&painted(&mut app));
    assert!(root.x < panel.x && root.y < panel.y, "wanted the browser grown to the window's left edge under the header, got its first row at {root:?} against {panel:?} in its panel");
    assert_eq!(remembered_big(click(&mut app, big_box(root))), Some(false), "wanted a second press on BIG to put the browser back and the session to remember that");
    assert_eq!(support::browser_rect(&painted(&mut app)), panel, "wanted the browser back in its panel");
    let resumed = session(&mut grown_then_touched());
    assert_eq!(resumed.get("browser_big").and_then(Json::as_bool), Some(true), "wanted a session resumed big to stay big");
}

fn grown_then_touched() -> App {
    let mut app = grown();
    let reply = call(&mut app, "write", r#"{"path":"bpm","value":100}"#);
    assert!(!reply.failed, "wanted the tempo written, got {}", reply.text);
    app
}

#[test]
fn nothing_under_the_big_browser_takes_a_press_or_a_scroll() {
    let mut small = open("");
    click(&mut small, second_tile());
    assert_eq!(selected_track(&mut small), Some(SECOND_TRACK as f64), "wanted a press on the second track's tile to select it with the browser in its panel");
    let mut app = grown();
    click(&mut app, second_tile());
    assert_eq!(selected_track(&mut app), Some(0.0), "wanted the same press under the big browser to select no track");
    let root = root_cell(&painted(&mut app));
    app.event(Event::Scroll { x: second_tile().0, y: second_tile().1, dx: 0, dy: -NOTCH });
    let requests = click(&mut app, (WIDTH / 2, row_y(root, 1)));
    assert_eq!(read_asked(&requests), [sound(NOTCH as usize / PITCH as usize)], "wanted a scroll anywhere over the big browser to scroll its list, the first row then holding a later sound");
}

#[test]
fn a_file_pressed_and_moved_in_the_big_browser_starts_no_drag_and_the_browser_stays_big() {
    let mut app = grown();
    let root = root_cell(&painted(&mut app));
    let (x, y) = (WIDTH / 2, row_y(root, 2));
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Move { x: x + LIFT, y: y + LIFT });
    app.event(Event::Move { x: WIDTH + LIFT, y: y + LIFT });
    let dragged = app.take_requests().iter().any(|request| matches!(request, Request::Drag { .. }));
    app.event(Event::Up { x: x + LIFT, y: y + LIFT });
    assert!(!dragged, "wanted no file handed to the system to drag out of the big browser");
    let tracks = call(&mut app, "read", r#"{"path":"tracks","outline":true}"#);
    assert!(!tracks.text.contains("clips") && !tracks.text.contains("audio"), "wanted no clip and no audio track made by the move, got {}", tracks.text);
    assert_eq!(root_cell(&painted(&mut app)), root, "wanted the browser still big after the move");
    let mut small = open("");
    let panel = support::browser_rect(&painted(&mut small));
    let (x, y) = (panel.x + panel.w / 2, panel.y + 2 * PITCH + CELL / 2);
    small.event(Event::Move { x, y });
    small.event(Event::Down { x, y, clicks: 1 });
    small.event(Event::Move { x, y: y + LIFT });
    small.event(Event::Move { x: WIDTH + LIFT, y: y + LIFT });
    assert!(small.take_requests().iter().any(|request| matches!(request, Request::Drag { .. })), "wanted a file pressed and moved out of the window from the panel handed to the system to drag, as before");
}

#[test]
fn a_press_on_the_bulls_eye_brings_the_previewing_sound_back_to_the_middle_and_does_nothing_with_none_previewing() {
    let mut app = grown();
    let root = root_cell(&painted(&mut app));
    let list_rows = (HEIGHT - RAIL - 2 * BORDER + 2 * FRAME) / PITCH - 1;
    scroll(&mut app, 30);
    click(&mut app, top_box(root, TARGET_SQUARE));
    let requests = click(&mut app, (WIDTH / 2, row_y(root, 0)));
    assert_eq!(read_asked(&requests), [sound(29)], "wanted a press on the bull's-eye with nothing previewing to leave the list where it was scrolled");
    let pressed = DEEP as i32 + 1 - 30;
    let requests = click(&mut app, (WIDTH / 2, row_y(root, pressed)));
    assert_eq!(read_asked(&requests), [sound(DEEP)], "wanted the press to preview the sound on that row");
    assert_eq!(lit_row(&painted(&mut app), root), Some(pressed), "wanted the previewing sound's mark lit on its row");
    scroll(&mut app, -60);
    assert_eq!(lit_row(&painted(&mut app), root), None, "wanted the previewing sound out of sight with the list scrolled back to its top");
    click(&mut app, top_box(root, TARGET_SQUARE));
    assert_eq!(lit_row(&painted(&mut app), root), Some((list_rows - 1) / 2), "wanted a press on the bull's-eye to bring the previewing sound's row to the middle of the rows showing");
    assert_eq!(remembered_big(app.take_requests()), None, "wanted the bull's-eye to change nothing the session remembers");
}
