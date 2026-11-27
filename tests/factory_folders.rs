use bp::app::{App, Event, Request};
use bp::browser::{Found, Item};
use bp::draw::{Fill, Rect, Tone};
use bp::face::{CELL, PITCH};
use bp::factory::{FOLDERS, PRESETS};
use bp::json::{self, Json};
use bp::project::Project;
use support::{HEIGHT, WIDTH, browser_rect, call, centre, chevrons, painted, session};
use std::path::{Path, PathBuf};

const SAMPLES: &str = "/x/Samples";
const FACTORY: &str = "/x/Factory";
const INSTRUMENTS: &str = "/x/Factory/Instruments";
const EFFECTS: &str = "/x/Factory/Effects";
const HEARTS: &str = "/x/Hearts.json";
const RAIL: i32 = 45;
const STEP: usize = 2;
const SETTINGS_ITEM: i32 = 10;
const ITEM_H: i32 = 32;
const MENU_INSET: i32 = 12;
const COLUMN_X: i32 = 40;

fn resumed(samples: &str, unfolded: &[&str]) -> App {
    let list: Vec<String> = unfolded.iter().map(|path| format!("\"{path}\"")).collect();
    let session = format!("{{\"project\":{},\"aside\":\"browser\",\"palette\":false,\"keyboard\":false,\"folders\":[\"{samples}\"],\"unfolded\":[{}]}}", Project::new().to_json().write(), list.join(","));
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.draw(WIDTH, HEIGHT);
    app.take_requests();
    app
}

fn shell_requests(app: &mut App) -> Vec<Request> {
    app.take_requests().into_iter().filter(|request| !matches!(request, Request::Remember { .. })).collect()
}

fn list(path: &str) -> Request {
    Request::List { path: String::from(path) }
}

fn hearts() -> Request {
    Request::Read { path: String::from(HEARTS) }
}

fn folder(parent: &str, name: &str) -> Found {
    Found { path: format!("{parent}/{name}"), name: String::from(name), item: Item::Folder }
}

fn mark(browser: Rect, depth: i32, row: i32) -> Rect {
    Rect::new(browser.x + depth * PITCH, browser.y + row * PITCH, CELL, CELL)
}

fn text(json: &Json, key: &str) -> String {
    json.get(key).and_then(Json::as_str).map(String::from).unwrap_or_default()
}

fn shelves(app: &mut App) -> Vec<(String, String, bool)> {
    let reply = call(app, "library", "{}");
    assert!(!reply.failed, "wanted library answered, got {}", reply.text);
    let answer = json::parse(&reply.text).unwrap_or_else(|| panic!("wanted library to answer JSON, got {}", reply.text));
    let folders = answer.get("folders").and_then(Json::as_array).unwrap_or_else(|| panic!("wanted folders in {}", reply.text));
    folders.iter().map(|folder| (text(folder, "name"), text(folder, "path"), folder.get("factory").and_then(Json::as_bool).unwrap_or(false))).collect()
}

fn names(shelves: &[(String, String, bool)]) -> Vec<&str> {
    shelves.iter().map(|(name, _, _)| name.as_str()).collect()
}

fn shelf(name: &str, path: &str, factory: bool) -> (String, String, bool) {
    (String::from(name), String::from(path), factory)
}

fn click(app: &mut App, (x, y): (i32, i32)) {
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    app.event(Event::Leave);
}

