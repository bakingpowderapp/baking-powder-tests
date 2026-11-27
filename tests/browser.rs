use bp::app::{App, Event, Request};
use bp::arp::Arp;
use bp::keyboard::Key;
use bp::browser::{Found, Item};
use bp::draw::{Fill, Rect, Tone};
use bp::face::{CELL, FRAME, PITCH, ROWS, W};
use bp::fx::Kind;
use bp::json::{self, Json};
use bp::project::{Preset, Project, RACK_ROWS, Row};
use support::{WIDTH, browser_rect, centre, chevrons, core, effect, face_rect, inked, midi, painted, session, unseeded};

const ROOT: &str = "/x/Presets";
const BORDER: i32 = 8;
const SPECTRUM_SQUARE: i32 = 3;
const BROWSER_SQUARE: i32 = 0;
const HEADER_Y: i32 = 10;
const ASK: (i32, i32) = (400, 156);
const OPEN_SQUARE: i32 = 0;
const REMOVE_SQUARE: i32 = 3;
const HEART_SQUARE: i32 = 2;

fn open(folders: &[&str], unfolded: &[&str], project: &Project) -> App {
    let list = |paths: &[&str]| paths.iter().map(|path| format!("\"{path}\"")).collect::<Vec<_>>().join(",");
    let session = format!("{{\"project\":{},\"aside\":\"browser\",\"keyboard\":false,\"folders\":[{}],\"unfolded\":[{}]}}", project.to_json().write(), list(folders), list(unfolded));
    let mut app = App::new();
    app.resume(&session);
    app
}

fn browsing() -> App {
    let mut app = open(&[ROOT], &[ROOT], &Project::new());
    app.take_requests();
    app
}

fn under(name: &str) -> String {
    format!("{ROOT}/{name}")
}

fn folder(name: &str) -> Found {
    Found { path: under(name), name: String::from(name), item: Item::Folder }
}

fn file(name: &str, text: &str) -> Found {
    Found { path: under(&format!("{name}.json")), name: format!("{name}.json"), item: Item::read(text) }
}

fn rack_text(kind: Kind) -> String {
    Preset::Rack(bp::project::Sound { rack: vec![core(1.0), effect(kind)], lfos: Vec::new() }).to_json().write()
}

fn row_text(kind: Kind) -> String {
    Preset::Row(effect(kind), Vec::new()).to_json().write()
}

fn square(face: Rect, slot: i32) -> Rect {
    Rect::new(face.x + W + BORDER - 2 * FRAME, face.y + slot * PITCH, CELL, CELL)
}

fn mark_cell(browser: Rect, depth: i32, row: i32) -> Rect {
    Rect::new(browser.x + depth * PITCH, browser.y + row * PITCH, CELL, CELL)
}

fn name_cell(browser: Rect, depth: i32, row: i32, folder: bool) -> Rect {
    let mark = mark_cell(browser, depth, row);
    let right = end_square(browser, row, if folder { REMOVE_SQUARE } else { HEART_SQUARE }).x + FRAME;
    Rect::new(mark.x + PITCH, mark.y, right - mark.x - PITCH, CELL)
}

fn end_square(browser: Rect, row: i32, back: i32) -> Rect {
    Rect::new(browser.x + browser.w - CELL - back * PITCH, browser.y + row * PITCH, CELL, CELL)
}

fn asked(fills: &[Fill]) -> bool {
    fills.iter().any(|fill| fill.tone == Tone::Ground && (fill.rect.w, fill.rect.h) == ASK)
}

fn interior(rect: Rect) -> Rect {
    Rect::new(rect.x + FRAME, rect.y + FRAME, rect.w - 2 * FRAME, rect.h - 2 * FRAME)
}

fn click(app: &mut App, rect: Rect, clicks: i32) {
    let (x, y) = centre(rect);
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks });
    app.event(Event::Up { x, y });
}

fn double_click(app: &mut App, rect: Rect) {
    click(app, rect, 1);
    click(app, rect, 2);
}

fn drag(app: &mut App, from: Rect, to: (i32, i32)) {
    let (x, y) = centre(from);
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Move { x: to.0, y: to.1 });
    app.event(Event::Up { x: to.0, y: to.1 });
}

fn press(app: &mut App, key: Key, shift: bool) {
    app.event(Event::Key { key, shift, command: false, repeat: false });
}

fn type_word(app: &mut App, word: &str) {
    for c in word.chars() {
        press(app, Key::Char(c.to_ascii_lowercase()), c.is_ascii_uppercase());
    }
}

