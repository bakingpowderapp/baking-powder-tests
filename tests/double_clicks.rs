use bp::app::{App, Event};
use bp::draw::{Fill, Rect, Tone};
use bp::face::{FRAME, H};
use bp::project::{Clip, DEFAULT_BPM, Project, TICKS_PER_BAR};
use bp::ruler;
use support::{HEIGHT, WIDTH, centre, clip_block, painted, remembered};

const BORDER: i32 = 8;
const RAIL: i32 = 45;
const TILE: i32 = 120;
const VOL_W: i32 = 28;
const COLUMN_W: i32 = TILE + VOL_W + BORDER;
const HANDLE_H: i32 = 6;
const CELL_INNER_H: i32 = 24;
const RAIL_STEP: usize = 2;
const SWEEP: usize = 4;
const TEMPO: usize = 6;
const LIFT: i32 = 30;
const FAST: f64 = 150.0;
const HALF: f32 = 0.5;
const BARS: i32 = 4;

fn open() -> App {
    let mut project = Project::new();
    project.bpm = FAST;
    project.master.gain = HALF;
    project.tracks[0].gain = HALF;
    project.tracks[0].sends[0] = HALF;
    project.tracks[0].clips.push(Clip { start: 0, length: BARS * TICKS_PER_BAR, label: String::from("take"), notes: Vec::new(), file: None });
    let session = format!("{{\"project\":{},\"automation\":false,\"keyboard\":false,\"palette\":false}}", project.to_json().write());
    let mut app = App::new();
    app.resume(&session);
    painted(&mut app);
    app
}

fn click(app: &mut App, x: i32, y: i32, clicks: i32) {
    app.event(Event::Down { x, y, clicks });
    app.event(Event::Up { x, y });
}

fn double_click(app: &mut App, rect: Rect) {
    let (x, y) = centre(rect);
    click(app, x, y, 1);
    click(app, x, y, 2);
}

fn drag(app: &mut App, rect: Rect, dy: i32) {
    let (x, y) = centre(rect);
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Move { x, y: y + dy });
    app.event(Event::Up { x, y: y + dy });
}

fn tile() -> Rect {
    Rect::new(0, RAIL, COLUMN_W, TILE)
}

fn stage() -> Rect {
    let top = HEIGHT - (H - 2 * FRAME + BORDER) - BORDER;
    Rect::new(0, top, WIDTH, HEIGHT - top)
}

fn zoom_readout() -> Rect {
    let arrangement_bottom = stage().y - ruler::H - BORDER;
    Rect::new(BORDER, arrangement_bottom + BORDER, COLUMN_W - 2 * BORDER, ruler::H)
}

fn tempo_button(app: &mut App) -> Rect {
    let mut found: Vec<Rect> = Vec::new();
    for x in (0..WIDTH).step_by(RAIL_STEP) {
        app.event(Event::Move { x, y: RAIL / 2 });
        if let Some(rect) = painted(app).iter().find(|fill| fill.tone == Tone::Ink25 && fill.rect.y < RAIL && fill.rect.h >= 30).map(|fill| fill.rect)
            && found.last() != Some(&rect)
        {
            found.push(rect);
        }
    }
    app.event(Event::Leave);
    *found.get(TEMPO).unwrap_or_else(|| panic!("wanted the tempo as the seventh header button, found {} buttons", found.len()))
}

fn fader_handle(fills: &[Fill], within: Rect, wide: bool) -> Rect {
    fills
        .iter()
        .filter(|fill| fill.tone == Tone::Ink100 && fill.rect.h == HANDLE_H && (fill.rect.w > VOL_W) == wide && within.contains(fill.rect.x, fill.rect.y))
        .map(|fill| fill.rect)
        .min_by_key(|rect| rect.y)
        .unwrap_or_else(|| panic!("wanted a {HANDLE_H} px fader handle painted within {}x{} at {},{}", within.w, within.h, within.x, within.y))
}

