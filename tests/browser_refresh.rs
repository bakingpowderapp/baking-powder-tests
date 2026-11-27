use bp::app::{App, Event, Request};
use bp::browser::{Action, Browser, Found, Item, Under};
use bp::draw::{DrawList, Fill, Rect, Tone};
use bp::face::{CELL, FRAME, H, PITCH};
use bp::keyboard::Key;
use bp::project::Project;
use support::{browser_rect, inked, painted};

const W: i32 = 400;
const NARROWEST: i32 = 5 * PITCH;
const ROOT: &str = "/x/Sounds";
const OTHER: &str = "/x/Other";
const KITS: &str = "/x/Sounds/Kits";
const LOOPS: &str = "/x/Sounds/Loops";
const DEEP: &str = "/x/Sounds/Kits/Deep";
const KICK: &str = "/x/Sounds/Kits/kick.wav";
const SNARE: &str = "/x/Sounds/Kits/snare.wav";
const TOM: &str = "/x/Sounds/Kits/tom.wav";
const SUB: &str = "/x/Sounds/Kits/Deep/sub.wav";
const BREAK: &str = "/x/Sounds/Loops/break.wav";
const SOUNDS: usize = 40;
const BAR_Y: i32 = CELL / 2;
const SEARCH_X: i32 = 3 * PITCH;
const HEARTS_SQUARE: i32 = 0;
const BULLS_EYE_SQUARE: i32 = 2;
const REFRESH_SQUARE: i32 = 4;
const KITS_ROW: i32 = 2;
const DEEP_ROW: i32 = 3;
const FIRST_SOUND: i32 = 4;

fn found(path: &str, item: Item) -> Found {
    Found { path: String::from(path), name: String::from(path.rsplit('/').next().unwrap_or(path)), item }
}

fn wave(path: &str) -> Found {
    found(path, Item::Wave(u64::MAX))
}

fn folder(path: &str) -> Found {
    found(path, Item::Folder)
}

fn sound(name: &str) -> String {
    format!("{ROOT}/{name}.wav")
}

fn numbered() -> impl Iterator<Item = String> {
    (0..SOUNDS).map(|at| format!("s{at:02}"))
}

fn waves(names: impl IntoIterator<Item = String>) -> Vec<Found> {
    names.into_iter().map(|name| wave(&sound(&name))).collect()
}

fn in_root(names: impl IntoIterator<Item = String>) -> Vec<Found> {
    [folder(KITS), folder(LOOPS)].into_iter().chain(waves(names)).collect()
}

fn changed() -> impl Iterator<Item = String> {
    [String::from("s00"), String::from("s00b")].into_iter().chain(numbered().skip(2))
}

fn shelf() -> Browser {
    let mut browser = Browser::new();
    browser.layout(W, H);
    browser.resume(vec![String::from(ROOT)], vec![String::from(ROOT)], Vec::new());
    browser.listed(ROOT, in_root(numbered()));
    browser
}

fn row_y(row: i32) -> i32 {
    row * PITCH + CELL / 2
}

fn named(browser: &mut Browser, row: i32) -> Option<String> {
    browser.hover(Some((W / 2, row_y(row))));
    let name = match browser.under()? {
        Under::Wave { name, .. } | Under::Folder { name, .. } | Under::Midi { name, .. } | Under::Unreadable { name } | Under::Project { name, .. } | Under::Preset { name, .. } => Some(String::from(name)),
        _ => None,
    };
    browser.hover(None);
    name
}

fn rows(browser: &mut Browser, count: i32) -> Vec<String> {
    (1..=count).filter_map(|row| named(browser, row)).collect()
}

fn pressed(browser: &mut Browser, x: i32, y: i32) -> Option<Action> {
    let (_, down) = browser.down(x, y, 1);
    let (_, up) = browser.up(x, y, false);
    down.or(up)
}

fn fold(browser: &mut Browser, row: i32, depth: i32) -> Option<Action> {
    pressed(browser, depth * PITCH + CELL / 2, row_y(row))
}

fn lists(action: Option<Action>, folder: &str) -> bool {
    matches!(action, Some(Action::List(path)) if path == folder)
}

fn bar_cell(browser: &mut Browser, wanted: fn(&Under) -> bool) -> Rect {
    let xs: Vec<i32> = (0..W).filter(|&x| {
        browser.hover(Some((x, BAR_Y)));
        browser.under().is_some_and(|under| wanted(&under))
    }).collect();
    browser.hover(None);
    Rect::new(*xs.first().expect("wanted the box in the browser's top row"), 0, CELL, CELL)
}

