use bp::browser::{Action, Browser, Found, Item, Under};
use bp::draw::{DrawList, Rect, Tone};
use bp::face::{CELL, FRAME, H, PITCH};
use bp::keyboard::Key;

const W: i32 = 400;
const ROOT: &str = "/x/Sounds";
const KITS: &str = "/x/Sounds/Kits";
const SOUNDS: usize = 40;
const LIST_ROWS: i32 = H / PITCH - 1;
const MIDDLE: i32 = 1 + (LIST_ROWS - 1) / 2;
const FAR: usize = 30;
const FIRST_SOUND_ROW: i32 = 3;
const BAR_Y: i32 = CELL / 2;
const SEARCH_X: i32 = 3 * PITCH;
const LIFT: i32 = 60;
const BRACKET: i32 = 5;
const BRACKET_INSET: i32 = 2;

fn shown_as(at: usize) -> String {
    format!("s{at:02}")
}

fn sound(at: usize) -> String {
    format!("{ROOT}/{}.wav", shown_as(at))
}

fn wave(path: String) -> Found {
    let name = String::from(path.rsplit('/').next().unwrap_or(&path));
    Found { path, name, item: Item::Wave(u64::MAX) }
}

fn shelf() -> Browser {
    let mut browser = Browser::new();
    browser.layout(W, H);
    browser.resume(vec![String::from(ROOT)], vec![String::from(ROOT)], Vec::new());
    let kits = Found { path: String::from(KITS), name: String::from("Kits"), item: Item::Folder };
    browser.listed(ROOT, std::iter::once(kits).chain((0..SOUNDS).map(|at| wave(sound(at)))).collect());
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

fn shown_at(browser: &mut Browser, name: &str) -> Option<i32> {
    (1..=LIST_ROWS).find(|&row| named(browser, row).as_deref() == Some(name))
}

fn pressed(browser: &mut Browser, x: i32, y: i32) -> Option<Action> {
    let (_, down) = browser.down(x, y, 1);
    let (_, up) = browser.up(x, y, false);
    down.or(up)
}

fn bar_cell(browser: &mut Browser, wanted: fn(&Under) -> bool) -> Rect {
    let xs: Vec<i32> = (0..W).filter(|&x| {
        browser.hover(Some((x, BAR_Y)));
        browser.under().is_some_and(|under| wanted(&under))
    }).collect();
    browser.hover(None);
    Rect::new(*xs.first().expect("wanted the box in the browser's top row"), 0, CELL, CELL)
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

fn searching(browser: &mut Browser, word: &str) {
    browser.down(SEARCH_X, BAR_Y, 1);
    browser.up(SEARCH_X, BAR_Y, false);
    for root in browser.scans() {
        browser.scanned(&root, (0..SOUNDS).map(|at| wave(sound(at))).collect());
    }
    for letter in word.chars() {
        browser.type_key(Key::Char(letter), false, false);
    }
    browser.type_key(Key::Return, false, false);
}

fn painted(browser: &Browser, playing: Option<&str>, big: bool) -> DrawList {
    let mut list = DrawList::new();
    browser.draw(&mut list, playing, big);
    list
}

fn lit(list: &DrawList, cell: Rect) -> bool {
    let inner = Rect::new(cell.x + FRAME, cell.y + FRAME, cell.w - 2 * FRAME, cell.h - 2 * FRAME);
    list.fills().iter().any(|fill| fill.rect == inner && fill.tone == Tone::Ink100)
}

fn marked(list: &DrawList, cell: Rect) -> usize {
    let inner = Rect::new(cell.x + FRAME, cell.y + FRAME, cell.w - 2 * FRAME, cell.h - 2 * FRAME);
    list.fills().iter().filter(|fill| fill.rect != inner && inner.intersect(fill.rect) == fill.rect && fill.rect.w > 0 && fill.rect.h > 0).count()
}

#[test]
fn the_bulls_eye_brings_a_sounds_row_to_the_middle_of_the_list_and_leaves_the_selection() {
    let mut browser = shelf();
    assert_eq!((shown_at(&mut browser, &shown_as(0)), shown_at(&mut browser, &shown_as(FAR))), (Some(FIRST_SOUND_ROW), None), "wanted the first sound under the root and its folder, and the far sound out of sight, before it is brought into view");
    let lists = browser.locate(&sound(FAR));
    assert!(lists.is_empty(), "wanted no folder to list for a sound already in the list, got {lists:?}");
    assert_eq!(shown_at(&mut browser, &shown_as(FAR)), Some(MIDDLE), "wanted the sound's row brought to the middle of the list");
    assert!(!browser.has_selection(), "wanted nothing selected by bringing a row into view");
    let mut browser = shelf();
    pressed(&mut browser, W / 2, row_y(FIRST_SOUND_ROW));
    assert!(browser.has_selection(), "wanted the first sound selected by a press on its name");
    browser.locate(&sound(FAR));
    assert_eq!(shown_at(&mut browser, &shown_as(FAR)), Some(MIDDLE), "wanted the far sound's row in the middle with another row selected");
    let (_, stepped) = browser.step(Key::Down);
    assert!(matches!(&stepped, Some(Action::Preview { path, .. }) if *path == sound(1)), "wanted the arrow to step on from the row that was selected, the first sound, and not from the row brought into view");
}

#[test]
fn the_bulls_eye_opens_the_folder_above_a_sound_and_shows_its_row_once_the_folder_is_listed() {
    let mut browser = shelf();
    let kick = format!("{KITS}/kick.wav");
    let lists = browser.locate(&kick);
    assert_eq!(lists, [KITS], "wanted the closed folder above the sound opened and asked to be listed");
    assert_eq!(shown_at(&mut browser, "kick"), None, "wanted the sound's row to wait for its folder's list");
    browser.listed(KITS, vec![wave(kick.clone()), wave(format!("{KITS}/snare.wav"))]);
    assert_eq!(shown_at(&mut browser, "kick"), Some(FIRST_SOUND_ROW), "wanted the sound's row in view under its folder once the folder is listed");
    browser.hover(Some((W / 2, row_y(2))));
    assert!(matches!(browser.under(), Some(Under::Folder { name: "Kits", open: true, .. })), "wanted the folder above the sound left open");
}

#[test]
fn the_bulls_eye_clears_a_search_only_when_it_hides_the_sound() {
    let mut browser = shelf();
    searching(&mut browser, "s3");
    assert_eq!((search_text(&mut browser), shown_at(&mut browser, &shown_as(5)), shown_at(&mut browser, &shown_as(31))), (String::from("s3"), None, Some(2)), "wanted the search to list the sounds it names and hide the rest");
    browser.locate(&sound(31));
    assert_eq!(search_text(&mut browser), "s3", "wanted the search kept for a sound it shows");
    assert!(shown_at(&mut browser, &shown_as(31)).is_some(), "wanted the sound's row in view under the search");
    browser.locate(&sound(5));
    assert_eq!(search_text(&mut browser), "", "wanted the search cleared for a sound it hid");
    assert!(shown_at(&mut browser, &shown_as(5)).is_some(), "wanted the sound's row in view once the search is cleared");
}

#[test]
fn the_bulls_eye_is_blank_with_nothing_previewing_and_the_previewing_row_shows_its_mark_lit() {
    let mut browser = shelf();
    let target = bar_cell(&mut browser, |under| matches!(under, Under::Locate));
    let playing = sound(0);
    let (quiet, heard) = (painted(&browser, None, false), painted(&browser, Some(&playing), false));
    assert_eq!(marked(&quiet, target), 0, "wanted the bull's-eye box blank with nothing previewing");
    assert!(marked(&heard, target) > 0, "wanted the bull's-eye drawn while a sound previews");
    let mark = Rect::new(PITCH, FIRST_SOUND_ROW * PITCH, CELL, CELL);
    let next = Rect::new(PITCH, (FIRST_SOUND_ROW + 1) * PITCH, CELL, CELL);
    assert!(lit(&heard, mark) && !lit(&heard, next), "wanted the previewing sound's mark lit and its neighbour's not");
    assert!(!lit(&quiet, mark), "wanted no mark lit with nothing previewing");
    assert!(matches!(pressed(&mut browser, target.x + target.w / 2, BAR_Y), Some(Action::Locate)), "wanted a press on the bull's-eye to ask for the previewing sound");
}

#[test]
fn big_is_a_plain_word_box_lit_when_on_and_a_press_asks_for_it() {
    let mut browser = shelf();
    let big = bar_cell(&mut browser, |under| matches!(under, Under::Big));
    assert_eq!(big.x, W - CELL - 3 * PITCH, "wanted BIG the fourth box from the right of the top row");
    assert!(matches!(pressed(&mut browser, big.x + big.w / 2, BAR_Y), Some(Action::Big)), "wanted a press on BIG to ask for the big browser");
    let (small, grown) = (painted(&browser, None, false), painted(&browser, None, true));
    assert!(!lit(&small, big) && lit(&grown, big), "wanted BIG lit only while the browser is big");
    let collapse = bar_cell(&mut browser, |under| matches!(under, Under::Collapse));
    assert_eq!(collapse.x - big.x, 2 * PITCH, "wanted BIG two boxes left of collapse-all, the bull's-eye between them");
    let bracket = Rect::new(big.x + FRAME + BRACKET_INSET, FRAME + BRACKET_INSET, BRACKET, 1);
    let bracketed = |list: &DrawList| list.fills().iter().any(|fill| fill.rect == bracket);
    assert!(!bracketed(&small) && !bracketed(&grown), "wanted BIG drawn without corner brackets, a plain word box like its neighbours");
}

#[test]
fn a_taller_browser_shows_as_many_rows_as_fit() {
    let mut browser = shelf();
    assert_eq!(named(&mut browser, LIST_ROWS + 1), None, "wanted no row under the last the panel fits");
    browser.layout(W, 2 * H);
    let last = 2 * H / PITCH - 1;
    assert!(named(&mut browser, LIST_ROWS + 1).is_some() && named(&mut browser, last).is_some(), "wanted the rows the taller browser fits listed down to row {last}");
    browser.locate(&sound(SOUNDS - 1));
    assert_eq!(shown_at_any(&mut browser, &shown_as(SOUNDS - 1), last), Some(last - 1), "wanted the last sound brought as near the middle as the list's end allows, the ADD FOLDER row under it");
    browser.layout(W, 4 * H);
    browser.locate(&sound(FAR));
    assert_eq!(shown_at_any(&mut browser, &shown_as(FAR), 4 * H / PITCH - 1), Some(FAR as i32 + FIRST_SOUND_ROW), "wanted a list that fits whole to stay where it is");
}

fn shown_at_any(browser: &mut Browser, name: &str, rows: i32) -> Option<i32> {
    (1..=rows).find(|&row| named(browser, row).as_deref() == Some(name))
}

#[test]
fn a_press_and_move_on_a_file_lifts_it_only_where_the_browser_lets_files_go() {
    let mut browser = shelf();
    let (x, y) = (W / 2, row_y(FIRST_SOUND_ROW));
    browser.down(x, y, 1);
    browser.moved(x + LIFT, y + LIFT, true);
    assert!(browser.lifted().is_some(), "wanted a file pressed and moved in the panel lifted for a drag");
    browser.up(x + LIFT, y + LIFT, false);
    browser.down(x, y, 1);
    browser.moved(x + LIFT, y + LIFT, false);
    assert!(browser.lifted().is_none() && browser.carry().is_none(), "wanted a file pressed and moved in the big browser left where it is, no drag begun");
    browser.up(x + LIFT, y + LIFT, false);
    assert!(browser.has_selection(), "wanted the pressed row still selected after the move");
}
