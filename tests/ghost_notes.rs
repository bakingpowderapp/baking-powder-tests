use bp::app::{App, Event};
use bp::arp::{Arp, Articulation};
use bp::core::Params;
use bp::draw::{Fill, Rect, Tone};
use bp::face::{CELL, FRAME, PITCH};
use bp::json::Json;
use bp::project::{Aim, Clip, Note, Project, Row, TICKS_PER_BAR, VELOCITY};
use support::{CORE_ROW, HEIGHT, WIDTH, centre, clip_block, core_layer, midi, painted, remembered, session};

const KEYS: i32 = 2 * PITCH;
const RAIL: i32 = 45;
const STEP: usize = 2;
const SETTINGS_ITEM: i32 = 10;
const ITEM_H: i32 = 32;
const MENU_INSET: i32 = 12;
const COLUMN_X: i32 = 40;
const ROOT: u8 = 60;
const THIRD: u8 = 64;
const FIFTH: i32 = 7;
const FOURTH: i32 = 5;
const TUNE_LOW: f64 = -48.0;
const TUNE_HIGH: f64 = 48.0;

struct Grid {
    top: i32,
    right: i32,
}

fn note(pitch: u8, start: f32) -> Note {
    Note { pitch, start, length: 1.0, velocity: VELOCITY, articulation: Articulation::PLAIN }
}

fn project(pitches: &[u8], level: f64, tune: i32) -> Project {
    let mut patch = Params::DEFAULT;
    patch.osc2.level = level;
    patch.osc2.tune = f64::from(tune);
    let mut project = Project::new();
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch))];
    let notes = pitches.iter().enumerate().map(|(index, &pitch)| note(pitch, 2.0 * index as f32)).collect();
    project.tracks[0].clips.push(Clip { start: 0, length: TICKS_PER_BAR, label: String::new(), notes, file: None });
    project
}

fn open(project: &Project, ghosts: bool) -> App {
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false,\"ghostNotes\":{ghosts}}}", project.to_json().write());
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

fn tuned_to(project: &Project) -> f64 {
    match &project.tracks[0].rack[CORE_ROW] {
        Row::Core(layer) => layer.patch.osc2.tune,
        Row::Midi(_) | Row::Effect(_) | Row::Bus(_) => panic!("wanted the Core under the MIDI row"),
    }
}

fn pitches(project: &Project) -> Vec<u8> {
    let mut pitches: Vec<u8> = project.tracks[0].clips[0].notes.iter().map(|note| note.pitch).collect();
    pitches.sort_unstable();
    pitches
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
    found.sort_by_key(|rect| rect.x);
    found
}

fn ghosts(fills: &[Fill], grid: &Grid, notes: &[Rect]) -> Vec<Rect> {
    let mut found: Vec<Rect> = fills.iter().filter(|fill| fill.tone == Tone::Ink25 && block_like(fill, grid) && notes.iter().any(|note| note.x == fill.rect.x && note.w == fill.rect.w)).map(|fill| fill.rect).collect();
    found.sort_by_key(|rect| rect.x);
    found
}

fn painted_ghosts(app: &mut App) -> (Vec<Rect>, Vec<Rect>) {
    let fills = painted(app);
    let grid = grid(&fills);
    let notes = real_notes(&fills, &grid);
    let ghosts = ghosts(&fills, &grid, &notes);
    (notes, ghosts)
}

fn rail_wash(fills: &[Fill]) -> Option<Rect> {
    fills.iter().find(|fill| fill.tone == Tone::Ink25 && fill.rect.y < RAIL && fill.rect.h >= 30).map(|fill| fill.rect)
}

fn file_button(app: &mut App) -> Rect {
    rail_button(app, (0..WIDTH).step_by(STEP)).expect("wanted the File button at the left of the rail")
}

