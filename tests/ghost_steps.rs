use bp::app::{App, Event};
use bp::arp::{Arp, Articulation};
use bp::core::Params;
use bp::draw::{Fill, Rect, Tone};
use bp::face::{CELL, FRAME, PITCH};
use bp::project::{Aim, Clip, Curve, Node, Note, Project, Row, TICKS_PER_BAR, TICKS_PER_BEAT, VELOCITY};
use support::{CORE_ROW, centre, clip_block, core_layer, midi, painted, remembered};

const KEYS: i32 = 2 * PITCH;
const ROOT: u8 = 60;
const THIRD: u8 = 64;
const FIFTH: i32 = 7;
const BAND: &str = "OSC 2";
const TUNE: &str = "TUNE";
const TUNE_LOW: f64 = -48.0;
const TUNE_HIGH: f64 = 48.0;
const TICK: f32 = 0.25;
const TICKS_TO_FLIP: usize = 16;
const TICKS_TO_PEAK: usize = 3;

struct Grid {
    top: i32,
    right: i32,
}

fn note(pitch: u8, start: f32) -> Note {
    Note { pitch, start, length: 1.0, velocity: VELOCITY, articulation: Articulation::PLAIN }
}

fn travel(tune: i32) -> f32 {
    ((f64::from(tune) - TUNE_LOW) / (TUNE_HIGH - TUNE_LOW)) as f32
}

fn tune_of(travel: f64) -> i32 {
    (TUNE_LOW + (TUNE_HIGH - TUNE_LOW) * travel).round() as i32
}

fn project(notes: &[(u8, f32)], tune: i32) -> Project {
    let mut patch = Params::DEFAULT;
    patch.osc2.level = 1.0;
    patch.osc2.tune = f64::from(tune);
    let mut project = Project::new();
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch))];
    let notes = notes.iter().map(|&(pitch, start)| note(pitch, start)).collect();
    project.tracks[0].clips.push(Clip { start: 0, length: TICKS_PER_BAR, label: String::new(), notes, file: None });
    project
}

fn open(project: &Project) -> App {
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false,\"ghostNotes\":true}}", project.to_json().write());
    let mut app = App::new();
    app.resume(&session);
    let clip = clip_block(&painted(&mut app));
    click(&mut app, centre(clip));
    app
}

fn click(app: &mut App, (x, y): (i32, i32)) {
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    app.event(Event::Leave);
}

fn drag(app: &mut App, (x, y): (i32, i32), (to_x, to_y): (i32, i32)) {
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Move { x: to_x, y: to_y });
    app.event(Event::Up { x: to_x, y: to_y });
    app.event(Event::Leave);
}

fn knob(project: &Project) -> f64 {
    match &project.tracks[0].rack[CORE_ROW] {
        Row::Core(layer) => layer.patch.osc2.tune,
        Row::Midi(_) | Row::Effect(_) | Row::Bus(_) => panic!("wanted the Core under the MIDI row"),
    }
}

fn tune_curve(project: &Project) -> Option<&Curve> {
    project.tracks[0].automation.curves.iter().find(|curve| curve.lfo.is_none() && matches!(&curve.aim, Aim::Knob { band, name, .. } if band == BAND && name == TUNE))
}

fn held(project: &Project, beat: f64) -> i32 {
    let curve = tune_curve(project).expect("wanted a TUNE curve drawn on osc 2 by the drag");
    tune_of(curve.at(beat))
}

fn grid(fills: &[Fill]) -> Grid {
    let edges: Vec<Rect> = fills.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.w == KEYS + FRAME && fill.rect.h == FRAME).map(|fill| fill.rect).collect();
    let bottom = edges.iter().map(|edge| edge.y).max().expect("wanted the roll's header cells framed");
    let right = edges.iter().map(|edge| edge.x + edge.w).max().expect("wanted the roll's header cells framed");
    Grid { top: bottom + FRAME, right }
}

fn block_like(fill: &Fill, grid: &Grid) -> bool {
    fill.rect.y >= grid.top && fill.rect.x > 0 && fill.rect.x + fill.rect.w < grid.right && fill.rect.h > FRAME && fill.rect.h < CELL && fill.rect.w > 2 * CELL
}

