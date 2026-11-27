use bp::app::{App, Event, Request};
use bp::keyboard::Key;
use bp::browser::{Found, Item};
use bp::draw::{Fill, Rect, Tone};
use bp::face::{CELL, FRAME, PITCH};
use bp::json::{self, Json};
use bp::project::{BEATS_PER_BAR, Project};
use support::{BLOCK, RATE, SAMPLE_RATE, browser_rect, chevrons, painted, wav};

const MAX_FRAMES: usize = 512;
const ROOT: &str = "/x/Samples";
const KICK: i16 = 16_384;
const SNARE: i16 = 8_192;
const NEAR: f32 = 1e-3;

fn wave_of(path: &str) -> Vec<u8> {
    let level = if path.ends_with("snare.wav") { SNARE } else { KICK };
    wav(&vec![level; (RATE / 4) as usize])
}

fn level(pcm: i16) -> f32 {
    f32::from(pcm) / 32_768.0
}

fn folder(path: &str) -> Found {
    Found { path: format!("{ROOT}/{path}"), name: String::from(path.rsplit('/').next().unwrap_or(path)), item: Item::Folder }
}

fn wave(path: &str) -> Found {
    Found { path: format!("{ROOT}/{path}"), name: String::from(path.rsplit('/').next().unwrap_or(path)), item: Item::Wave(u64::MAX) }
}

fn preset(path: &str) -> Found {
    Found { path: format!("{ROOT}/{path}"), name: String::from(path.rsplit('/').next().unwrap_or(path)), item: Item::read("{}") }
}

fn open() -> App {
    let session = format!("{{\"project\":{},\"aside\":\"browser\",\"keyboard\":false,\"folders\":[\"{ROOT}\"],\"unfolded\":[\"{ROOT}\"]}}", Project::new().to_json().write());
    let mut app = App::new();
    app.resume(&session);
    app.take_requests();
    app.listed(ROOT, vec![folder("A1"), wave("kick.wav"), wave("snare.wav"), preset("tone.json")]);
    app.take_requests();
    app
}

struct Served {
    reads: usize,
    lists: Vec<String>,
    play_from: i32,
}

fn serve(app: &mut App) -> Served {
    let mut served = Served { reads: 0, lists: Vec::new(), play_from: 0 };
    loop {
        let requests = app.take_requests();
        if requests.is_empty() {
            return served;
        }
        for request in requests {
            match request {
                Request::Read { path } => {
                    served.reads += 1;
                    let bytes = wave_of(&path);
                    app.read(path, Some(bytes));
                }
                Request::List { path } => {
                    served.lists.push(path.clone());
                    if path == format!("{ROOT}/A1") {
                        app.listed(&path, vec![wave("A1/a.wav"), wave("A1/b.wav")]);
                    }
                }
                Request::Remember { json } => served.play_from = json::parse(&json).and_then(|session| session.get("playheadBeat").and_then(Json::as_i64)).unwrap_or(0) as i32,
                _ => {}
            }
        }
    }
}

fn name_cell(browser: Rect, depth: i32, row: i32) -> Rect {
    let mark = Rect::new(browser.x + depth * PITCH, browser.y + row * PITCH, CELL, CELL);
    Rect::new(mark.x + PITCH, mark.y, browser.x + browser.w - mark.x - PITCH, CELL)
}

fn selected_row(fills: &[Fill], browser: Rect) -> Option<i32> {
    fills.iter().find(|fill| fill.tone == Tone::Ink100 && fill.rect.h == CELL - 2 * FRAME && fill.rect.w > 4 * CELL && fill.rect.x > browser.x && fill.rect.x < browser.x + browser.w && fill.rect.y >= browser.y && fill.rect.y < browser.y + browser.h).map(|fill| (fill.rect.y - FRAME - browser.y) / PITCH)
}

fn folder_open(fills: &[Fill], index: usize) -> Option<bool> {
    chevrons(fills).get(index).map(|(_, open)| *open)
}

fn click(app: &mut App, rect: Rect) {
    let (x, y) = (rect.x + rect.w / 2, rect.y + rect.h / 2);
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
}

fn key(app: &mut App, key: Key) {
    app.event(Event::Key { key, shift: false, command: false, repeat: false });
    app.event(Event::KeyUp { key });
}

fn heard(renderer: &mut bp::audio::Renderer) -> f32 {
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    for _ in 0..2 {
        renderer.render(&mut l, &mut r);
    }
    l[BLOCK - 1].abs()
}

