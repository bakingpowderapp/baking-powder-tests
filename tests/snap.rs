use bp::app::{App, Event};
use bp::draw::{Fill, Rect, Tone};
use bp::face::{FRAME, H, PITCH};
use bp::grid::BAR_W;
use bp::grid::BEAT_W;
use bp::json::{self, Json};
use bp::keyboard::Key;
use bp::project::{Aim, Clip, Curve, Node, Project, TICKS_PER_BAR, TICKS_PER_BEAT};
use bp::ruler;
use support::{CORE_ROW, HEIGHT, centre, clip_block, lane_rows, painted, remembered, session};

const BORDER: i32 = 8;
const LINE: i32 = 2;
const TICK_H: i32 = 6;
const KEYS: i32 = 2 * PITCH;
const ROLL_ROW: i32 = 100;
const DOTTED_HALF: &str = "1/2.";
const THIRD: i32 = TICKS_PER_BEAT / 3;
const DRAG_BEATS: f64 = 1.6;

fn open(project: &Project, snap: &str, extra: &str) -> App {
    let session = format!("{{\"project\":{},\"snap\":\"{snap}\",\"keyboard\":false{extra}}}", project.to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app
}

fn play_from(app: &mut App) -> f64 {
    session(app).get("playheadBeat").and_then(Json::as_f64).unwrap_or(0.0)
}

fn click(app: &mut App, (x, y): (i32, i32)) {
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
}

fn drag(app: &mut App, (x, y): (i32, i32), dx: i32) {
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Move { x: x + dx, y });
    app.event(Event::Up { x: x + dx, y });
}

fn key(app: &mut App, key: Key, shift: bool) {
    app.event(Event::Key { key, shift, command: false, repeat: false });
    app.event(Event::KeyUp { key });
}

fn column(rows: &[Rect]) -> i32 {
    rows.iter().map(|row| row.x).min().expect("wanted a lane row") - BAR_W
}

fn beat_x(rows: &[Rect], beats: f64) -> i32 {
    column(rows) + (beats * BEAT_W) as i32
}

fn ticks_in_first_bar(fills: &[Fill]) -> usize {
    let rows = lane_rows(fills);
    let (left, top) = (column(&rows), rows[0].y);
    fills.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.w == LINE && fill.rect.h == TICK_H && fill.rect.y == top && fill.rect.x >= left && fill.rect.x < left + BAR_W).count()
}

fn stage_top() -> i32 {
    HEIGHT - (H - 2 * FRAME + BORDER) - BORDER
}

fn ruler_middle() -> i32 {
    stage_top() - ruler::H + ruler::H / 2
}

fn roll_cell() -> (i32, i32) {
    (BORDER - FRAME + KEYS + FRAME + 12, stage_top() + BORDER - FRAME + PITCH + ROLL_ROW)
}

fn one_bar() -> Project {
    let mut project = Project::new();
    project.tracks[0].clips.push(Clip::chord(0, String::new(), &[60]));
    project
}

fn placed(project: &Project) -> (i32, i32) {
    let clip = &project.tracks[0].clips[0];
    (clip.start, clip.length)
}

#[test]
fn the_lane_ticks_follow_the_snap() {
    for (snap, wanted) in [("1/4", 3), ("1/16", 15), ("1/12", 11), ("1/8.", 5), ("1/1", 0)] {
        let mut app = open(&Project::new(), snap, "");
        let got = ticks_in_first_bar(&painted(&mut app));
        assert_eq!(got, wanted, "wanted {wanted} ticks across the first bar at a {snap} snap, got {got}");
    }
}

#[test]
fn a_dragged_clip_lands_on_the_chosen_grid() {
    for (snap, wanted) in [("1/4", 2 * TICKS_PER_BEAT), ("1/8", 3 * TICKS_PER_BEAT / 2), ("1/12", 5 * THIRD)] {
        let mut app = open(&one_bar(), snap, "");
        let block = clip_block(&painted(&mut app));
        drag(&mut app, centre(block), (DRAG_BEATS * BEAT_W) as i32);
        let start = remembered(&mut app).tracks[0].clips[0].start;
        assert_eq!(start, wanted, "wanted a drag of {DRAG_BEATS} beats to land the clip on the {snap} grid at tick {wanted}, got {start}");
    }
}