fn ink25(fills: &[Fill], region: Rect) -> Vec<Rect> {
    fills.iter().filter(|fill| fill.tone == Tone::Ink25 && region.contains(fill.rect.x, fill.rect.y)).map(|fill| fill.rect).collect()
}

fn washes(app: &mut App, region: Rect) -> Vec<Rect> {
    app.event(Event::Leave);
    let quiet = ink25(&painted(app), region);
    let mut found: Vec<Rect> = Vec::new();
    for y in (region.y..region.y + region.h).step_by(SWEEP) {
        for x in (region.x..region.x + region.w).step_by(SWEEP) {
            app.event(Event::Move { x, y });
            for rect in ink25(&painted(app), region) {
                if !quiet.contains(&rect) && !found.contains(&rect) {
                    found.push(rect);
                }
            }
        }
    }
    app.event(Event::Leave);
    found
}

fn first_send_cell(app: &mut App) -> Rect {
    let cells: Vec<Rect> = washes(app, tile()).into_iter().filter(|rect| rect.h == CELL_INNER_H).collect();
    let row = cells.iter().map(|rect| rect.y).min().unwrap_or_else(|| panic!("wanted the send cells to wash under the pointer in the first tile, found no {CELL_INNER_H} px cells"));
    cells.into_iter().filter(|rect| rect.y == row).min_by_key(|rect| rect.x).expect("wanted a leftmost send cell")
}

#[test]
fn a_double_click_on_the_tempo_returns_the_default_bpm() {
    let mut app = open();
    let tempo = tempo_button(&mut app);
    double_click(&mut app, tempo);
    let bpm = remembered(&mut app).bpm;
    assert_eq!(bpm, DEFAULT_BPM, "wanted the tempo back at {DEFAULT_BPM} after a double-click, got {bpm}");
}

#[test]
fn a_double_click_on_the_zoom_readout_returns_the_default_zoom() {
    let mut app = open();
    let readout = zoom_readout();
    let rested = clip_block(&painted(&mut app)).w;
    drag(&mut app, readout, LIFT);
    let zoomed = clip_block(&painted(&mut app)).w;
    assert_ne!(zoomed, rested, "wanted the clip painted at another width after dragging the zoom {LIFT} px, still {zoomed} px");
    double_click(&mut app, readout);
    let reset = clip_block(&painted(&mut app)).w;
    assert_eq!(reset, rested, "wanted the clip back at {rested} px after a double-click on the zoom, got {reset} px");
}

#[test]
fn a_double_click_on_a_track_fader_returns_unity_gain() {
    let mut app = open();
    let handle = fader_handle(&painted(&mut app), tile(), false);
    drag(&mut app, handle, -LIFT);
    let lifted = remembered(&mut app).tracks[0].gain;
    assert!(lifted > HALF, "wanted the track gain above {HALF} after lifting its fader {LIFT} px, got {lifted}");
    let handle = fader_handle(&painted(&mut app), tile(), false);
    double_click(&mut app, handle);
    let gain = remembered(&mut app).tracks[0].gain;
    assert_eq!(gain, 1.0, "wanted the track gain back at unity after a double-click on its fader, got {gain}");
}

#[test]
fn a_double_click_on_the_master_fader_returns_unity_gain() {
    let mut app = open();
    let handle = fader_handle(&painted(&mut app), stage(), true);
    double_click(&mut app, handle);
    let gain = remembered(&mut app).master.gain;
    assert_eq!(gain, 1.0, "wanted the master gain back at unity after a double-click on its fader, got {gain}");
}

#[test]
fn a_double_click_on_a_send_clears_it() {
    let mut app = open();
    let cell = first_send_cell(&mut app);
    double_click(&mut app, cell);
    let send = remembered(&mut app).tracks[0].sends[0];
    assert_eq!(send, 0.0, "wanted send A cleared after a double-click on its cell, got {send}");
}