fn shell_requests(app: &mut App) -> Vec<Request> {
    app.take_requests().into_iter().filter(|request| !matches!(request, Request::Remember { .. })).collect()
}

fn rack_of(session: &Json) -> Vec<Row> {
    Project::from_json(session.get("project").expect("wanted the remembered session to hold the project")).expect("wanted the remembered project to read").tracks[0].rack.clone()
}

fn strings(session: &Json, key: &str) -> Vec<String> {
    session.get(key).and_then(Json::as_array).map_or_else(Vec::new, |items| items.iter().filter_map(Json::as_str).map(String::from).collect())
}

fn list(path: &str) -> Request {
    Request::List { path: String::from(path) }
}

fn hearts(folder: &str) -> Request {
    Request::Read { path: format!("{folder}/Hearts.json") }
}

#[test]
fn resuming_lists_the_open_folders_and_the_ones_they_reveal() {
    let mut app = open(&[ROOT, "/x/Closed"], &[ROOT, &under("Bass")], &Project::new());
    assert_eq!(shell_requests(&mut app), vec![list(ROOT), hearts("/x/Closed"), hearts(ROOT)], "wanted the open root listed at resume, the closed one left alone, and each folder's hearts read");
    app.listed(ROOT, vec![folder("Bass"), folder("Arp"), file("Chrome", &rack_text(Kind::Delay))]);
    assert_eq!(shell_requests(&mut app), vec![list(&under("Bass"))], "wanted the remembered open subfolder listed as soon as the root revealed it");
    app.listed(&under("Bass"), vec![file("Knuckle", &rack_text(Kind::Reverb))]);
    app.event(Event::Focus);
    assert_eq!(shell_requests(&mut app), Vec::new(), "wanted the app coming back to the front to read no folder again, Refresh being the one way to");
}

#[test]
fn a_dropped_folder_shows_the_browser_and_is_remembered() {
    let mut app = App::new();
    app.resume(&format!("{{\"project\":{}}}", Project::new().to_json().write()));
    app.take_requests();
    app.add_folder(String::from(ROOT));
    let requests = app.take_requests();
    assert_eq!(requests.first(), Some(&list(ROOT)), "wanted the dropped folder listed at once, got {requests:?}");
    let session = requests.iter().rev().find_map(|request| if let Request::Remember { json } = request { json::parse(json) } else { None }).expect("wanted the session remembered after the drop");
    assert_eq!(session.get("aside").and_then(Json::as_str), Some("browser"), "wanted the drop to show the browser");
    assert_eq!(strings(&session, "folders"), vec![ROOT], "wanted the dropped folder kept as a root");
    assert_eq!(strings(&session, "unfolded"), vec![ROOT], "wanted the dropped folder open");
}

#[test]
fn folders_lead_the_rows_and_names_sort_case_blind() {
    let mut app = browsing();
    app.listed(ROOT, vec![file("b", &rack_text(Kind::Delay)), folder("zed"), file("A", &rack_text(Kind::Reverb)), folder("Alpha")]);
    let fills = painted(&mut app);
    let browser = browser_rect(&fills);
    let marks = chevrons(&fills);
    let wanted = vec![(mark_cell(browser, 0, 0), true), (mark_cell(browser, 1, 1), false), (mark_cell(browser, 1, 2), false)];
    assert_eq!(marks, wanted, "wanted the root open on row 0 and the two folders closed on rows 1 and 2");
    double_click(&mut app, name_cell(browser, 1, 3, false));
    assert_eq!(unseeded(&rack_of(&session(&mut app))), vec![midi(Arp::DEFAULT), core(1.0), effect(Kind::Reverb)], "wanted row 3 to be A.json, the reverb rack, sorted before b.json");
    double_click(&mut app, name_cell(browser, 1, 4, false));
    assert_eq!(unseeded(&rack_of(&session(&mut app))), vec![midi(Arp::DEFAULT), core(1.0), effect(Kind::Delay)], "wanted row 4 to be b.json, the delay rack");
}

#[test]
fn a_chevron_opens_a_folder_and_lists_it() {
    let mut app = browsing();
    app.listed(ROOT, vec![folder("Bass")]);
    app.take_requests();
    let browser = browser_rect(&painted(&mut app));
    click(&mut app, mark_cell(browser, 1, 1), 1);
    assert_eq!(shell_requests(&mut app), vec![list(&under("Bass"))], "wanted the opened folder listed");
    assert_eq!(chevrons(&painted(&mut app))[1], (mark_cell(browser, 1, 1), true), "wanted the folder's chevron pointing down once open");
    click(&mut app, mark_cell(browser, 1, 1), 1);
    assert_eq!(shell_requests(&mut app), Vec::new(), "wanted no listing when a folder closes");
    assert_eq!(chevrons(&painted(&mut app))[1], (mark_cell(browser, 1, 1), false), "wanted the chevron pointing right again once closed");
}

