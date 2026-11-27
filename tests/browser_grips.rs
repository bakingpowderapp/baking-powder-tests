use bp::app::{App, Event, Request};
use bp::browser::{Found, Item};
use bp::draw::{Fill, Rect, Tone};
use bp::face::{CELL, FRAME, PITCH};
use bp::json::{self, Json};
use bp::project::Project;
use support::{browser_rect, call, centre, chevrons, inked, painted};

const BASS: &str = "/x/Bass";
const KEYS: &str = "/x/Keys";
const PADS: &str = "/x/Pads";
const FACTORY: &str = "/x/Factory";
const ARM: i32 = 4;
const GRIP: i32 = 2;
const BELOW: i32 = 6;

fn open(folders: &[&str], unfolded: &[&str]) -> App {
    let list = |paths: &[&str]| paths.iter().map(|path| format!("\"{path}\"")).collect::<Vec<_>>().join(",");
    let session = format!("{{\"project\":{},\"aside\":\"browser\",\"keyboard\":false,\"folders\":[{}],\"unfolded\":[{}]}}", Project::new().to_json().write(), list(folders), list(unfolded));
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.take_requests();
    app
}

fn folder(parent: &str, name: &str) -> Found {
    Found { path: format!("{parent}/{name}"), name: String::from(name), item: Item::Folder }
}

fn mark(browser: Rect, depth: i32, row: i32) -> Rect {
    Rect::new(browser.x + depth * PITCH, browser.y + row * PITCH, CELL, CELL)
}

fn grip(browser: Rect, row: i32) -> Rect {
    Rect::new(browser.x + browser.w - CELL - GRIP * PITCH, browser.y + row * PITCH, CELL, CELL)
}

fn root_name(browser: Rect, row: i32) -> Rect {
    let (mark, remove) = (mark(browser, 0, row), grip(browser, row).x - PITCH);
    Rect::new(mark.x + PITCH, mark.y, remove + FRAME - mark.x - PITCH, CELL)
}

fn interior(rect: Rect) -> Rect {
    Rect::new(rect.x + FRAME, rect.y + FRAME, rect.w - 2 * FRAME, rect.h - 2 * FRAME)
}

fn lift(app: &mut App, from: Rect, to: (i32, i32)) {
    let (x, y) = centre(from);
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Move { x: to.0, y: to.1 });
}

fn let_go(app: &mut App, at: (i32, i32)) {
    app.take_requests();
    app.event(Event::Up { x: at.0, y: at.1 });
}

fn ghost(fills: &[Fill], browser: Rect) -> Option<Rect> {
    fills.iter().find(|fill| fill.tone == Tone::Ground && fill.rect.x == browser.x && fill.rect.w == browser.w && fill.rect.h == CELL && fill.rect.y >= browser.y).map(|fill| fill.rect)
}

fn taken(app: &mut App) -> (Vec<Request>, Vec<String>) {
    let mut requests = Vec::new();
    let mut folders = Vec::new();
    for request in app.take_requests() {
        match request {
            Request::Remember { json } => {
                let session = json::parse(&json).expect("wanted the remembered session to parse");
                folders = session.get("folders").and_then(Json::as_array).map_or_else(Vec::new, |items| items.iter().filter_map(Json::as_str).map(String::from).collect());
            }
            other => requests.push(other),
        }
    }
    (requests, folders)
}

fn shelf_names(app: &mut App) -> Vec<String> {
    let reply = call(app, "library", "{}");
    assert!(!reply.failed, "wanted library answered, got {}", reply.text);
    let answer = json::parse(&reply.text).unwrap_or_else(|| panic!("wanted library to answer JSON, got {}", reply.text));
    answer.get("folders").and_then(Json::as_array).map_or_else(Vec::new, |folders| folders.iter().filter_map(|folder| folder.get("name").and_then(Json::as_str)).map(String::from).collect())
}

