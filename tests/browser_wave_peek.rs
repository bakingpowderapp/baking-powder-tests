use bp::app::{App, Event, Request};
use bp::browser::{Found, Item};
use bp::draw::{Fill, Rect, Tone};
use bp::face::{CELL, PITCH, READOUT_ROWS, W};
use bp::project::Project;
use std::f64::consts::TAU;
use support::{SAMPLE_RATE, browser_rect, centre, face_rect, painted, wav};

const SECONDS: f64 = 1.0;
const HZ: f64 = 110.0;
const LEVEL: f64 = 0.5;
const TOO_LONG_SAMPLES: usize = 1_600_000;
const WAVE_ROW: i32 = 1;
const DEPTH: i32 = 1;

fn tone() -> Vec<i16> {
    (0..(SECONDS * SAMPLE_RATE) as usize).map(|i| ((TAU * HZ * i as f64 / SAMPLE_RATE).sin() * LEVEL * f64::from(i16::MAX)) as i16).collect()
}

fn folder_with(name: &str, bytes: &[u8]) -> (String, String) {
    let root = std::env::temp_dir().join(format!("bp-peek-{}-{name}", std::process::id()));
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

fn hover_wave(app: &mut App, root: &str, path: &str, bytes: u64) -> Vec<Request> {
    app.listed(root, vec![Found { path: String::from(path), name: String::from("loop.wav"), item: Item::Wave(bytes) }]);
    let row = wave_row(&painted(app));
    let (x, y) = centre(row);
    app.event(Event::Move { x, y });
    app.take_requests()
}

fn waveform_columns(fills: &[Fill]) -> Vec<Rect> {
    let face = face_rect(fills);
    let readout = Rect::new(face.x, face.y, W, READOUT_ROWS * PITCH);
    fills.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.w == 1 && fill.rect.h >= 2 && readout.contains(fill.rect.x, fill.rect.y)).map(|fill| fill.rect).collect()
}

#[test]
fn hovering_a_short_wave_draws_it_in_the_readout() {
    let bytes = wav(&tone());
    let (root, path) = folder_with("short", &bytes);
    let mut app = browsing(&root);
    let requests = hover_wave(&mut app, &root, &path, bytes.len() as u64);
    assert!(requests.contains(&Request::Read { path: path.clone() }), "wanted the hovered wave read for its picture, got {requests:?}");
    assert!(waveform_columns(&painted(&mut app)).is_empty(), "wanted no waveform before the bytes arrive");
    app.read(path.clone(), Some(bytes));
    let fills = painted(&mut app);
    let columns = waveform_columns(&fills);
    let face = face_rect(&fills);
    assert!(columns.len() > W as usize / 2, "wanted a waveform of columns across the readout, got {} columns", columns.len());
    assert!(columns.iter().any(|rect| rect.x > face.x + W * 3 / 4), "wanted the waveform to reach the right quarter of the readout, where a word never does");
    let middle = columns.iter().map(|rect| rect.y + rect.h / 2).min().unwrap_or(0);
    assert!(columns.iter().all(|rect| (rect.y + rect.h / 2 - middle).abs() <= 1), "wanted every column centred on one line");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_long_wave_keeps_the_word_and_is_not_read() {
    let bytes = wav(&vec![0; TOO_LONG_SAMPLES]);
    let (root, path) = folder_with("long", &bytes);
    let mut app = browsing(&root);
    let requests = hover_wave(&mut app, &root, &path, bytes.len() as u64);
    assert!(!requests.iter().any(|request| matches!(request, Request::Read { .. })), "wanted a wave over the size cap left unread on hover, got {requests:?}");
    assert!(waveform_columns(&painted(&mut app)).is_empty(), "wanted the WAV word, not a waveform, for a long file");
    std::fs::remove_dir_all(&root).ok();
}
