use bp::app::{App, Event, Request};
use bp::arp::Articulation;
use bp::draw::{Fill, Rect, Tone};
use bp::face::{CELL, FRAME, PITCH};
use bp::json;
use bp::project::{Clip, Note, Project, TICKS_PER_BEAT, VELOCITY};
use support::{clip_block, painted};

const KEYS: i32 = 2 * PITCH;
const HEADER_CELLS: usize = 9;
const OCTAVE: u8 = 12;
const TRIAD: [u8; 3] = [60, 64, 67];
const TOP: u8 = 107;
const LOW: u8 = 12;

fn note(pitch: u8, start: f32) -> Note {
    Note { pitch, start, length: 1.0, velocity: VELOCITY, articulation: Articulation::PLAIN }
}

fn open(pitches: &[u8]) -> App {
    let mut project = Project::new();
    let notes = pitches.iter().enumerate().map(|(index, &pitch)| note(pitch, index as f32)).collect();
    project.tracks[0].clips.push(Clip { start: 0, length: 4 * TICKS_PER_BEAT, label: String::from("line"), notes, file: None });
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false}}", project.to_json().write());
    let mut app = App::new();
    app.resume(&session);
    let clip = clip_block(&painted(&mut app));
    click(&mut app, clip);
    app
}

fn click(app: &mut App, rect: Rect) {
    let (x, y) = (rect.x + rect.w / 2, rect.y + rect.h / 2);
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    app.event(Event::Leave);
}

fn header_cells(fills: &[Fill]) -> [Rect; HEADER_CELLS] {
    let edges: Vec<Rect> = fills.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.w == KEYS + FRAME && fill.rect.h == FRAME).map(|fill| fill.rect).collect();
    let bottom = edges.iter().map(|edge| edge.y).max().expect("wanted the roll's header cells framed");
    let top = bottom - (CELL - FRAME);
    let mut xs: Vec<i32> = edges.iter().filter(|edge| edge.y == top).map(|edge| edge.x).collect();
    xs.sort_unstable();
    let right: Vec<i32> = xs.iter().rev().take(HEADER_CELLS).rev().copied().collect();
    assert!(right.len() == HEADER_CELLS && right.windows(2).all(|pair| pair[1] - pair[0] == KEYS), "wanted {HEADER_CELLS} header cells side by side at the roll's right, got cell edges at {xs:?}");
    std::array::from_fn(|index| Rect::new(right[index], top, KEYS + FRAME, CELL))
}

fn pitches(app: &mut App) -> Vec<u8> {
    let text = app.take_requests().into_iter().rev().find_map(|request| if let Request::Remember { json } = request { Some(json) } else { None }).expect("wanted the session remembered after the press");
    let session = json::parse(&text).expect("wanted the remembered session to parse");
    let project = Project::from_json(session.get("project").expect("wanted the remembered session to hold the project")).expect("wanted the remembered project to read");
    project.tracks[0].clips[0].notes.iter().map(|note| note.pitch).collect()
}

#[test]
fn the_octave_cells_move_every_note_of_the_clip_an_octave() {
    let mut app = open(&TRIAD);
    let [_, _, up, down, ..] = header_cells(&painted(&mut app));
    assert!(up.x < down.x, "wanted +OCT to the left of −OCT, after the ×2 and /2 cells, got {up:?} and {down:?}");
    click(&mut app, up);
    let raised = pitches(&mut app);
    assert_eq!(raised, TRIAD.map(|pitch| pitch + OCTAVE).to_vec(), "wanted every note an octave up after +OCT, got {raised:?}");
    click(&mut app, down);
    click(&mut app, down);
    let lowered = pitches(&mut app);
    assert_eq!(lowered, TRIAD.map(|pitch| pitch - OCTAVE).to_vec(), "wanted every note an octave below where it began after two −OCT presses, got {lowered:?}");
}

#[test]
fn a_note_at_the_edge_of_the_roll_keeps_the_clip_where_it_is() {
    let mut app = open(&[TOP - OCTAVE - 1, TOP]);
    let [_, _, up, ..] = header_cells(&painted(&mut app));
    click(&mut app, up);
    let kept = pitches(&mut app);
    assert_eq!(kept, vec![TOP - OCTAVE - 1, TOP], "wanted +OCT refused with a note already at the top key, got {kept:?}");
    let mut app = open(&[LOW, LOW + OCTAVE + 1]);
    let [_, _, _, down, ..] = header_cells(&painted(&mut app));
    click(&mut app, down);
    let kept = pitches(&mut app);
    assert_eq!(kept, vec![LOW, LOW + OCTAVE + 1], "wanted −OCT refused with a note already at the bottom key, got {kept:?}");
}