#[test]
fn a_root_lifted_by_its_grip_travels_as_one_row_and_lands_where_it_is_let_go() {
    let mut app = open(&[BASS, KEYS, PADS], &[BASS]);
    app.listed(BASS, vec![folder(BASS, "Sub"), folder(BASS, "Growl")]);
    let fills = painted(&mut app);
    let browser = browser_rect(&fills);
    assert_eq!(chevrons(&fills).len(), 5, "wanted Bass open over its two folders, then Keys and Pads");
    let (x, y) = centre(grip(browser, 0));
    lift(&mut app, grip(browser, 0), (x, y + ARM - 1));
    assert_eq!(ghost(&painted(&mut app), browser), None, "wanted a grip moved under the arming distance to lift nothing");
    let below = (x, y + BELOW * PITCH);
    app.event(Event::Move { x: below.0, y: below.1 });
    let travelling = painted(&mut app);
    assert_eq!(ghost(&travelling, browser), Some(Rect::new(browser.x, browser.y + BELOW * PITCH, browser.w, CELL)), "wanted the lifted row's ghost under the pointer, {BELOW} rows down");
    let wanted = vec![(mark(browser, 0, 0), false), (mark(browser, 0, 1), false), (mark(browser, 0, BELOW), true)];
    assert_eq!(chevrons(&travelling), wanted, "wanted Keys and Pads closed up at the top and Bass travelling as one row, Sub and Growl folded away");
    let_go(&mut app, below);
    let (requests, folders) = taken(&mut app);
    assert!(requests.is_empty(), "wanted the move to ask nothing of the shell, got {requests:?}");
    assert_eq!(folders, vec![KEYS, PADS, BASS], "wanted Bass let go below the others to land last, the order kept with the session");
    let wanted = vec![(mark(browser, 0, 0), false), (mark(browser, 0, 1), false), (mark(browser, 0, 2), true), (mark(browser, 1, 3), false), (mark(browser, 1, 4), false)];
    assert_eq!(chevrons(&painted(&mut app)), wanted, "wanted Bass open on row 2 with Sub and Growl back beneath it");
}

#[test]
fn a_root_let_go_over_the_top_lands_first_and_a_press_that_stays_put_only_selects() {
    let mut app = open(&[KEYS, PADS, BASS], &[BASS]);
    app.listed(BASS, vec![folder(BASS, "Sub"), folder(BASS, "Growl")]);
    let browser = browser_rect(&painted(&mut app));
    let (x, _) = centre(grip(browser, 2));
    let top = (x, browser.y + CELL / 4);
    lift(&mut app, grip(browser, 2), top);
    let_go(&mut app, top);
    assert_eq!(taken(&mut app).1, vec![BASS, KEYS, PADS], "wanted Bass let go over the top of the first row to land first");
    let wanted = vec![(mark(browser, 0, 0), true), (mark(browser, 1, 1), false), (mark(browser, 1, 2), false), (mark(browser, 0, 3), false), (mark(browser, 0, 4), false)];
    assert_eq!(chevrons(&painted(&mut app)), wanted, "wanted Bass open on row 0 over Sub and Growl, then Keys and Pads");
    let (x, y) = centre(grip(browser, 3));
    lift(&mut app, grip(browser, 3), (x, y + ARM - 1));
    let_go(&mut app, (x, y + ARM - 1));
    assert_eq!(shelf_names(&mut app), vec!["Bass", "Keys", "Pads"], "wanted a press on Keys's grip that stays put to leave the order");
    let fills = painted(&mut app);
    assert_eq!(chevrons(&fills), wanted, "wanted the rows as they were after the press");
    assert!(inked(&fills, interior(root_name(browser, 3))), "wanted the press to select Keys, its name cell inverted");
}

#[test]
fn the_factory_folders_keep_the_top_when_a_root_is_let_go_above_them() {
    let mut app = open(&[BASS, KEYS], &[]);
    app.factory(String::from(FACTORY));
    let browser = browser_rect(&painted(&mut app));
    assert_eq!(shelf_names(&mut app), vec!["Instruments", "Effects", "Bass", "Keys"], "wanted the two factory folders pinned above the owner's");
    let (x, _) = centre(grip(browser, 3));
    let bar = (x, browser.y - PITCH + CELL / 2);
    lift(&mut app, grip(browser, 3), bar);
    let_go(&mut app, bar);
    assert_eq!(taken(&mut app).1, vec![KEYS, BASS], "wanted the session to keep the owner's folders in their new order, the factory's left out");
    assert_eq!(shelf_names(&mut app), vec!["Instruments", "Effects", "Keys", "Bass"], "wanted Keys let go over the search bar to land under the factory folders, above Bass");
    let wanted: Vec<(Rect, bool)> = (0..4).map(|row| (mark(browser, 0, row), false)).collect();
    assert_eq!(chevrons(&painted(&mut app)), wanted, "wanted four closed root rows, the factory's still first");
}
