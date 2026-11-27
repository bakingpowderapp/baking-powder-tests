use bp::app::{App, Event, Request};
use bp::arp::Articulation;
use bp::draw::{Fill, Rect, Tone};
use bp::face::{CELL, FRAME, PITCH};
use bp::json;
use bp::project::{Clip, Note, Project, TICKS_PER_BAR, TICKS_PER_BEAT, VELOCITY};
use support::{WIDTH, clip_block, painted};

const KEYS: i32 = 2 * PITCH;
const TIMES_TWO: i32 = 9;
const OVER_TWO: i32 = 8;
const SWEEP: usize = 8;
const ONE_BAR: i32 = TICKS_PER_BAR;
const TWO_BARS: i32 = 2 * TICKS_PER_BAR;

fn note(pitch: u8, start: f32, length: f32) -> Note {
    Note { pitch, start, length, velocity: VELOCITY, articulation: Articulation::PLAIN }
}

fn phrase() -> Vec<Note> {
    vec![note(60, 0.0, 2.0), note(64, 2.0, 2.0), note(67, 4.0, 4.0)]
}

fn clip(start: i32, length: i32, notes: Vec<Note>) -> Clip {
    Clip { start, length, label: String::from("phrase"), notes, file: None }
}

fn open(clips: Vec<Clip>) -> App {
    let mut project = Project::new();
    project.tracks[0].clips = clips;
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false,\"snap\":\"1/4\"}}", project.to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    let block = clip_block(&painted(&mut app));
    click(&mut app, block.x + block.w / 2, block.y + block.h / 2);
    app.take_requests();
    app
}

fn click(app: &mut App, x: i32, y: i32) {
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    app.event(Event::Leave);
}

fn roll_top(fills: &[Fill]) -> i32 {
    let tallest = fills.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.w == FRAME).map(|fill| fill.rect.h).max().expect("wanted the roll's lines painted");
    let line = fills.iter().find(|fill| fill.tone == Tone::Ink100 && fill.rect.w == FRAME && fill.rect.h == tallest).expect("wanted a line of the roll").rect;
    line.y - PITCH
}

fn washes(fills: &[Fill], row: Rect) -> Vec<Rect> {
    fills.iter().filter(|fill| fill.tone == Tone::Ink25 && row.contains(fill.rect.x, fill.rect.y)).map(|fill| fill.rect).collect()
}

fn scale_cells(app: &mut App) -> (Rect, Rect) {
    let top = roll_top(&painted(app));
    let row = Rect::new(0, top, WIDTH, CELL);
    app.event(Event::Leave);
    let quiet = washes(&painted(app), row);
    let mut found: Vec<Rect> = Vec::new();
    for x in (0..WIDTH).step_by(SWEEP) {
        app.event(Event::Move { x, y: top + CELL / 2 });
        for rect in washes(&painted(app), row) {
            if !quiet.contains(&rect) && !found.contains(&rect) {
                found.push(rect);
            }
        }
    }
    app.event(Event::Leave);
    let big = found.iter().copied().max_by_key(|rect| rect.x).expect("wanted the roll's header cells to wash under the pointer");
    let left_of_big = |cells: i32| Rect::new(big.x - cells * KEYS, big.y, big.w, big.h);
    let (times_two, over_two) = (left_of_big(TIMES_TWO), left_of_big(OVER_TWO));
    assert!(found.contains(&times_two) && found.contains(&over_two), "wanted ×2 and /2 washing {TIMES_TWO} and {OVER_TWO} cells left of BIG, got {found:?}");
    (times_two, over_two)
}

fn remembered(app: &mut App) -> Option<Project> {
    let text = app.take_requests().into_iter().rev().find_map(|request| if let Request::Remember { json } = request { Some(json) } else { None })?;
    let session = json::parse(&text).expect("wanted the remembered session to parse");
    Some(Project::from_json(session.get("project").expect("wanted the remembered session to hold the project")).expect("wanted the remembered project to read"))
}

fn press(app: &mut App, cell: Rect) -> Option<Project> {
    click(app, cell.x + cell.w / 2, cell.y + cell.h / 2);
    remembered(app)
}

fn laid(project: &Project, index: usize) -> (i32, Vec<(f32, f32)>) {
    let clip = &project.tracks[0].clips[index];
    (clip.length, clip.notes.iter().map(|note| (note.start, note.length)).collect())
}

#[test]
fn times_two_doubles_every_note_and_the_clip_with_them() {
    let mut app = open(vec![clip(0, TWO_BARS, phrase())]);
    let (times_two, _) = scale_cells(&mut app);
    let project = press(&mut app, times_two).expect("wanted ×2 to change the clip");
    assert_eq!(laid(&project, 0), (2 * TWO_BARS, vec![(0.0, 4.0), (4.0, 4.0), (8.0, 8.0)]), "wanted ×2 to double each note's start and length and the clip to four bars, got {:?}", laid(&project, 0));
}

#[test]
fn over_two_halves_them_and_times_two_brings_them_back() {
    let mut app = open(vec![clip(0, TWO_BARS, phrase())]);
    let (times_two, over_two) = scale_cells(&mut app);
    let halved = press(&mut app, over_two).expect("wanted /2 to change the clip");
    assert_eq!(laid(&halved, 0), (ONE_BAR, vec![(0.0, 1.0), (1.0, 1.0), (2.0, 2.0)]), "wanted /2 to halve each note and the clip to one bar, got {:?}", laid(&halved, 0));
    let back = press(&mut app, times_two).expect("wanted ×2 to change the clip");
    assert_eq!(laid(&back, 0), (TWO_BARS, vec![(0.0, 2.0), (2.0, 2.0), (4.0, 4.0)]), "wanted ×2 after /2 to bring the phrase back as it was, got {:?}", laid(&back, 0));
}

#[test]
fn times_two_does_nothing_where_a_following_clip_is_in_the_way() {
    let mut app = open(vec![clip(0, TWO_BARS, phrase()), clip(TWO_BARS, ONE_BAR, vec![note(72, 0.0, 4.0)])]);
    let (times_two, _) = scale_cells(&mut app);
    assert!(press(&mut app, times_two).is_none(), "wanted ×2 to change nothing with a clip in the way of the doubled length");
}

#[test]
fn over_two_stops_short_of_a_beat() {
    let mut app = open(vec![clip(0, TWO_BARS, vec![note(60, 0.0, 8.0)])]);
    let (_, over_two) = scale_cells(&mut app);
    press(&mut app, over_two).expect("wanted the first /2 to change the clip");
    press(&mut app, over_two).expect("wanted the second /2 to change the clip");
    let beat = press(&mut app, over_two).expect("wanted the third /2 to change the clip");
    assert_eq!(laid(&beat, 0), (TICKS_PER_BEAT, vec![(0.0, 1.0)]), "wanted three halvings to leave a one-beat clip holding a one-beat note, got {:?}", laid(&beat, 0));
    assert!(press(&mut app, over_two).is_none(), "wanted a fourth /2 to leave the one-beat clip alone");
}