fn real_notes(fills: &[Fill], grid: &Grid) -> Vec<Rect> {
    let mut found: Vec<Rect> = fills.iter().filter(|fill| fill.tone == Tone::Ink100 && block_like(fill, grid)).map(|fill| fill.rect).collect();
    found.sort_by_key(|rect| (rect.x, rect.y));
    found
}

fn ghosts(fills: &[Fill], grid: &Grid, notes: &[Rect]) -> Vec<Rect> {
    let mut found: Vec<Rect> = fills.iter().filter(|fill| fill.tone == Tone::Ink25 && block_like(fill, grid) && notes.iter().any(|note| note.x == fill.rect.x && note.w == fill.rect.w)).map(|fill| fill.rect).collect();
    found.sort_by_key(|rect| (rect.x, rect.y));
    found
}

fn painted_ghosts(app: &mut App) -> (Vec<Rect>, Vec<Rect>) {
    let fills = painted(app);
    let grid = grid(&fills);
    let notes = real_notes(&fills, &grid);
    let ghosts = ghosts(&fills, &grid, &notes);
    (notes, ghosts)
}

fn row_h(note: Rect, ghost: Rect) -> i32 {
    let gap = note.y - ghost.y;
    assert!(gap > 0 && gap % FIFTH == 0, "wanted the ghost a whole {FIFTH} rows over its note, got {ghost:?} over {note:?}");
    gap / FIFTH
}

fn rim_fade(fills: &[Fill], ghost: Rect) -> (Tone, Tone, u8) {
    fills.iter().find(|fill| fill.rect.x == ghost.x && fill.rect.y == ghost.y && fill.rect.w == ghost.w && fill.rect.h < ghost.h).map(|fill| (fill.tone, fill.toward, fill.mix)).expect("wanted the ghost's top rim painted over its body")
}

#[test]
fn dragging_one_ghost_steps_only_its_note() {
    let mut app = open(&project(&[(ROOT, 0.0), (THIRD, 2.0)], FIFTH));
    let (notes, ghosts) = painted_ghosts(&mut app);
    assert_eq!(ghosts.len(), 2, "wanted a ghost over each of the two notes, got {ghosts:?}");
    let row = row_h(notes[0], ghosts[0]);
    let from = centre(ghosts[0]);
    drag(&mut app, from, (from.0, from.1 + 2 * row));
    let project = remembered(&mut app);
    assert_eq!(knob(&project), f64::from(FIFTH), "wanted the TUNE knob left at {FIFTH} st by the drag, got {} st", knob(&project));
    assert_eq!(held(&project, 0.0), FIFTH - 2, "wanted the first note held two semitones lower, got {} st", held(&project, 0.0));
    assert_eq!(held(&project, 2.0), FIFTH, "wanted the second note still at {FIFTH} st, got {} st", held(&project, 2.0));
    let (_, moved) = painted_ghosts(&mut app);
    assert_eq!(moved.len(), 2, "wanted both ghosts still painted after the drag, got {moved:?}");
    assert_eq!(moved[0].y, ghosts[0].y + 2 * row, "wanted the first ghost two rows lower, got {:?} from {:?}", moved[0], ghosts[0]);
    assert_eq!(moved[1], ghosts[1], "wanted the second ghost where it stood, got {:?} from {:?}", moved[1], ghosts[1]);
}

