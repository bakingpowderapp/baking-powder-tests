use bp::app::{App, Event};
use bp::arp::Articulation;
use bp::draw::{Fill, Rect, Tone};
use bp::face::{CELL, FRAME, PITCH};
use bp::project::{Clip, Note, Project, TICKS_PER_BAR, TICKS_PER_BEAT, VELOCITY};
use support::{clip_block, painted};

const KEYS: i32 = 2 * PITCH;
const HEADER_CELLS: usize = 4;
const CHORD: [u8; 3] = [62, 65, 69];
const MELODY: u8 = 66;
const CHORD_BEATS: i32 = 8;
const MELODY_BEATS: i32 = 16;

fn note(pitch: u8, start: f32, length: f32) -> Note {
    Note { pitch, start, length, velocity: VELOCITY, articulation: Articulation::PLAIN }
}

fn open() -> App {
    let mut project = Project::new();
    let chord = CHORD.iter().map(|&pitch| note(pitch, 0.0, 4.0)).collect();
    project.tracks[0].clips.push(Clip { start: 0, length: CHORD_BEATS * TICKS_PER_BEAT, label: String::from("chords"), notes: chord, file: None });
    project.tracks[1].clips.push(Clip { start: 0, length: MELODY_BEATS * TICKS_PER_BEAT, label: String::from("melody"), notes: vec![note(MELODY, 0.0, 1.0)], file: None });
    let session = format!("{{\"project\":{},\"palette\":false}}", project.to_json().write());
    let mut app = App::new();
    app.resume(&session);
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

fn guide_notes(fills: &[Fill], header: Rect) -> usize {
    fills.iter().filter(|fill| fill.tone == Tone::Ink50 && fill.rect.y >= header.y + CELL && fill.rect.w > FRAME).count()
}

#[test]
fn the_guide_lays_the_other_clips_notes_under_the_grid_while_it_is_on() {
    let mut app = open();
    let melody = clip_block(&painted(&mut app));
    click(&mut app, melody);
    let fills = painted(&mut app);
    let [guide, ..] = header_cells(&fills);
    let before = guide_notes(&fills, guide);
    assert_eq!(before, 0, "wanted no guide notes with GUIDE off, got {before}");
    click(&mut app, guide);
    let shown = guide_notes(&painted(&mut app), guide);
    assert_eq!(shown, CHORD.len(), "wanted the {} chord notes laid in the grid with GUIDE on, got {shown}", CHORD.len());
    click(&mut app, guide);
    let after = guide_notes(&painted(&mut app), guide);
    assert_eq!(after, 0, "wanted the guide notes gone with GUIDE off again, got {after}");
}

#[test]
fn the_guide_takes_what_other_clips_sound_across_the_clip_cut_to_its_edges() {
    let mut project = Project::new();
    let chords = vec![note(60, 0.0, 2.0), note(62, 2.0, 4.0), note(64, 6.0, 4.0)];
    project.tracks[0].clips.push(Clip { start: 0, length: 8 * TICKS_PER_BEAT, label: String::from("chords"), notes: chords, file: None });
    project.tracks[1].clips.push(Clip { start: 0, length: TICKS_PER_BAR, label: String::from("earlier"), notes: vec![note(48, 0.0, 4.0)], file: None });
    project.tracks[1].clips.push(Clip { start: TICKS_PER_BAR, length: TICKS_PER_BAR, label: String::from("melody"), notes: vec![note(72, 1.0, 1.0)], file: None });
    let beside = project.beside(1, &project.tracks[1].clips[1]);
    let got: Vec<(u8, f32, f32)> = beside.iter().map(|note| (note.pitch, note.start, note.length)).collect();
    let wanted = vec![(62, 0.0, 2.0), (64, 2.0, 2.0)];
    assert_eq!(got, wanted, "wanted the chord notes crossing beats 4 to 8 in the melody's own beats, cut to its edges, as {wanted:?}, got {got:?}");
}