fn sounds_as(renderer: &mut bp::audio::Renderer, pcm: i16) -> bool {
    (heard(renderer) - level(pcm)).abs() < NEAR
}

#[test]
fn up_and_down_walk_the_rows_and_play_the_waves_they_land_on() {
    let mut app = open();
    let mut renderer = app.audio_started(SAMPLE_RATE, MAX_FRAMES, BLOCK as u32);
    app.take_requests();
    let browser = browser_rect(&painted(&mut app));
    click(&mut app, name_cell(browser, 1, 2));
    let served = serve(&mut app);
    assert_eq!((selected_row(&painted(&mut app), browser), served.reads), (Some(2), 1), "wanted the kick row selected and read once after the press");
    assert!(sounds_as(&mut renderer, KICK), "wanted the kick heard at its own level after the press");
    key(&mut app, Key::Down);
    let served = serve(&mut app);
    assert_eq!((selected_row(&painted(&mut app), browser), served.reads), (Some(3), 1), "wanted the arrow to land on the snare and read it");
    assert!(sounds_as(&mut renderer, SNARE), "wanted the snare, at half the kick's level, heard after the arrow");
    key(&mut app, Key::Down);
    let served = serve(&mut app);
    assert_eq!((selected_row(&painted(&mut app), browser), served.reads), (Some(4), 0), "wanted the arrow to land on the preset without reading anything");
    key(&mut app, Key::Down);
    assert_eq!(selected_row(&painted(&mut app), browser), Some(4), "wanted the last row to hold the selection");
    key(&mut app, Key::Up);
    key(&mut app, Key::Up);
    let served = serve(&mut app);
    assert_eq!((selected_row(&painted(&mut app), browser), served.reads), (Some(2), 1), "wanted the kick read once more, the snare's preview having taken its place in memory");
    assert!(sounds_as(&mut renderer, KICK), "wanted the kick heard again at its own level on landing");
    key(&mut app, Key::Escape);
    key(&mut app, Key::Down);
    let served = serve(&mut app);
    assert_eq!(selected_row(&painted(&mut app), browser), None, "wanted Escape to clear the selection");
    assert_eq!(served.play_from, 0, "wanted Down with nothing selected to leave the playhead where the arrows have no bar to step to, got {}", served.play_from);
    key(&mut app, Key::Right);
    let served = serve(&mut app);
    assert_eq!(served.play_from, BEATS_PER_BAR, "wanted Right with nothing selected to step the playhead a bar, got {}", served.play_from);
}

#[test]
fn right_opens_and_enters_a_folder_and_left_closes_and_climbs() {
    let mut app = open();
    app.take_requests();
    let browser = browser_rect(&painted(&mut app));
    click(&mut app, name_cell(browser, 1, 1));
    serve(&mut app);
    assert_eq!(folder_open(&painted(&mut app), 1), Some(false), "wanted A1 closed before the arrow");
    key(&mut app, Key::Right);
    let served = serve(&mut app);
    assert_eq!(served.lists, vec![format!("{ROOT}/A1")], "wanted Right to open A1 and list it, got {:?}", served.lists);
    assert_eq!((folder_open(&painted(&mut app), 1), selected_row(&painted(&mut app), browser)), (Some(true), Some(1)), "wanted A1 open and still selected");
    key(&mut app, Key::Right);
    let served = serve(&mut app);
    assert_eq!((selected_row(&painted(&mut app), browser), served.reads), (Some(2), 1), "wanted Right on the open folder to step into its first wave and play it");
    key(&mut app, Key::Left);
    assert_eq!(selected_row(&painted(&mut app), browser), Some(1), "wanted Left on a file to climb to its folder");
    key(&mut app, Key::Left);
    assert_eq!((folder_open(&painted(&mut app), 1), selected_row(&painted(&mut app), browser)), (Some(false), Some(1)), "wanted Left on the open folder to close it");
    key(&mut app, Key::Left);
    assert_eq!(selected_row(&painted(&mut app), browser), Some(0), "wanted Left on the closed folder to climb to the root");
    key(&mut app, Key::Left);
    key(&mut app, Key::Up);
    assert_eq!((folder_open(&painted(&mut app), 0), selected_row(&painted(&mut app), browser)), (Some(false), Some(0)), "wanted Left to close the root as it closes any open folder, and Up to leave it selected at the top");
}