fn close_button(app: &mut App) -> Rect {
    rail_button(app, (0..WIDTH).rev().step_by(STEP)).expect("wanted the X button at the right of the rail")
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

fn ghost_control(app: &mut App) -> Rect {
    let mut washes: Vec<Rect> = Vec::new();
    for y in (RAIL..HEIGHT).step_by(STEP) {
        app.event(Event::Move { x: COLUMN_X, y });
        if let Some(rect) = painted(app).iter().find(|fill| fill.tone == Tone::Ink25 && fill.rect.y >= RAIL && fill.rect.contains(COLUMN_X, y)).map(|fill| fill.rect)
            && washes.last() != Some(&rect)
        {
            washes.push(rect);
        }
    }
    let [.., ghost, _numbers, _follow] = washes.as_slice() else {
        panic!("wanted the settings column to end in the ghost notes control, BAR NUMBERS and then FOLLOW, got {washes:?}");
    };
    *ghost
}

#[test]
fn a_tuned_oscillator_casts_a_ghost_above_every_note() {
    let mut app = open(&project(&[ROOT, THIRD], 1.0, FIFTH), true);
    let (notes, ghosts) = painted_ghosts(&mut app);
    assert_eq!(notes.len(), 2, "wanted the two notes inked in the grid, got {notes:?}");
    assert_eq!(ghosts.len(), notes.len(), "wanted a ghost for each note with osc 2 at +{FIFTH} st, got {ghosts:?} for {notes:?}");
    for (note, ghost) in notes.iter().zip(&ghosts) {
        assert!(ghost.x == note.x && ghost.w == note.w && ghost.y < note.y, "wanted the ghost over its note's span and above it, got {ghost:?} for {note:?}");
    }
}

#[test]
fn a_silent_or_untuned_oscillator_casts_nothing() {
    for (level, tune) in [(0.0, FIFTH), (1.0, 0)] {
        let mut app = open(&project(&[ROOT], level, tune), true);
        let (notes, ghosts) = painted_ghosts(&mut app);
        assert_eq!(notes.len(), 1, "wanted the note inked in the grid, got {notes:?}");
        assert!(ghosts.is_empty(), "wanted no ghost with osc 2 at level {level} and {tune} st, got {ghosts:?}");
    }
}

fn held_at_start(project: &Project) -> i32 {
    let curve = project.tracks[0].automation.curves.iter().find(|curve| curve.lfo.is_none() && matches!(&curve.aim, Aim::Knob { band, name, .. } if band == "OSC 2" && name == "TUNE")).expect("wanted the drag to hold osc 2's TUNE at the note");
    (TUNE_LOW + (TUNE_HIGH - TUNE_LOW) * curve.at(0.0)).round() as i32
}

#[test]
fn dragging_a_ghost_onto_its_note_holds_that_note_at_zero() {
    for tune in [FIFTH, -FOURTH] {
        let mut app = open(&project(&[ROOT], 1.0, tune), true);
        let (notes, ghosts) = painted_ghosts(&mut app);
        let (note, ghost) = (notes[0], ghosts[0]);
        assert!((ghost.y < note.y) == (tune > 0), "wanted the ghost {} the note at {tune} st, got {ghost:?} for {note:?}", if tune > 0 { "above" } else { "below" });
        let from = centre(ghost);
        drag(&mut app, from, (from.0, centre(note).1));
        let project = remembered(&mut app);
        assert_eq!(tuned_to(&project), f64::from(tune), "wanted the TUNE knob left at {tune} st by the drag, got {} st", tuned_to(&project));
        assert_eq!(held_at_start(&project), 0, "wanted the note's hold dragged from {tune} st to 0 st, got {} st", held_at_start(&project));
        assert_eq!(pitches(&project), vec![ROOT], "wanted the clip's notes untouched by the drag, got {:?}", pitches(&project));
        let (_, left) = painted_ghosts(&mut app);
        assert!(left.is_empty(), "wanted the ghosts gone at 0 st, got {left:?}");
    }
}

#[test]
fn a_tap_on_a_ghost_lays_a_real_note_there_and_keeps_the_tuning() {
    let mut app = open(&project(&[ROOT], 1.0, FIFTH), true);
    let (_, ghosts) = painted_ghosts(&mut app);
    click(&mut app, centre(ghosts[0]));
    let project = remembered(&mut app);
    let wanted = vec![ROOT, ROOT + FIFTH as u8];
    assert_eq!(pitches(&project), wanted, "wanted a real note laid where the ghost stood, got {:?}", pitches(&project));
    assert_eq!(tuned_to(&project), f64::from(FIFTH), "wanted the tuning kept by a tap, got {} st", tuned_to(&project));
}

#[test]
fn the_setting_hides_the_ghosts_from_the_session_and_from_the_panel() {
    let mut app = open(&project(&[ROOT], 1.0, FIFTH), false);
    let (_, hidden) = painted_ghosts(&mut app);
    assert!(hidden.is_empty(), "wanted no ghosts with the setting off in the session, got {hidden:?}");
    let mut app = open(&project(&[ROOT], 1.0, FIFTH), true);
    let file = file_button(&mut app);
    click(&mut app, centre(file));
    click(&mut app, (file.x + MENU_INSET, file.y + file.h + SETTINGS_ITEM * ITEM_H + ITEM_H / 2));
    let control = ghost_control(&mut app);
    click(&mut app, centre(control));
    let flag = session(&mut app).get("ghostNotes").and_then(Json::as_bool).unwrap_or(true);
    assert!(!flag, "wanted the ghost notes control, above BAR NUMBERS and FOLLOW at the foot of the settings column, to turn the ghost notes off, got the setting still on");
    let close = close_button(&mut app);
    click(&mut app, centre(close));
    let (notes, gone) = painted_ghosts(&mut app);
    assert_eq!(notes.len(), 1, "wanted the roll back with its note after closing the settings, got {notes:?}");
    assert!(gone.is_empty(), "wanted no ghosts once the setting is off, got {gone:?}");
}