fn top_row(browser: &mut Browser, w: i32) -> (Vec<&'static str>, i32) {
    browser.layout(w, H);
    let (mut boxes, mut search) = (Vec::new(), 0);
    for x in 0..w {
        browser.hover(Some((x, BAR_Y)));
        let name = match browser.under() {
            Some(Under::Search { .. }) => {
                search += 1;
                continue;
            }
            Some(Under::Refresh) => "refresh",
            Some(Under::Big) => "BIG",
            Some(Under::Locate) => "bull's-eye",
            _ => continue,
        };
        if boxes.last() != Some(&name) {
            boxes.push(name);
        }
    }
    browser.hover(None);
    (boxes, search)
}

fn search_text(browser: &mut Browser) -> String {
    browser.hover(Some((SEARCH_X, BAR_Y)));
    let text = match browser.under() {
        Some(Under::Search { text, .. }) => String::from(text),
        _ => panic!("wanted the search box under the pointer"),
    };
    browser.hover(None);
    text
}

fn interior(cell: Rect) -> Rect {
    Rect::new(cell.x + FRAME, cell.y + FRAME, cell.w - 2 * FRAME, cell.h - 2 * FRAME)
}

fn marked(fills: &[Fill], cell: Rect) -> usize {
    let inner = interior(cell);
    fills.iter().filter(|fill| fill.rect != inner && inner.intersect(fill.rect) == fill.rect && fill.rect.w > 0 && fill.rect.h > 0).count()
}

fn holding() -> Browser {
    let mut browser = shelf();
    fold(&mut browser, KITS_ROW, 1);
    browser.listed(KITS, vec![folder(DEEP), wave(KICK)]);
    fold(&mut browser, DEEP_ROW, 2);
    browser.listed(DEEP, vec![wave(SUB)]);
    assert_eq!(rows(&mut browser, 6), ["Sounds", "Kits", "Deep", "sub", "kick", "Loops"], "wanted the folder open over an open folder of its own and a sound");
    assert!(fold(&mut browser, KITS_ROW, 1).is_none(), "wanted the folder closed with nothing asked");
    browser
}