#[test]
fn the_last_ghost_holds_its_tune_to_the_end() {
    let mut app = open(&project(&[(ROOT, 0.0), (THIRD, 2.0)], FIFTH));
    let (notes, ghosts) = painted_ghosts(&mut app);
    let row = row_h(notes[1], ghosts[1]);
    let from = centre(ghosts[1]);
    drag(&mut app, from, (from.0, from.1 - row));
    let project = remembered(&mut app);
    assert_eq!(held(&project, 0.0), FIFTH, "wanted the first note still at {FIFTH} st, got {} st", held(&project, 0.0));
    assert_eq!(held(&project, 2.0), FIFTH + 1, "wanted the second note held a semitone higher, got {} st", held(&project, 2.0));
    assert_eq!(held(&project, 3.75), FIFTH + 1, "wanted the last hold to run on to the end of the bar, got {} st at beat 3.75", held(&project, 3.75));
    let last = tune_curve(&project).and_then(|curve| curve.nodes.iter().map(|node| node.tick).max()).unwrap_or(0);
    assert_eq!(last, 2 * TICKS_PER_BEAT, "wanted no node after the last note's start, got one at tick {last}");
    let (_, moved) = painted_ghosts(&mut app);
    assert_eq!(moved[0], ghosts[0], "wanted the first ghost where it stood, got {:?} from {:?}", moved[0], ghosts[0]);
    assert_eq!(moved[1].y, ghosts[1].y - row, "wanted the second ghost a row higher, got {:?} from {:?}", moved[1], ghosts[1]);
}

#[test]
fn dragging_a_ghost_back_leaves_no_curve() {
    let mut app = open(&project(&[(ROOT, 0.0), (THIRD, 2.0)], FIFTH));
    let (notes, ghosts) = painted_ghosts(&mut app);
    let row = row_h(notes[0], ghosts[0]);
    let from = centre(ghosts[0]);
    drag(&mut app, from, (from.0, from.1 + 2 * row));
    assert!(tune_curve(&remembered(&mut app)).is_some(), "wanted the first drag to draw a TUNE curve");
    let (_, moved) = painted_ghosts(&mut app);
    let back = centre(moved[0]);
    drag(&mut app, back, (back.0, back.1 - 2 * row));
    let project = remembered(&mut app);
    assert!(tune_curve(&project).is_none(), "wanted the curve gone once the ghost sat back at the knob's {FIFTH} st, got {:?}", tune_curve(&project));
    assert_eq!(knob(&project), f64::from(FIFTH), "wanted the knob untouched, got {} st", knob(&project));
    let (_, returned) = painted_ghosts(&mut app);
    assert_eq!(returned, ghosts, "wanted both ghosts back where they began");
}

#[test]
fn a_hover_lights_one_ghost_alone() {
    let mut app = open(&project(&[(ROOT, 0.0), (THIRD, 2.0)], FIFTH));
    let (_, ghosts) = painted_ghosts(&mut app);
    let (x, y) = centre(ghosts[0]);
    app.event(Event::Move { x, y });
    let fills = painted(&mut app);
    assert!(fills.iter().any(|fill| fill.tone == Tone::Ink50 && fill.rect == ghosts[0]), "wanted the ghost under the pointer lit to Ink50, got no such fill at {:?}", ghosts[0]);
    assert!(fills.iter().any(|fill| fill.tone == Tone::Ink25 && fill.rect == ghosts[1]), "wanted the other note's ghost left at Ink25, got no such fill at {:?}", ghosts[1]);
}

#[test]
fn chord_ghosts_share_one_hold() {
    let mut app = open(&project(&[(ROOT, 0.0), (THIRD, 0.0)], FIFTH));
    let (notes, ghosts) = painted_ghosts(&mut app);
    assert_eq!((notes.len(), ghosts.len()), (2, 2), "wanted two notes and two ghosts at one start, got {notes:?} and {ghosts:?}");
    let row = row_h(notes[1], ghosts[1]);
    let from = centre(ghosts[1]);
    drag(&mut app, from, (from.0, from.1 + 2 * row));
    let (_, moved) = painted_ghosts(&mut app);
    assert_eq!(moved.len(), 2, "wanted both chord ghosts still painted, got {moved:?}");
    for (before, after) in ghosts.iter().zip(&moved) {
        assert_eq!(after.y, before.y + 2 * row, "wanted every ghost of the chord two rows lower, got {after:?} from {before:?}");
    }
    assert_eq!(held(&remembered(&mut app), 0.0), FIFTH - 2, "wanted the chord's one hold two semitones lower");
}

