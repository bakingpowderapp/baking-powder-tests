use bp::app::{App, Event, Opened, Request};
use bp::keyboard::Key;
use bp::browser::{Found, Item};
use bp::draw::{Fill, Rect, Tone};
use bp::face::{CELL, FRAME, H, PITCH};
use bp::fx::Kind;
use bp::project::{Preset, Project};
use support::{FOLD_OPEN, FOLD_OPEN_INSET, HEIGHT, WIDTH, centre, core, effect, inked, painted};

const ROOT: &str = "/x/Files";
const CURRENT: &str = "/x/Current.json";
const ASK: (i32, i32) = (400, 156);
const ASK_BUTTON_H: i32 = 53;
const RACK_ROW: i32 = 1;
const ROW_PRESET_ROW: i32 = 2;
const PROJECT_ROW: i32 = 3;
const WAVE_ROW: i32 = 4;
const UNREADABLE_ROW: i32 = 5;

fn open(path: Option<&str>, project: &Project) -> App {
    let path = path.map_or_else(String::new, |path| format!("\"path\":\"{path}\","));
    let session = format!("{{\"project\":{},{path}\"aside\":\"browser\",\"keyboard\":false,\"folders\":[\"{ROOT}\"],\"unfolded\":[\"{ROOT}\"]}}", project.to_json().write());
    let mut app = App::new();
    app.resume(&session);
    app.take_requests();
    app
}

fn under(name: &str) -> String {
    format!("{ROOT}/{name}")
}

fn file(name: &str, text: &str) -> Found {
    Found { path: under(&format!("{name}.json")), name: format!("{name}.json"), item: Item::read(text) }
}

fn wave(name: &str) -> Found {
    Found { path: under(&format!("{name}.wav")), name: format!("{name}.wav"), item: Item::Wave(u64::MAX) }
}

fn rack_text() -> String {
    Preset::Rack(bp::project::Sound { rack: vec![core(1.0), effect(Kind::Motion)], lfos: Vec::new() }).to_json().write()
}

fn row_text() -> String {
    Preset::Row(effect(Kind::Delay), Vec::new()).to_json().write()
}

fn project_text() -> String {
    let mut project = Project::new();
    project.add_midi_track(2);
    project.to_json().write()
}

fn listed(app: &mut App) {
    app.listed(ROOT, vec![file("1 Rack", &rack_text()), file("2 Slap", &row_text()), file("3 Legend", &project_text()), wave("4 kick"), file("5 notes", "{\"hello\": 1}")]);
    app.take_requests();
}

fn browser_rect(fills: &[Fill]) -> Rect {
    let chevrons = fills.iter().filter(|fill| fill.tone == Tone::Ink100 && (fill.rect.w, fill.rect.h) == FOLD_OPEN && inked(fills, Rect::new(fill.rect.x + 2, fill.rect.y + 2, 10, 2)));
    let rows = chevrons.map(|fill| Rect::new(fill.rect.x - FOLD_OPEN_INSET.0, fill.rect.y - FOLD_OPEN_INSET.1, CELL, CELL));
    let (root, name) = rows
        .filter_map(|root| fills.iter().find(|fill| fill.tone == Tone::Ink100 && fill.rect.x == root.x + PITCH && fill.rect.y == root.y && fill.rect.h == FRAME).map(|name| (root, name.rect)))
        .min_by_key(|(root, _)| (root.y, root.x))
        .expect("wanted the root folder's open chevron painted beside a framed name cell");
    Rect::new(root.x, root.y, name.w + 2 * PITCH, H)
}

fn mark_cell(browser: Rect, row: i32) -> Rect {
    Rect::new(browser.x + PITCH, browser.y + row * PITCH, CELL, CELL)
}

fn name_cell(browser: Rect, row: i32) -> Rect {
    let mark = mark_cell(browser, row);
    Rect::new(mark.x + PITCH, mark.y, browser.x + browser.w - mark.x - PITCH, CELL)
}

fn interior(rect: Rect) -> Rect {
    Rect::new(rect.x + FRAME, rect.y + FRAME, rect.w - 2 * FRAME, rect.h - 2 * FRAME)
}