fn opened() -> (App, Rect) {
    let session = format!("{{\"project\":{},\"aside\":\"browser\",\"keyboard\":false,\"folders\":[\"{ROOT}\",\"{OTHER}\"],\"unfolded\":[\"{ROOT}\",\"{LOOPS}\"]}}", Project::new().to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.listed(ROOT, in_root(numbered()));
    app.listed(LOOPS, vec![wave(BREAK)]);
    let panel = browser_rect(&painted(&mut app));
    app.take_requests();
    (app, panel)
}

fn top_box(panel: Rect, back: i32) -> Rect {
    Rect::new(panel.x + panel.w - CELL - back * PITCH, panel.y - PITCH, CELL, CELL)
}

fn name_at(panel: Rect, row: i32) -> (i32, i32) {
    (panel.x + panel.w / 2, panel.y + row * PITCH + CELL / 2)
}

fn click(app: &mut App, (x, y): (i32, i32)) -> Vec<Request> {
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    painted(app);
    app.take_requests().into_iter().filter(|request| !matches!(request, Request::Remember { .. })).collect()
}

fn refresh(app: &mut App, panel: Rect) -> Vec<Request> {
    let square = top_box(panel, REFRESH_SQUARE);
    click(app, (square.x + CELL / 2, square.y + CELL / 2))
}

fn heard_on(app: &mut App, panel: Rect, row: i32) -> Vec<String> {
    let heard = click(app, name_at(panel, row)).into_iter().filter_map(|request| if let Request::Read { path } = request { Some(path) } else { None }).collect();
    click(app, name_at(panel, row));
    heard
}

fn previewing(app: &mut App, panel: Rect, row: i32) -> (bool, bool) {
    let fills = painted(app);
    (inked(&fills, interior(Rect::new(panel.x + PITCH, panel.y + row * PITCH, CELL, CELL))), marked(&fills, top_box(panel, BULLS_EYE_SQUARE)) > 0)
}

fn type_word(app: &mut App, word: &str) {
    for letter in word.chars() {
        app.event(Event::Key { key: Key::Char(letter), shift: false, command: false, repeat: false });
    }
}

fn list(path: &str) -> Request {
    Request::List { path: String::from(path) }
}

fn scan(path: &str) -> Request {
    Request::Scan { path: String::from(path) }
}

fn hearts(folder: &str) -> Request {
    Request::Read { path: hearts_file(folder) }
}

fn hearts_file(folder: &str) -> String {
    format!("{folder}/Hearts.json")
}

fn hearted(files: &[&str]) -> Option<Vec<u8>> {
    let quoted: Vec<String> = files.iter().map(|file| format!("\"{file}\"")).collect();
    Some(format!("{{\"hearts\":[{}]}}", quoted.join(",")).into_bytes())
}

#[test]
fn refresh_stands_left_of_big_in_the_top_row_and_a_press_asks_for_it() {
    let mut browser = shelf();
    let refresh = bar_cell(&mut browser, |under| matches!(under, Under::Refresh));
    let big = bar_cell(&mut browser, |under| matches!(under, Under::Big));
    assert_eq!((refresh.x, big.x), (W - CELL - REFRESH_SQUARE * PITCH, W - CELL - (REFRESH_SQUARE - 1) * PITCH), "wanted refresh the fifth box from the right of the top row, next left of BIG");
    assert!(matches!(pressed(&mut browser, refresh.x + refresh.w / 2, BAR_Y), Some(Action::Refresh)), "wanted a press on the refresh square to ask for the folders to be read again");
    browser.hover(None);
    let mut list = DrawList::new();
    browser.draw(&mut list, None, false);
    assert!(marked(list.fills(), refresh) > 0, "wanted the refresh square to draw its mark");
    assert!(!list.fills().iter().any(|fill| fill.rect == interior(refresh) && fill.tone == Tone::Ink100), "wanted the refresh square unlit, a box to press and not a switch");
}

#[test]
fn a_narrow_panel_leaves_out_refresh_then_the_bulls_eye_then_big_and_gives_the_search_box_their_room() {
    let mut browser = shelf();
    let seen: Vec<(Vec<&str>, i32)> = (NARROWEST..=W).rev().map(|w| top_row(&mut browser, w)).collect();
    let mut stages: Vec<Vec<&str>> = seen.iter().map(|(boxes, _)| boxes.clone()).collect();
    stages.dedup();
    assert_eq!(stages, [vec!["refresh", "BIG", "bull's-eye"], vec!["BIG", "bull's-eye"], vec!["BIG"], vec![]], "wanted the top row to give up refresh first, then the bull's-eye, then BIG, as the panel narrows from {W} to {NARROWEST}");
    for pair in seen.windows(2) {
        let ((wider, room), (narrower, search)) = (&pair[0], &pair[1]);
        assert!(wider == narrower || search > room, "wanted the search box wider once {wider:?} became {narrower:?}, got {search} after {room}");
    }
}

#[test]
fn a_refresh_shows_the_folders_as_they_now_are_and_keeps_open_folders_scroll_and_selection() {
    let mut browser = shelf();
    assert!(lists(fold(&mut browser, KITS_ROW, 1), KITS), "wanted the folder opened and asked to be listed");
    browser.listed(KITS, vec![wave(KICK), wave(SNARE)]);
    browser.scroll(-2 * PITCH);
    pressed(&mut browser, W / 2, row_y(6));
    assert_eq!(rows(&mut browser, 7), ["kick", "snare", "Loops", "s00", "s01", "s02", "s03"], "wanted the list scrolled two rows down, the open folder's sounds at its top");
    assert_eq!(browser.renew(), [ROOT], "wanted a refresh to name every top-level folder to be read again");
    assert_eq!(browser.unfolded(), [ROOT, KITS], "wanted the open folders still open, each to be read again, and the closed one left closed");
    assert_eq!(rows(&mut browser, 7), ["kick", "snare", "Loops", "s00", "s01", "s02", "s03"], "wanted the list to stand as it was until the folders are read");
    browser.listed(ROOT, in_root(changed()));
    browser.listed(KITS, vec![wave(KICK), wave(TOM)]);
    assert_eq!(rows(&mut browser, 7), ["kick", "tom", "Loops", "s00", "s00b", "s02", "s03"], "wanted the renamed, added and removed sounds shown as they now are, the list still scrolled two rows down");
    assert!(browser.has_selection(), "wanted the selected sound still selected");
    let (_, stepped) = browser.step(Key::Down);
    assert!(matches!(&stepped, Some(Action::Preview { path, .. }) if *path == sound("s03")), "wanted the arrow to step on from the sound that was selected before the refresh");
}

#[test]
fn a_closed_folder_drops_what_it_held_at_a_refresh_and_reads_when_opened() {
    let mut kept = holding();
    assert!(lists(fold(&mut kept, KITS_ROW, 1), KITS), "wanted the folder asked to be listed when it is opened again");
    assert_eq!(rows(&mut kept, 6), ["Sounds", "Kits", "Deep", "sub", "kick", "Loops"], "wanted a folder opened again with no refresh between to show what it held while it is read");
    let mut browser = holding();
    browser.renew();
    assert!(lists(fold(&mut browser, KITS_ROW, 1), KITS), "wanted the closed folder read when it is opened after a refresh");
    assert_eq!((rows(&mut browser, 3), browser.unlisted()), (vec![String::from("Sounds"), String::from("Kits"), String::from("Loops")], vec![String::from(KITS)]), "wanted the folder to have dropped what it held, empty until its list arrives");
    browser.listed(KITS, vec![folder(DEEP), wave(SNARE)]);
    assert_eq!(browser.unlisted(), [DEEP], "wanted the folder that stood open inside it open again and waiting for its own list");
    browser.listed(DEEP, vec![wave(SUB)]);
    assert_eq!(rows(&mut browser, 6), ["Sounds", "Kits", "Deep", "sub", "snare", "Loops"], "wanted the folder shown as it now is on disk");
}

#[test]
fn a_refresh_scans_the_folders_of_a_search_in_force_again_and_keeps_its_word() {
    let mut browser = shelf();
    pressed(&mut browser, SEARCH_X, BAR_Y);
    assert_eq!(browser.scans(), [ROOT], "wanted the folder scanned when the search is first pressed");
    browser.scanned(ROOT, waves(numbered()));
    for letter in "s3".chars() {
        browser.type_key(Key::Char(letter), false, false);
    }
    assert_eq!(rows(&mut browser, 2), ["s30", "s31"], "wanted the search to list the sounds it names");
    assert!(browser.scans().is_empty(), "wanted a scanned folder left alone while the search stands");
    browser.renew();
    assert_eq!(browser.scans(), [ROOT], "wanted a refresh to scan the search's folders again");
    browser.scanned(ROOT, waves(numbered().filter(|name| name != "s30").chain([String::from("s3x")])));
    assert_eq!((search_text(&mut browser), rows(&mut browser, 2)), (String::from("s3"), vec![String::from("s31"), String::from("s32")]), "wanted the search's word kept and the sound that is gone left out of what it finds");
    assert!(rows(&mut browser, H / PITCH - 1).contains(&String::from("s3x")), "wanted the sound added elsewhere found by the search");
}

#[test]
fn a_press_on_refresh_reads_every_top_level_folder_every_open_folder_beneath_and_the_hearts_files() {
    let (mut app, panel) = opened();
    assert_eq!(refresh(&mut app, panel), vec![list(OTHER), list(ROOT), list(LOOPS), hearts(OTHER), hearts(ROOT)], "wanted both top-level folders and the open folder beneath read again, the closed folder beneath left alone, and each folder's hearts read");
    app.listed(OTHER, Vec::new());
    app.listed(ROOT, in_root(changed()));
    app.listed(LOOPS, vec![wave(BREAK)]);
    assert!(app.take_requests().iter().all(|request| matches!(request, Request::Remember { .. })), "wanted nothing more asked once the folders are read");
    assert_eq!((heard_on(&mut app, panel, FIRST_SOUND), heard_on(&mut app, panel, FIRST_SOUND + 1), heard_on(&mut app, panel, FIRST_SOUND + 2)), (vec![sound("s00")], vec![sound("s00b")], vec![sound("s02")]), "wanted the sound added elsewhere on its row and the one removed gone");
}

#[test]
fn with_a_search_in_force_a_press_on_refresh_scans_the_top_level_folders_again() {
    let (mut app, panel) = opened();
    assert_eq!(click(&mut app, (panel.x + SEARCH_X, panel.y - PITCH + CELL / 2)), vec![scan(ROOT), scan(OTHER)], "wanted each top-level folder scanned when the search is first pressed");
    app.scanned(String::from(ROOT), waves(numbered()));
    app.scanned(String::from(OTHER), Vec::new());
    type_word(&mut app, "s3");
    painted(&mut app);
    assert_eq!(heard_on(&mut app, panel, 0), [sound("s30")], "wanted the search's first find on the first row");
    assert_eq!(refresh(&mut app, panel), vec![list(OTHER), list(ROOT), list(LOOPS), scan(ROOT), scan(OTHER), hearts(OTHER), hearts(ROOT)], "wanted the folders read again and each top-level folder scanned again for the search");
    app.scanned(String::from(ROOT), waves(numbered().filter(|name| name != "s30")));
    app.scanned(String::from(OTHER), Vec::new());
    painted(&mut app);
    assert_eq!(heard_on(&mut app, panel, 0), [sound("s31")], "wanted the search still in force and the sound that is gone left out of what it finds");
}

#[test]
fn a_preview_whose_file_is_gone_stops_at_a_refresh_and_one_whose_file_remains_plays_on() {
    let (mut app, panel) = opened();
    let heard = click(&mut app, name_at(panel, FIRST_SOUND));
    assert_eq!(heard, vec![Request::Read { path: sound("s00") }], "wanted a press on the sound's row to preview it");
    assert_eq!(previewing(&mut app, panel, FIRST_SOUND), (true, true), "wanted the previewing sound's mark lit and the bull's-eye drawn");
    assert!(refresh(&mut app, panel).contains(&list(ROOT)), "wanted the previewing sound's folder read again");
    app.listed(ROOT, in_root(numbered()));
    assert_eq!(previewing(&mut app, panel, FIRST_SOUND), (true, true), "wanted a preview whose file is still there to play on through the refresh");
    refresh(&mut app, panel);
    app.listed(ROOT, in_root(numbered().skip(1)));
    assert_eq!(previewing(&mut app, panel, FIRST_SOUND), (false, false), "wanted the preview stopped once its file is gone from its folder, no mark lit and the bull's-eye blank");
}

#[test]
fn a_preview_from_a_closed_folder_has_its_folder_read_at_a_refresh_and_stops_if_its_file_is_gone() {
    let (mut app, panel) = opened();
    click(&mut app, (panel.x + SEARCH_X, panel.y - PITCH + CELL / 2));
    app.scanned(String::from(ROOT), vec![wave(KICK), wave(SNARE)]);
    app.scanned(String::from(OTHER), Vec::new());
    type_word(&mut app, "kick");
    painted(&mut app);
    assert_eq!(click(&mut app, name_at(panel, 0)), vec![Request::Read { path: String::from(KICK) }], "wanted the sound found in the closed folder previewed on a press");
    assert!(marked(&painted(&mut app), top_box(panel, BULLS_EYE_SQUARE)) > 0, "wanted the bull's-eye drawn while the sound previews");
    assert_eq!(refresh(&mut app, panel), vec![list(OTHER), list(ROOT), list(KITS), list(LOOPS), scan(ROOT), scan(OTHER), hearts(OTHER), hearts(ROOT)], "wanted the closed folder of the previewing sound read with the rest");
    app.listed(KITS, vec![wave(SNARE)]);
    let fills = painted(&mut app);
    assert_eq!(marked(&fills, top_box(panel, BULLS_EYE_SQUARE)), 0, "wanted the preview stopped once its folder is read without its file, the bull's-eye blank");
}

#[test]
fn with_the_hearts_showing_a_refresh_reads_their_folders_and_a_file_hearted_elsewhere_joins_them() {
    let (mut app, panel) = opened();
    app.read(hearts_file(ROOT), hearted(&["Kits/kick.wav"]));
    let heart = top_box(panel, HEARTS_SQUARE);
    assert_eq!(click(&mut app, (heart.x + CELL / 2, heart.y + CELL / 2)), vec![list(KITS)], "wanted the hearted sound's folder read when the hearts are shown");
    app.listed(KITS, vec![wave(KICK), wave(SNARE)]);
    painted(&mut app);
    assert_eq!(heard_on(&mut app, panel, 0), [KICK], "wanted the hearted sound on the first row");
    assert_eq!(refresh(&mut app, panel), vec![list(OTHER), list(ROOT), list(KITS), list(LOOPS), hearts(OTHER), hearts(ROOT)], "wanted the hearted sound's closed folder read with the rest, and the hearts files read again");
    app.listed(LOOPS, vec![wave(BREAK)]);
    app.read(hearts_file(ROOT), hearted(&["Kits/kick.wav", "Loops/break.wav"]));
    let asked: Vec<Request> = app.take_requests().into_iter().filter(|request| !matches!(request, Request::Remember { .. })).collect();
    assert_eq!(asked, vec![list(LOOPS)], "wanted the folder of the sound hearted elsewhere read again once the hearts file names it");
    app.listed(LOOPS, vec![wave(BREAK)]);
    painted(&mut app);
    assert_eq!((heard_on(&mut app, panel, 0), heard_on(&mut app, panel, 1)), (vec![String::from(BREAK)], vec![String::from(KICK)]), "wanted the sound hearted elsewhere among the hearts, above the one already there");
}
