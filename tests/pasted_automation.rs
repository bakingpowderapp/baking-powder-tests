use bp::app::{App, Event};
use bp::arp::Articulation;
use bp::keyboard::Key;
use bp::project::{Aim, Clip, Curve, Node, Note, Project, TICKS_PER_BAR, TICKS_PER_BEAT, VELOCITY, beats};
use support::{CORE_ROW, centre, clip_block, painted, remembered};

const RAIL: i32 = 45;
const TILE: i32 = 120;
const BORDER: i32 = 8;
const PITCH: i32 = TILE - BORDER;
const VOL_W: i32 = 28;
const COLUMN: i32 = PITCH + VOL_W + 2 * BORDER;
const BAR_W: i32 = 120;
const LANE_H: i32 = TILE - 2 * BORDER;
const INTO_CELL: i32 = 10;
const BAND: &str = "OSC 1";
const PARAM: &str = "LEVEL";
const CLOSE: f64 = 1e-5;
const QUARTERS: i32 = 16;
const PASTE_AT: i32 = 4 * TICKS_PER_BAR;

fn node(tick: i32, travel: f32) -> Node {
    Node { tick, travel, bend: 0.0 }
}

fn rising_then_falling() -> Vec<Node> {
    vec![node(2 * TICKS_PER_BEAT, 0.2), node(3 * TICKS_PER_BEAT, 0.8), node(4 * TICKS_PER_BAR, 0.0)]
}

fn stepping_up() -> Vec<Node> {
    vec![node(3 * TICKS_PER_BEAT, 0.0), node(3 * TICKS_PER_BEAT + 1, 1.0)]
}

fn project(nodes: Vec<Node>) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    let notes = vec![Note { pitch: 60, start: 0.0, length: 1.0, velocity: VELOCITY, articulation: Articulation::PLAIN }];
    project.tracks[0].clips.push(Clip { start: 0, length: TICKS_PER_BAR, label: String::new(), notes, file: None });
    project.tracks[0].automation.curves.push(Curve { aim: Aim::knob(CORE_ROW, BAND, PARAM), lfo: None, nodes });
    project
}

fn open(project: &Project) -> App {
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false}}", project.to_json().write());
    let mut app = App::new();
    app.resume(&session);
    painted(&mut app);
    app
}

fn click(app: &mut App, (x, y): (i32, i32)) {
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    app.event(Event::Leave);
}

fn command(app: &mut App, key: char) {
    app.event(Event::Key { key: Key::Char(key), shift: false, command: true, repeat: false });
}

fn key(app: &mut App, key: Key, shift: bool) {
    app.event(Event::Key { key, shift, command: false, repeat: false });
    app.event(Event::KeyUp { key });
}

fn pick_clip(app: &mut App) {
    let block = clip_block(&painted(app));
    click(app, centre(block));
}

fn point_at_paste_bar(app: &mut App) {
    click(app, (COLUMN + (PASTE_AT / TICKS_PER_BAR) * BAR_W + INTO_CELL, RAIL + BORDER + LANE_H / 2));
}

fn starts(project: &Project) -> Vec<i32> {
    project.tracks[0].clips.iter().map(|clip| clip.start).collect()
}

fn level(project: &Project) -> Curve {
    project.tracks[0].automation.curves.iter().find(|curve| matches!(&curve.aim, Aim::Knob { band, name, .. } if band == BAND && name == PARAM) && curve.lfo.is_none()).cloned().expect("wanted the level curve kept")
}

fn reads(curve: &Curve, tick: i32) -> f64 {
    curve.at(beats(tick))
}

fn same_shape(curve: &Curve, from: i32, original: &Curve, what: &str) {
    for quarter in 0..=QUARTERS {
        let tick = quarter * TICKS_PER_BEAT / 4;
        let (wanted, got) = (reads(original, tick), reads(curve, from + tick));
        assert!((wanted - got).abs() <= CLOSE, "wanted {what} at beat {:.2} to read {wanted:.4} as the original does at beat {:.2}, got {got:.4}", beats(from + tick), beats(tick));
    }
}