fn factory_shown(app: &mut App) -> bool {
    session(app).get("factory").and_then(Json::as_bool).unwrap_or(true)
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

fn open_settings(app: &mut App) {
    let file = file_button(app);
    click(app, centre(file));
    click(app, (file.x + MENU_INSET, file.y + file.h + SETTINGS_ITEM * ITEM_H + ITEM_H / 2));
}

fn close_settings(app: &mut App) {
    for x in (0..WIDTH).rev().step_by(STEP) {
        app.event(Event::Move { x, y: RAIL / 2 });
        if let Some(rect) = rail_wash(&painted(app)) {
            click(app, centre(rect));
            return;
        }
    }
    panic!("wanted the X button at the right of the rail");
}

fn pressed_factory_control(app: &mut App) -> bool {
    open_settings(app);
    let control = factory_control(app);
    click(app, centre(control));
    let shown = factory_shown(app);
    close_settings(app);
    shown
}

fn factory_control(app: &mut App) -> Rect {
    let mut washes: Vec<Rect> = Vec::new();
    for y in (RAIL..HEIGHT).step_by(STEP) {
        app.event(Event::Move { x: COLUMN_X, y });
        if let Some(rect) = painted(app).iter().find(|fill| fill.tone == Tone::Ink25 && fill.rect.y >= RAIL && fill.rect.contains(COLUMN_X, y)).map(|fill| fill.rect)
            && washes.last() != Some(&rect)
        {
            washes.push(rect);
        }
    }
    let [.., factory, _ghost, _numbers, _follow] = washes.as_slice() else {
        panic!("wanted the settings column to end in the factory control, the ghost notes control, BAR NUMBERS and then FOLLOW, got {washes:?}");
    };
    *factory
}

fn real_folders() -> (PathBuf, PathBuf) {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join("factory_folders");
    let (factory, samples) = (root.join("Factory"), root.join("Samples"));
    for folder in [factory.join("Instruments"), factory.join("Effects"), samples.clone()] {
        std::fs::create_dir_all(&folder).unwrap_or_else(|error| panic!("wanted {}: {error}", folder.display()));
    }
    (factory, samples)
}

fn shown(path: &Path) -> String {
    path.to_str().map(String::from).expect("wanted the temporary folder's path to be text")
}

#[test]
fn the_factory_pins_instruments_then_effects_above_the_browsers_own_folders() {
    let mut app = resumed(SAMPLES, &[]);
    app.factory(String::from(FACTORY));
    assert_eq!(shell_requests(&mut app), vec![hearts()], "wanted the two closed factory folders pinned without a listing, only their hearts read");
    let wanted = vec![shelf("Instruments", INSTRUMENTS, true), shelf("Effects", EFFECTS, true), shelf("Samples", SAMPLES, false)];
    assert_eq!(shelves(&mut app), wanted, "wanted Instruments, then Effects, both of the factory, then the browser's own folder");
    let fills = painted(&mut app);
    let browser = browser_rect(&fills);
    let rows: Vec<(Rect, bool)> = (0..3).map(|row| (mark(browser, 0, row), false)).collect();
    assert_eq!(chevrons(&fills), rows, "wanted three closed root rows, the two factory folders above the browser's own");
}

#[test]
fn an_open_factory_folder_is_listed_at_once_and_holds_its_categories_above_the_next() {
    let mut app = resumed(SAMPLES, &[INSTRUMENTS]);
    app.factory(String::from(FACTORY));
    assert_eq!(shell_requests(&mut app), vec![list(INSTRUMENTS), hearts()], "wanted the remembered open Instruments listed, the closed Effects left alone, and the factory's hearts read");
    app.listed(INSTRUMENTS, vec![folder(INSTRUMENTS, "Basses"), folder(INSTRUMENTS, "Drums")]);
    let fills = painted(&mut app);
    let browser = browser_rect(&fills);
    let wanted = vec![(mark(browser, 0, 0), true), (mark(browser, 1, 1), false), (mark(browser, 1, 2), false), (mark(browser, 0, 3), false), (mark(browser, 0, 4), false)];
    assert_eq!(chevrons(&fills), wanted, "wanted Instruments open at the top with its categories beneath it, then Effects and the browser's own folder closed");
}

#[test]
fn the_settings_control_hides_both_folders_and_shows_them_again_in_order() {
    let mut app = resumed(SAMPLES, &[]);
    app.factory(String::from(FACTORY));
    app.take_requests();
    assert!(!pressed_factory_control(&mut app), "wanted the factory control, above the ghost notes control at the foot of the settings column, to hide the factory");
    assert_eq!(chevrons(&painted(&mut app)).len(), 1, "wanted both factory folders unpinned while hidden, the browser's own folder alone");
    assert!(pressed_factory_control(&mut app), "wanted a second press to show the factory again");
    let fills = painted(&mut app);
    let browser = browser_rect(&fills);
    let rows: Vec<(Rect, bool)> = (0..3).map(|row| (mark(browser, 0, row), false)).collect();
    assert_eq!(chevrons(&fills), rows, "wanted both folders pinned back at the top, closed, above the browser's own");
    assert_eq!(names(&shelves(&mut app)), vec!["Instruments", "Effects", "Samples"], "wanted Instruments then Effects back in their order");
    assert!(shell_requests(&mut app).is_empty(), "wanted the closed folders shown again without a listing");
}

#[test]
fn a_hidden_factory_pins_nothing_until_shown() {
    let session = format!("{{\"project\":{},\"aside\":\"browser\",\"palette\":false,\"keyboard\":false,\"folders\":[\"{SAMPLES}\"],\"factory\":false}}", Project::new().to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.draw(WIDTH, HEIGHT);
    app.take_requests();
    app.factory(String::from(FACTORY));
    assert!(shell_requests(&mut app).is_empty(), "wanted nothing listed for a hidden factory");
    assert_eq!(chevrons(&painted(&mut app)).len(), 1, "wanted the browser's own folder alone in the browser while the setting is off");
    assert_eq!(names(&shelves(&mut app)), vec!["Instruments", "Effects", "Samples"], "wanted the agent's library to keep both factory folders while the browser hides them");
}

#[test]
fn every_preset_sits_in_one_of_the_two_pinned_folders() {
    assert_eq!(FOLDERS, ["Instruments", "Effects"], "wanted the factory to pin Instruments and then Effects");
    for (name, _) in PRESETS {
        let folder = name.split('/').next().unwrap_or_default();
        assert!(FOLDERS.contains(&folder), "wanted {name} inside Instruments or Effects, the only folders the browser pins");
    }
}

#[test]
fn the_agent_cannot_rename_in_or_move_into_a_factory_folder() {
    let (factory, samples) = real_folders();
    let mut app = resumed(&shown(&samples), &[]);
    app.factory(shown(&factory));
    app.take_requests();
    let inside = shown(&factory.join("Effects").join("Glow.json"));
    let reply = call(&mut app, "rename", &format!("{{\"path\":{},\"name\":\"Gleam.json\"}}", Json::String(inside).write()));
    assert!(reply.failed, "wanted a rename inside Effects refused, got {}", reply.text);
    assert_eq!(reply.text, "Effects is a Factory folder, which stays as it is", "wanted the refusal to name the factory folder");
    let own = shown(&samples.join("Kick.wav"));
    let reply = call(&mut app, "move", &format!("{{\"path\":{},\"to\":{}}}", Json::String(own).write(), Json::String(shown(&factory.join("Instruments"))).write()));
    assert!(reply.failed, "wanted a move into Instruments refused, got {}", reply.text);
    assert_eq!(reply.text, "Instruments is a Factory folder, which takes nothing", "wanted the refusal to name the factory folder");
}
