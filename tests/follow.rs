use bp::app::{App, Event};
use bp::audio::Renderer;
use bp::draw::{Fill, Rect, Tone};
use bp::grid::{BEAT_W, PLAYHEAD_W};
use bp::json::Json;
use bp::keyboard::Key;
use bp::project::{Clip, Project, TICKS_PER_BAR};
use bp::ruler;
use support::{BLOCK, HEIGHT, SAMPLE_RATE, WIDTH, beat_frame, centre, clip_block, painted, session};

const MAX_FRAMES: usize = 4_096;
const RAIL: i32 = 45;
const STEP: usize = 2;
const SETTINGS_ITEM: i32 = 10;
const ITEM_H: i32 = 32;
const MENU_INSET: i32 = 12;
const COLUMN_X: i32 = 40;
const PAST_THE_EDGE: usize = 4;

fn project() -> Project {
    let mut project = Project::new();
    project.tracks[0].clips.push(Clip { start: 0, length: TICKS_PER_BAR, label: String::new(), notes: Vec::new(), file: None });
    project
}

fn open(follow: Option<bool>) -> App {
    let setting = follow.map_or(String::new(), |on| format!(",\"follow\":{on}"));
    let session = format!("{{\"project\":{},\"automation\":false,\"keyboard\":false,\"palette\":false{setting}}}", project().to_json().write());
    let mut app = App::new();
    app.resume(&session);
    app
}

fn start(app: &mut App) -> Renderer {
    let renderer = app.audio_started(SAMPLE_RATE, MAX_FRAMES, BLOCK as u32);
    app.take_requests();
    renderer
}

fn space(app: &mut App) {
    app.event(Event::Key { key: Key::Space, shift: false, command: false, repeat: false });
    app.tick(BLOCK as f32 / SAMPLE_RATE as f32);
}

fn advance(app: &mut App, renderer: &mut Renderer, total: usize) {
    let (mut l, mut r) = (vec![0.0; MAX_FRAMES], vec![0.0; MAX_FRAMES]);
    let mut done = 0;
    while done < total {
        let n = (total - done).min(MAX_FRAMES);
        renderer.render(&mut l[..n], &mut r[..n]);
        done += n;
        app.tick(n as f32 / SAMPLE_RATE as f32);
    }
}

fn past_the_edge() -> usize {
    beat_frame((f64::from(WIDTH) / BEAT_W).ceil() as usize + PAST_THE_EDGE)
}

fn playhead_line(playing: &[Fill], stopped: &[Fill]) -> Option<Rect> {
    let fresh: Vec<Rect> = playing.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.w == PLAYHEAD_W && fill.rect.h > ruler::H && !stopped.iter().any(|other| other.rect == fill.rect)).map(|fill| fill.rect).collect();
    let tallest = fresh.iter().map(|rect| rect.h).max()?;
    let mut lines = fresh.iter().filter(|rect| rect.h == tallest);
    let line = *lines.next()?;
    lines.next().is_none().then_some(line)
}

fn click(app: &mut App, (x, y): (i32, i32)) {
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    app.event(Event::Leave);
}

fn follows(app: &mut App) -> bool {
    app.event(Event::Blur);
    session(app).get("follow").and_then(Json::as_bool).unwrap_or(true)
}

fn rail_wash(fills: &[Fill]) -> Option<Rect> {
    fills.iter().find(|fill| fill.tone == Tone::Ink25 && fill.rect.y < RAIL && fill.rect.h >= 30).map(|fill| fill.rect)
}

fn rail_button(app: &mut App, sweep: impl Iterator<Item = i32>) -> Option<Rect> {
    for x in sweep {
        app.event(Event::Move { x, y: RAIL / 2 });
        if let Some(rect) = rail_wash(&painted(app)) {
            return Some(rect);
        }
    }
    None
}

fn open_settings(app: &mut App) {
    let file = rail_button(app, (0..WIDTH).step_by(STEP)).expect("wanted the File button at the left of the rail");
    click(app, centre(file));
    click(app, (file.x + MENU_INSET, file.y + file.h + SETTINGS_ITEM * ITEM_H + ITEM_H / 2));
}

fn lowest_control(app: &mut App) -> Rect {
    let mut lowest: Option<Rect> = None;
    for y in (RAIL..HEIGHT).step_by(STEP) {
        app.event(Event::Move { x: COLUMN_X, y });
        if let Some(rect) = painted(app).iter().find(|fill| fill.tone == Tone::Ink25 && fill.rect.y >= RAIL && fill.rect.contains(COLUMN_X, y)).map(|fill| fill.rect)
            && lowest.is_none_or(|low| rect.y > low.y)
        {
            lowest = Some(rect);
        }
    }
    lowest.expect("wanted the settings controls to wash under the pointer")
}

#[test]
fn the_tracks_page_along_with_the_playhead_by_default() {
    let mut app = open(None);
    assert!(follows(&mut app), "wanted following on in a session that never set it, got it off");
    let mut renderer = start(&mut app);
    let stopped = painted(&mut app);
    space(&mut app);
    advance(&mut app, &mut renderer, past_the_edge());
    let line = playhead_line(&painted(&mut app), &stopped).expect("wanted the playhead line, the one fresh line taller than the bar lines, kept in view past the right edge with FOLLOW ON");
    assert!(line.x < WIDTH / 2, "wanted the view paged along so the line sits in the left half, got x {}", line.x);
}

#[test]
fn follow_off_leaves_the_view_where_it_was() {
    let mut app = open(Some(false));
    let mut renderer = start(&mut app);
    let stopped = painted(&mut app);
    let before = clip_block(&stopped);
    space(&mut app);
    advance(&mut app, &mut renderer, past_the_edge());
    let playing = painted(&mut app);
    let line = playhead_line(&playing, &stopped);
    assert!(line.is_none(), "wanted the playhead to run off the right edge with FOLLOW OFF, found a line at {line:?}");
    let after = clip_block(&playing);
    assert_eq!(after.x, before.x, "wanted the clip left where it was with FOLLOW OFF, got x {} from x {}", after.x, before.x);
}

#[test]
fn the_lowest_control_of_the_settings_column_switches_following_off_and_on() {
    let mut app = open(None);
    open_settings(&mut app);
    let control = lowest_control(&mut app);
    click(&mut app, centre(control));
    assert!(!follows(&mut app), "wanted the lowest control, FOLLOW, to switch following off, got it still on");
    click(&mut app, centre(control));
    assert!(follows(&mut app), "wanted a second press to switch following back on, got it still off");
}