#[test]
fn the_arrows_step_the_playhead_and_nudge_a_clip_along_a_dotted_grid() {
    let mut project = Project::new();
    project.tracks[0].clips.push(Clip { start: 0, length: TICKS_PER_BEAT, label: String::new(), notes: Vec::new(), file: None });
    let mut app = open(&project, DOTTED_HALF, "");
    let block = clip_block(&painted(&mut app));
    app.take_requests();
    let mut stepped = Vec::new();
    for _ in 0..4 {
        key(&mut app, Key::Right, true);
        stepped.push(play_from(&mut app));
    }
    for _ in 0..4 {
        key(&mut app, Key::Left, true);
        stepped.push(play_from(&mut app));
    }
    assert_eq!(stepped, vec![3.0, 4.0, 7.0, 8.0, 7.0, 4.0, 3.0, 0.0], "wanted shift and the arrows to walk the dotted-half grid that restarts at each bar, got {stepped:?}");
    click(&mut app, centre(block));
    app.take_requests();
    let mut nudged = Vec::new();
    for _ in 0..4 {
        key(&mut app, Key::Right, false);
        nudged.push(remembered(&mut app).tracks[0].clips[0].start);
    }
    for _ in 0..4 {
        key(&mut app, Key::Left, false);
        nudged.push(remembered(&mut app).tracks[0].clips[0].start);
    }
    let beats: Vec<i32> = nudged.iter().map(|tick| tick / TICKS_PER_BEAT).collect();
    assert_eq!(beats, vec![3, 4, 7, 8, 7, 4, 3, 0], "wanted the arrows to nudge the clip along the same grid, got beats {beats:?} from ticks {nudged:?}");
}

#[test]
fn a_moved_loop_lands_on_the_dotted_grid() {
    let mut app = open(&Project::new(), DOTTED_HALF, ",\"loopStartBeat\":0,\"loopEndBeat\":4");
    let rows = lane_rows(&painted(&mut app));
    drag(&mut app, (beat_x(&rows, 3.0) + 1, ruler_middle()), (5.0 * BEAT_W) as i32);
    let session = session(&mut app);
    let range = (session.get("loopStartBeat").and_then(Json::as_f64), session.get("loopEndBeat").and_then(Json::as_f64));
    assert_eq!(range, (Some(4.0), Some(8.0)), "wanted the loop grabbed at beat 3 and carried to beat 8 to settle on the bar at 4, got {range:?}");
}

#[test]
fn a_resize_keeps_the_clip_edges_on_the_grid() {
    let mut app = open(&one_bar(), DOTTED_HALF, "");
    let block = clip_block(&painted(&mut app));
    let y = block.y + block.h / 2;
    drag(&mut app, (block.x + 2, y), (3.0 * BEAT_W) as i32);
    let held = placed(&remembered(&mut app));
    assert_eq!(held, (0, TICKS_PER_BAR), "wanted the head left alone where no grid line leaves a three-beat clip, got {held:?}");
    drag(&mut app, (block.x + block.w - 2, y), -((2.0 * BEAT_W) as i32));
    let shorter = placed(&remembered(&mut app));
    assert_eq!(shorter, (0, 3 * TICKS_PER_BEAT), "wanted the end pulled to beat 2 to stop on the grid's three-beat line, got {shorter:?}");
}

#[test]
fn a_clip_shorter_than_the_roll_snap_still_takes_a_drawn_note() {
    let mut project = Project::new();
    project.tracks[0].clips.push(Clip { start: 0, length: TICKS_PER_BEAT / 8, label: String::new(), notes: Vec::new(), file: None });
    let mut app = open(&project, "1/32", ",\"picked\":[[0,0]]");
    painted(&mut app);
    drag(&mut app, roll_cell(), 12);
    let drawn: Vec<(f32, f32)> = remembered(&mut app).tracks[0].clips[0].notes.iter().map(|note| (note.start, note.length)).collect();
    assert_eq!(drawn, vec![(0.0, 0.125)], "wanted one note filling the eighth-of-a-beat clip, got {drawn:?}");
}

#[test]
fn fractional_beats_save_as_beats_and_reopen_on_the_same_ticks() {
    let mut project = Project::new();
    project.tracks[0].clips.push(Clip { start: 3 * TICKS_PER_BEAT / 2, length: THIRD, label: String::new(), notes: Vec::new(), file: None });
    project.tracks[0].automation.curves.push(Curve { aim: Aim::knob(CORE_ROW, "OSC 1", "LEVEL"), lfo: None, nodes: vec![Node { tick: THIRD, travel: 0.5, bend: 0.0 }, Node { tick: TICKS_PER_BAR, travel: 1.0, bend: 0.0 }] });
    let text = project.to_json().write();
    let document = json::parse(&text).expect("wanted the project to write as JSON");
    let start = document.get("tracks").and_then(Json::as_array).and_then(|tracks| tracks[0].get("clips")).and_then(Json::as_array).and_then(|clips| clips[0].get("startBeat")).and_then(Json::as_f64);
    assert_eq!(start, Some(1.5), "wanted the clip's start written as one and a half beats, got {start:?}");
    let opened = Project::from_json(&document).expect("wanted the project to reopen");
    let ticks: Vec<i32> = opened.tracks[0].automation.curves[0].nodes.iter().map(|node| node.tick).collect();
    assert_eq!((placed(&opened), ticks), ((3 * TICKS_PER_BEAT / 2, THIRD), vec![THIRD, TICKS_PER_BAR]), "wanted the triplet and the half beat back on their ticks, got {:?} and {:?}", placed(&opened), opened.tracks[0].automation.curves[0].nodes);
}
