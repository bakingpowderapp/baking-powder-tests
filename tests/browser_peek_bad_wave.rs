use bp::app::{App, Event, Request};
use bp::browser::{Found, Item};
use bp::draw::{Fill, Rect, Tone};
use bp::face::{CELL, PITCH, READOUT_ROWS, W};
use bp::project::Project;
use support::{browser_rect, centre, face_rect, painted};

const WAVE_ROW: i32 = 1;
const DEPTH: i32 = 1;
const AWAY: (i32, i32) = (1, 1);
const BROKEN: &[u8] = b"RIFF\x24\x00\x00\x00WAVEjunk, not a wave at all";

fn folder_with(bytes: &[u8]) -> (String, String) {
    let root = std::env::temp_dir().join(format!("bp-peek-broken-{}", std::process::id()));
    std::fs::create_dir_all(&root).expect("wanted a folder for the wave");
    let path = root.join("loop.wav");
    std::fs::write(&path, bytes).expect("wanted the wave written");
    (root.to_string_lossy().into_owned(), path.to_string_lossy().into_owned())
}

fn browsing(root: &str) -> App {
    let session = format!("{{\"project\":{},\"aside\":\"browser\",\"keyboard\":false,\"folders\":[\"{root}\"],\"unfolded\":[\"{root}\"]}}", Project::new().to_json().write());
    let mut app = App::new();
    app.resume(&session);
    app.take_requests();
    app
}

fn wave_row(fills: &[Fill]) -> Rect {
    let browser = browser_rect(fills);
    Rect::new(browser.x + (DEPTH + 1) * PITCH, browser.y + WAVE_ROW * PITCH, browser.w - (DEPTH + 1) * PITCH, CELL)
}

fn hover_wave(app: &mut App) -> Vec<Request> {
    let row = wave_row(&painted(app));
    let (x, y) = centre(row);
    app.event(Event::Move { x, y });
    painted(app);
    app.take_requests()
}

fn waveform_columns(fills: &[Fill]) -> Vec<Rect> {
    let face = face_rect(fills);
    let readout = Rect::new(face.x, face.y, W, READOUT_ROWS * PITCH);
    fills.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.w == 1 && fill.rect.h >= 2 && readout.contains(fill.rect.x, fill.rect.y)).map(|fill| fill.rect).collect()
}

#[test]
fn a_wave_that_will_not_parse_is_read_once_and_keeps_its_word() {
    let (root, path) = folder_with(BROKEN);
    let mut app = browsing(&root);
    app.listed(&root, vec![Found { path: path.clone(), name: String::from("loop.wav"), item: Item::Wave(BROKEN.len() as u64) }]);
    let requests = hover_wave(&mut app);
    assert!(requests.contains(&Request::Read { path: path.clone() }), "wanted the hovered wave read for its picture, got {requests:?}");
    app.read(path.clone(), Some(BROKEN.to_vec()));
    assert!(waveform_columns(&painted(&mut app)).is_empty(), "wanted the WAV word kept for a file that would not parse");
    app.event(Event::Move { x: AWAY.0, y: AWAY.1 });
    painted(&mut app);
    app.take_requests();
    let again = hover_wave(&mut app);
    assert!(!again.iter().any(|request| matches!(request, Request::Read { .. })), "wanted a wave that failed to parse remembered and not asked of the shell again on the next hover, got {again:?}");
    std::fs::remove_dir_all(&root).ok();
}