#[test]
fn a_row_preset_joins_the_rack_and_a_full_rack_refuses_it() {
    let mut app = browsing();
    app.listed(ROOT, vec![file("Delay", &row_text(Kind::Delay))]);
    let browser = browser_rect(&painted(&mut app));
    double_click(&mut app, name_cell(browser, 1, 1, false));
    assert_eq!(unseeded(&rack_of(&session(&mut app))), vec![midi(Arp::DEFAULT), core(1.0), effect(Kind::Delay)], "wanted the delay row to join the track's rack");
    let mut full = Project::new();
    full.tracks[0].rack = [midi(Arp::DEFAULT), core(1.0)].into_iter().chain((2..RACK_ROWS).map(|_| effect(Kind::Utility))).collect();
    let mut app = open(&[ROOT], &[ROOT], &full);
    app.take_requests();
    app.listed(ROOT, vec![file("Delay", &row_text(Kind::Delay))]);
    let browser = browser_rect(&painted(&mut app));
    double_click(&mut app, name_cell(browser, 1, 1, false));
    let project = Project::from_json(session(&mut app).get("project").expect("wanted the remembered session to hold the project")).expect("wanted the remembered project to read");
    assert_eq!(unseeded(&project.tracks[0].rack), full.tracks[0].rack, "wanted a full rack left as it was, got {} rows", project.tracks[0].rack.len());
}

#[test]
fn a_preset_dragged_onto_the_core_loads_and_dropped_elsewhere_does_not() {
    let mut app = browsing();
    app.listed(ROOT, vec![file("Chrome", &rack_text(Kind::Motion))]);
    let fills = painted(&mut app);
    let (browser, face) = (browser_rect(&fills), face_rect(&fills));
    let row = name_cell(browser, 1, 1, false);
    let (x, y) = centre(row);
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    let (fx, fy) = (face.x + face.w / 2, face.y + face.h / 2);
    app.event(Event::Move { x: fx, y: fy });
    let lifted = painted(&mut app);
    let ghost = Rect::new(fx - row.w / 2, fy - row.h / 2, row.w, row.h);
    assert!(lifted.iter().any(|fill| fill.tone == Tone::Ground && fill.rect == ghost), "wanted the lifted row's ghost under the pointer at {ghost:?}");
    app.event(Event::Up { x: fx, y: fy });
    assert_eq!(unseeded(&rack_of(&session(&mut app))), vec![midi(Arp::DEFAULT), core(1.0), effect(Kind::Motion)], "wanted the preset let go over the Core to replace the rack, the MIDI row kept at row 0");
    let mut untouched = browsing();
    untouched.listed(ROOT, vec![file("Chrome", &rack_text(Kind::Motion))]);
    let browser = browser_rect(&painted(&mut untouched));
    drag(&mut untouched, name_cell(browser, 1, 1, false), (WIDTH / 2, HEADER_Y));
    assert_eq!(unseeded(&rack_of(&session(&mut untouched))), vec![midi(Arp::DEFAULT), core(1.0)], "wanted a preset let go over the header, off the tracks, to change nothing");
}

#[test]
fn return_renames_through_the_shell_and_a_taken_name_is_refused() {
    let mut app = browsing();
    app.listed(ROOT, vec![file("a", &rack_text(Kind::Delay)), file("b", &rack_text(Kind::Reverb))]);
    app.take_requests();
    let browser = browser_rect(&painted(&mut app));
    let row = name_cell(browser, 1, 2, false);
    click(&mut app, row, 1);
    assert!(inked(&painted(&mut app), interior(row)), "wanted the pressed row inverted");
    press(&mut app, Key::Return, false);
    type_word(&mut app, "Warm");
    press(&mut app, Key::Return, false);
    let renamed = Request::Rename { path: under("b.json"), name: String::from("Warm.json") };
    assert_eq!(shell_requests(&mut app), vec![renamed, list(ROOT)], "wanted the shell asked to rename b.json to Warm.json and list the folder again");
    click(&mut app, row, 1);
    press(&mut app, Key::Return, false);
    type_word(&mut app, "A");
    press(&mut app, Key::Return, false);
    assert_eq!(shell_requests(&mut app), Vec::new(), "wanted a rename to a sibling's name, case aside, refused");
    press(&mut app, Key::Escape, false);
    assert!(!inked(&painted(&mut app), interior(row)), "wanted Escape to clear the selection");
}

