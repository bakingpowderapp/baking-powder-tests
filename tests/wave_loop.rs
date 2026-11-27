use bp::app::{App, Event, Request};
use bp::audio::{Audio, Renderer};
use bp::audio::vision::Seer;
use bp::draw::{Fill, Rect, Tone};
use bp::face::{CELL, FRAME, PITCH};
use bp::grid::PLAYHEAD_W;
use bp::keyboard::Key;
use bp::project::{Clip, Project, TICKS_PER_BEAT, Track};
use bp::wav::Wave;
use std::sync::Arc;
use support::{BLOCK, Onsets, RATE, SAMPLE_RATE, centre, clip_block, inked, painted};

const PATH: &str = "/x/burst.wav";
const NAME: &str = "burst";
const FILE_SECONDS: f64 = 1.0;
const BURST: (f64, f64) = (0.30, 0.32);
const HALF: i16 = 16_384;
const CLIP_BEATS: i32 = 8;
const LOOP: (f64, f64) = (0.5, 1.0);
const LOOP_FRAMES: usize = 12_000;
const FIRST_ONSET: usize = 2_400;
const WRAPS: u16 = 4;
const NEAR: usize = 4;
const SLACK: usize = 300;
const MAX_FRAMES: usize = 4_096;
const BAND_CELLS: i32 = 10;
const ROWS_UNDER_WAVE: i32 = 3;
const SELECTED: (i32, i32) = (4, 2);

fn burst() -> Vec<u8> {
    let (from, to) = ((BURST.0 * SAMPLE_RATE) as usize, (BURST.1 * SAMPLE_RATE) as usize);
    let samples: Vec<i16> = (0..(FILE_SECONDS * SAMPLE_RATE) as usize).map(|frame| if (from..to).contains(&frame) { HALF } else { 0 }).collect();
    let data: Vec<u8> = samples.iter().flat_map(|sample| sample.to_le_bytes()).collect();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&((36 + data.len()) as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&RATE.to_le_bytes());
    bytes.extend_from_slice(&(RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&data);
    bytes
}

fn project() -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0] = Track::audio();
    project.tracks[0].clips.push(Clip::audio(0, CLIP_BEATS * TICKS_PER_BEAT, String::from(NAME), String::from(NAME)));
    project
}

fn open() -> App {
    let session = format!("{{\"project\":{},\"keyboard\":false,\"palette\":false,\"automation\":false,\"beat_w\":120,\"origins\":[{{\"name\":\"{NAME}\",\"from\":\"{PATH}\"}}]}}", project().to_json().write());
    let mut app = App::new();
    app.resume(&session);
    loop {
        let requests = app.take_requests();
        if requests.is_empty() {
            break;
        }
        for request in requests {
            if let Request::Read { path } = request {
                app.read(path, Some(burst()));
            }
        }
    }
    app
}

fn click(app: &mut App, (x, y): (i32, i32)) {
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
}

fn editor_body(fills: &[Fill]) -> Rect {
    let inked = |fill: &&Fill| fill.tone == Tone::Ink100;
    let shelf_under = |line: Rect| fills.iter().filter(inked).find(|fill| fill.rect.h == FRAME && fill.rect.w > CELL && fill.rect.x == line.x - FRAME && fill.rect.y == line.y + line.h).map(|fill| fill.rect);
    let lines: Vec<(Rect, Rect)> = fills.iter().filter(inked).filter(|fill| fill.rect.w == PLAYHEAD_W && fill.rect.h >= CELL).filter_map(|fill| Some((fill.rect, shelf_under(fill.rect)?))).collect();
    let [(entry, shelf)] = lines.as_slice() else {
        panic!("wanted one entry line standing on the span lane's frame, found {}: {lines:?}", lines.len());
    };
    Rect::new(entry.x, entry.y, shelf.w - 2 * FRAME, entry.h)
}

fn loop_cell(body: Rect) -> Rect {
    Rect::new(body.x - FRAME + BAND_CELLS * PITCH, body.y + body.h + ROWS_UNDER_WAVE * PITCH, CELL, CELL)
}

fn interior(rect: Rect) -> Rect {
    Rect::new(rect.x + FRAME, rect.y + FRAME, rect.w - 2 * FRAME, rect.h - 2 * FRAME)
}

fn select(app: &mut App, body: Rect) {
    let (from, to) = (body.x + body.w / SELECTED.0, body.x + body.w / SELECTED.1);
    let y = body.y + body.h / 2;
    app.event(Event::Down { x: from, y, clicks: 1 });
    app.event(Event::Move { x: to, y });
    app.event(Event::Up { x: to, y });
}

fn heard(renderer: &mut Renderer, frames: usize) -> Onsets {
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut onsets = Onsets::default();
    while onsets.frame < frames {
        let n = BLOCK.min(frames - onsets.frame);
        renderer.render(&mut l[..n], &mut r[..n]);
        onsets.feed(&l[..n]);
    }
    onsets
}

fn check(onsets: &[usize], slack: usize) {
    assert!(onsets.len() >= WRAPS as usize, "wanted the burst heard {WRAPS} times in a second of looping, heard it at {onsets:?}");
    let first = onsets[0];
    assert!(first.abs_diff(FIRST_ONSET) <= slack, "wanted the first burst within {slack} frames of frame {FIRST_ONSET}, heard it at frame {first}");
    for pair in onsets.windows(2) {
        let gap = pair[1] - pair[0];
        assert!(gap.abs_diff(LOOP_FRAMES) <= slack, "wanted bursts {LOOP_FRAMES} frames apart within {slack}, got {gap} between frames {} and {}", pair[0], pair[1]);
    }
}

#[test]
fn a_loop_shorter_than_a_beat_wraps() {
    let wave = Arc::new(Wave::parse(String::from(NAME), String::from(PATH), &burst()).expect("wanted the burst file to read"));
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(&project(), None, &[], &[wave]);
    audio.set_loop(Some(LOOP));
    audio.play(LOOP.0);
    let onsets = heard(&mut renderer, RATE as usize);
    let wraps = audio.position().wraps;
    assert_eq!(wraps, WRAPS, "wanted a half-beat loop to wrap {WRAPS} times in a second, it wrapped {wraps} times");
    check(&onsets.at, NEAR);
}

#[test]
fn the_loop_cell_plays_the_selection_round_and_round() {
    let mut app = open();
    let mut renderer = app.audio_started(SAMPLE_RATE, MAX_FRAMES, BLOCK as u32);
    let clip = clip_block(&painted(&mut app));
    click(&mut app, centre(clip));
    let body = editor_body(&painted(&mut app));
    select(&mut app, body);
    let cell = loop_cell(body);
    assert!(inked(&painted(&mut app), Rect::new(cell.x, cell.y, CELL, FRAME)), "wanted the LOOP cell framed beside ZOOM at {cell:?}");
    click(&mut app, centre(cell));
    assert!(inked(&painted(&mut app), interior(cell)), "wanted the LOOP cell lit after pressing it");
    app.event(Event::Key { key: Key::Space, shift: false, command: false, repeat: false });
    let onsets = heard(&mut renderer, RATE as usize);
    check(&onsets.at, SLACK);
}