fn within(fills: &[Fill], rect: Rect, tone: Tone) -> Vec<(i32, i32, i32, i32)> {
    let inner = interior(rect);
    let mut found: Vec<(i32, i32, i32, i32)> = fills
        .iter()
        .filter(|fill| fill.tone == tone && fill.rect.x >= inner.x && fill.rect.y >= inner.y && fill.rect.x + fill.rect.w <= inner.x + inner.w && fill.rect.y + fill.rect.h <= inner.y + inner.h)
        .map(|fill| (fill.rect.x - rect.x, fill.rect.y - rect.y, fill.rect.w, fill.rect.h))
        .collect();
    found.sort_unstable();
    found
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

fn press(app: &mut App, key: Key) {
    app.event(Event::Key { key, shift: false, command: false, repeat: false });
}

fn opened(app: &mut App) -> Option<String> {
    app.take_requests().into_iter().find_map(|request| if let Request::Open { path } = request { path } else { None })
}

fn asking(fills: &[Fill]) -> Option<Rect> {
    fills.iter().find(|fill| fill.tone == Tone::Ground && (fill.rect.w, fill.rect.h) == ASK).map(|fill| fill.rect)
}

fn project_row(app: &mut App) -> Rect {
    name_cell(browser_rect(&painted(app)), PROJECT_ROW)
}

#[test]
fn each_kind_of_file_wears_its_own_mark() {
    let mut app = open(Some(CURRENT), &Project::new());
    listed(&mut app);
    let fills = painted(&mut app);
    let browser = browser_rect(&fills);
    let mark = |row: i32| within(&fills, mark_cell(browser, row), Tone::Ink100);
    let (rack, row_preset, project, wave, unreadable) = (mark(RACK_ROW), mark(ROW_PRESET_ROW), mark(PROJECT_ROW), mark(WAVE_ROW), mark(UNREADABLE_ROW));
    assert!(!rack.is_empty() && !project.is_empty() && !wave.is_empty(), "wanted a mark on the preset, project and wave rows, got {} {} {} fills", rack.len(), project.len(), wave.len());
    assert_eq!(rack, row_preset, "wanted a rack and a row preset to wear the same mark");
    assert_ne!(rack, project, "wanted a preset's mark unlike a project's");
    assert_ne!(project, wave, "wanted a project's mark unlike a wave's");
    assert_ne!(rack, wave, "wanted a preset's mark unlike a wave's");
    assert!(unreadable.is_empty(), "wanted no mark on a JSON that is neither preset nor project, got {unreadable:?}");
    let kind = |row: i32| within(&fills, name_cell(browser, row), Tone::Ink50).len();
    assert_eq!(kind(RACK_ROW), 0, "wanted no Rack word after a rack preset's name");
    assert!(kind(ROW_PRESET_ROW) > 0, "wanted a row preset's kind, Delay, after its name");
    assert!(kind(UNREADABLE_ROW) > 0, "wanted the unreadable JSON faded with a ? after its name");
}

#[test]
fn a_double_pressed_project_asks_before_it_opens() {
    let mut app = open(Some(CURRENT), &Project::new());
    listed(&mut app);
    let row = project_row(&mut app);
    double_click(&mut app, row);
    assert_eq!(opened(&mut app), None, "wanted nothing opened before the message is answered");
    let asked = asking(&painted(&mut app)).expect("wanted the OPEN PROJECT message painted after the double-press");
    assert_eq!(centre(asked), (WIDTH / 2, HEIGHT / 2), "wanted the message in the middle of the window");
    press(&mut app, Key::Escape);
    assert_eq!(opened(&mut app), None, "wanted Escape to keep the project you have");
    assert_eq!(asking(&painted(&mut app)), None, "wanted the message gone after Escape");
    double_click(&mut app, row);
    press(&mut app, Key::Return);
    assert_eq!(opened(&mut app), Some(under("3 Legend.json")), "wanted Return to ask the shell for the project");
    double_click(&mut app, row);
    click(&mut app, Rect::new(100, 300, 1, 1), 1);
    assert_eq!(opened(&mut app), None, "wanted a press outside the message to keep the project you have");
    assert_eq!(asking(&painted(&mut app)), None, "wanted the message gone after a press outside it");
    double_click(&mut app, row);
    let asked = asking(&painted(&mut app)).expect("wanted the message painted again");
    let buttons = Rect::new(asked.x, asked.y + asked.h - ASK_BUTTON_H, asked.w, ASK_BUTTON_H);
    click(&mut app, Rect::new(buttons.x, buttons.y, buttons.w / 2, buttons.h), 1);
    assert_eq!(opened(&mut app), None, "wanted CANCEL to keep the project you have");
    double_click(&mut app, row);
    click(&mut app, Rect::new(buttons.x + buttons.w / 2, buttons.y, buttons.w / 2, buttons.h), 1);
    assert_eq!(opened(&mut app), Some(under("3 Legend.json")), "wanted OPEN to ask the shell for the project");
}

#[test]
fn a_pristine_project_opens_without_asking() {
    let mut app = open(None, &Project::new());
    listed(&mut app);
    let row = project_row(&mut app);
    double_click(&mut app, row);
    assert_eq!(asking(&painted(&mut app)), None, "wanted no message when there is nothing to lose");
    assert_eq!(opened(&mut app), Some(under("3 Legend.json")), "wanted an untouched new project replaced at once");
}

#[test]
fn a_file_opens_by_what_it_holds() {
    let mut app = open(Some(CURRENT), &Project::new());
    assert_eq!(app.opened(String::from("/x/notes.json"), Some("{\"hello\": 1}"), None), Opened::Refused, "wanted a JSON with neither kind nor tracks refused");
    assert_eq!(app.opened(String::from("/x/rack.json"), Some(&rack_text()), None), Opened::Preset, "wanted a JSON with a kind loaded as a preset");
    assert_eq!(app.opened(String::from("/x/song.json"), Some(&project_text()), None), Opened::Project, "wanted a JSON with tracks opened as a project");
}