#[test]
fn delete_trashes_a_preset_and_leaves_a_root() {
    let mut app = browsing();
    app.listed(ROOT, vec![file("Chrome", &rack_text(Kind::Delay))]);
    app.take_requests();
    let browser = browser_rect(&painted(&mut app));
    click(&mut app, name_cell(browser, 1, 1, false), 1);
    press(&mut app, Key::Delete, false);
    assert_eq!(shell_requests(&mut app), vec![Request::Delete { path: under("Chrome.json") }, list(ROOT)], "wanted the shell asked to trash Chrome.json and list the folder again");
    click(&mut app, name_cell(browser, 0, 0, true), 1);
    press(&mut app, Key::Delete, false);
    assert!(!asked(&painted(&mut app)), "wanted Delete on a root to ask nothing");
    assert_eq!(chevrons(&painted(&mut app)).len(), 1, "wanted Delete on a root to keep it");
    assert_eq!(shell_requests(&mut app), Vec::new(), "wanted Delete on a root to touch nothing on disk");
}

#[test]
fn the_arrow_square_shows_a_folder_on_disk() {
    let mut app = browsing();
    app.listed(ROOT, vec![file("Chrome", &rack_text(Kind::Delay))]);
    app.take_requests();
    let browser = browser_rect(&painted(&mut app));
    click(&mut app, end_square(browser, 0, OPEN_SQUARE), 1);
    assert_eq!(shell_requests(&mut app), vec![Request::Reveal { path: String::from(ROOT) }], "wanted the arrow square to ask the shell to show the folder on disk and nothing else");
    assert!(!asked(&painted(&mut app)), "wanted the arrow square to ask the owner nothing");
    assert_eq!(chevrons(&painted(&mut app)).len(), 1, "wanted the folder row kept");
}

#[test]
fn the_plus_row_asks_for_a_folder() {
    let mut app = browsing();
    app.listed(ROOT, vec![file("Chrome", &rack_text(Kind::Delay))]);
    app.take_requests();
    let browser = browser_rect(&painted(&mut app));
    click(&mut app, Rect::new(browser.x, browser.y + 2 * PITCH, browser.w, CELL), 1);
    assert_eq!(shell_requests(&mut app), vec![Request::Folder { at: None }], "wanted the + row after the last row to run the folder panel");
}

#[test]
fn the_s_and_f_squares_choose_what_sits_beside_the_core() {
    let mut app = browsing();
    let face = face_rect(&painted(&mut app));
    click(&mut app, square(face, SPECTRUM_SQUARE), 1);
    assert_eq!(session(&mut app).get("aside").and_then(Json::as_str), None, "wanted S to show the spectrum, the session's default");
    click(&mut app, square(face, SPECTRUM_SQUARE), 1);
    assert_eq!(session(&mut app).get("aside").and_then(Json::as_str), Some("blank"), "wanted S pressed again to blank the cell");
    click(&mut app, square(face, BROWSER_SQUARE), 1);
    assert_eq!(session(&mut app).get("aside").and_then(Json::as_str), Some("browser"), "wanted F to show the browser");
    assert!(!chevrons(&painted(&mut app)).is_empty(), "wanted the browser painted once F lit");
    click(&mut app, square(face, SPECTRUM_SQUARE), 1);
    assert!(chevrons(&painted(&mut app)).is_empty(), "wanted the browser gone once S took the cell");
}

#[test]
fn the_wheel_scrolls_the_rows() {
    let mut app = browsing();
    app.listed(ROOT, (0..ROWS as usize + 2).map(|index| folder(&format!("Folder {index:02}"))).collect());
    let browser = browser_rect(&painted(&mut app));
    let (x, y) = centre(browser);
    app.event(Event::Scroll { x, y, dx: 0, dy: -3 * PITCH });
    let marks = chevrons(&painted(&mut app));
    assert_eq!(marks.first(), Some(&(mark_cell(browser, 1, 0), false)), "wanted three rows scrolled away so a subfolder sits on row 0");
    assert_eq!(marks.len(), ROWS as usize - 1, "wanted every visible row under the search bar a folder after the scroll, got {}", marks.len());
    app.event(Event::Scroll { x, y, dx: 0, dy: 40 * PITCH });
    assert_eq!(chevrons(&painted(&mut app)).first(), Some(&(mark_cell(browser, 0, 0), true)), "wanted a scroll back up to stop at the root");
}