fn untouched(curve: &Curve, original: &Curve, ticks: &[i32], what: &str) {
    for &tick in ticks {
        let (wanted, got) = (reads(original, tick), reads(curve, tick));
        assert!((wanted - got).abs() <= CLOSE, "wanted {what} at beat {:.2} left reading {wanted:.4}, got {got:.4}", beats(tick));
    }
}

fn nodes_of(curve: &Curve) -> Vec<(i32, f32)> {
    curve.nodes.iter().map(|node| (node.tick, node.travel)).collect()
}

#[test]
fn a_duplicate_reproduces_the_curve_under_the_clip_and_leaves_the_rest() {
    let original = level(&project(rising_then_falling()));
    let mut app = open(&project(rising_then_falling()));
    pick_clip(&mut app);
    command(&mut app, 'd');
    let project = remembered(&mut app);
    assert_eq!(starts(&project), vec![0, TICKS_PER_BAR], "wanted the duplicate laid right after the clip, got {:?}", starts(&project));
    let curve = level(&project);
    same_shape(&curve, TICKS_PER_BAR, &original, "the duplicate");
    untouched(&curve, &original, &[0, 2 * TICKS_PER_BEAT, 3 * TICKS_PER_BEAT, 3 * TICKS_PER_BEAT + TICKS_PER_BEAT / 2, TICKS_PER_BAR - 1], "the source");
    untouched(&curve, &original, &[2 * TICKS_PER_BAR + 2 * TICKS_PER_BEAT, 3 * TICKS_PER_BAR, 4 * TICKS_PER_BAR, 5 * TICKS_PER_BAR], "the curve past the duplicate");
}

#[test]
fn a_paste_leaves_the_curve_between_the_source_and_the_copy_as_it_was() {
    let original = level(&project(stepping_up()));
    let mut app = open(&project(stepping_up()));
    pick_clip(&mut app);
    command(&mut app, 'c');
    point_at_paste_bar(&mut app);
    command(&mut app, 'v');
    let project = remembered(&mut app);
    assert_eq!(starts(&project), vec![0, PASTE_AT], "wanted the copy pasted at the clicked bar, got {:?}", starts(&project));
    let curve = level(&project);
    untouched(&curve, &original, &[TICKS_PER_BAR, 2 * TICKS_PER_BAR, PASTE_AT - 1, PASTE_AT + 2 * TICKS_PER_BAR, PASTE_AT + 4 * TICKS_PER_BAR], "the curve beside the paste");
    same_shape(&curve, PASTE_AT, &original, "the pasted copy");
}

#[test]
fn a_locked_paste_moves_no_node() {
    let mut locked = project(stepping_up());
    locked.lock = true;
    let original = level(&locked);
    let mut app = open(&locked);
    pick_clip(&mut app);
    command(&mut app, 'c');
    point_at_paste_bar(&mut app);
    command(&mut app, 'v');
    let project = remembered(&mut app);
    assert_eq!(starts(&project), vec![0, PASTE_AT], "wanted the clip pasted under LOCK, got {:?}", starts(&project));
    assert_eq!(nodes_of(&level(&project)), nodes_of(&original), "wanted LOCK to keep every node where it was");
}

#[test]
fn a_nudged_clip_carries_its_curve_and_leaves_the_rest() {
    let original = level(&project(rising_then_falling()));
    let mut app = open(&project(rising_then_falling()));
    pick_clip(&mut app);
    key(&mut app, Key::Right, true);
    let project = remembered(&mut app);
    assert_eq!(starts(&project), vec![TICKS_PER_BAR], "wanted SHIFT-right to move the clip a bar on, got {:?}", starts(&project));
    let curve = level(&project);
    same_shape(&curve, TICKS_PER_BAR, &original, "the moved clip");
    untouched(&curve, &original, &[2 * TICKS_PER_BAR + 2 * TICKS_PER_BEAT, 3 * TICKS_PER_BAR, 4 * TICKS_PER_BAR, 5 * TICKS_PER_BAR], "the curve past the move");
}