#[test]
fn a_drawn_curve_places_each_ghost_by_its_own_tune() {
    let mut project = project(&[(ROOT, 0.0), (THIRD, 2.0)], FIFTH);
    let nodes = vec![Node { tick: 0, travel: travel(FIFTH), bend: 0.0 }, Node { tick: 2 * TICKS_PER_BEAT - 1, travel: travel(FIFTH), bend: 0.0 }, Node { tick: 2 * TICKS_PER_BEAT, travel: travel(FIFTH - 2), bend: 0.0 }];
    project.tracks[0].automation.curves.push(Curve { aim: Aim::knob(CORE_ROW, BAND, TUNE), lfo: None, nodes });
    let mut app = open(&project);
    let (notes, ghosts) = painted_ghosts(&mut app);
    assert_eq!(ghosts.len(), 2, "wanted a ghost over each note, got {ghosts:?}");
    let row = row_h(notes[0], ghosts[0]);
    assert_eq!(notes[1].y - ghosts[1].y, (FIFTH - 2) * row, "wanted the second ghost {} rows up where its hold puts it, got {:?} over {:?}", FIFTH - 2, ghosts[1], notes[1]);
}

#[test]
fn the_rims_breathe_and_ask_to_repaint() {
    let mut app = open(&project(&[(ROOT, 0.0)], FIFTH));
    let (_, ghosts) = painted_ghosts(&mut app);
    let ghost = ghosts[0];
    assert_eq!(rim_fade(&painted(&mut app), ghost), (Tone::Ink50, Tone::Ink75, 0), "wanted the ghost's rim at Ink50 at rest, bound for Ink75 with none of the way gone");
    let asked = (1..=TICKS_TO_PEAK).filter(|_| app.tick(TICK)).count();
    assert_eq!(asked, TICKS_TO_PEAK, "wanted every quarter-second tick to ask for a repaint as the rim darkens, got {asked} of {TICKS_TO_PEAK}");
    assert_eq!(rim_fade(&painted(&mut app), ghost), (Tone::Ink50, Tone::Ink75, u8::MAX), "wanted the rim all the way to Ink75 at the peak of the breath, three quarters of a second in");
    for _ in 0..TICKS_TO_PEAK {
        app.tick(TICK);
    }
    assert_eq!(rim_fade(&painted(&mut app), ghost), (Tone::Ink50, Tone::Ink75, 0), "wanted the rim back at Ink50 after a full breath of a second and a half");
}

#[test]
fn a_still_roll_without_ghosts_asks_for_no_repaint() {
    let mut app = open(&project(&[(ROOT, 0.0)], 0));
    let (_, ghosts) = painted_ghosts(&mut app);
    assert!(ghosts.is_empty(), "wanted no ghosts with the oscillator at 0 st, got {ghosts:?}");
    let asked = (1..=TICKS_TO_FLIP).filter(|_| app.tick(TICK)).count();
    assert_eq!(asked, 0, "wanted no repaint asked for over {TICKS_TO_FLIP} ticks without ghosts, got {asked}");
}

#[test]
fn the_drag_runs_past_the_keys_to_the_range_end() {
    let mut app = open(&project(&[(ROOT, 0.0)], FIFTH));
    let (notes, ghosts) = painted_ghosts(&mut app);
    let row = row_h(notes[0], ghosts[0]);
    let from = centre(ghosts[0]);
    drag(&mut app, from, (from.0, from.1 - 60 * row));
    let project = remembered(&mut app);
    assert_eq!(held(&project, 0.0), TUNE_HIGH as i32, "wanted the hold clamped at {TUNE_HIGH} st far past the keys, got {} st", held(&project, 0.0));
}

#[test]
fn a_locked_project_keeps_its_ghosts_still() {
    let mut locked = project(&[(ROOT, 0.0), (THIRD, 2.0)], FIFTH);
    locked.lock = true;
    let mut app = open(&locked);
    let (notes, ghosts) = painted_ghosts(&mut app);
    let row = row_h(notes[0], ghosts[0]);
    let from = centre(ghosts[0]);
    drag(&mut app, from, (from.0, from.1 + 2 * row));
    let project = remembered(&mut app);
    assert!(tune_curve(&project).is_none(), "wanted no curve drawn while the project is locked, got {:?}", tune_curve(&project));
    let (_, still) = painted_ghosts(&mut app);
    assert_eq!(still, ghosts, "wanted the ghosts unmoved under the lock");
}
