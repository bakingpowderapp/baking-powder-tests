use bp::app::{App, Event};
use bp::draw::{Fill, Rect, Tone};
use bp::json::Json;
use bp::project::{Clip, Project, TICKS_PER_BAR};
use support::{WIDTH, clip_block, painted, remembered, session};

const BARS: i32 = 5;
const LENGTH: i32 = BARS * TICKS_PER_BAR;
const LINE: i32 = 2;
const OCTAVE: usize = 12;

fn open(palette: bool) -> App {
    let mut project = Project::new();
    project.tracks[0].clips.push(Clip { start: 0, length: LENGTH, label: String::from("take"), notes: Vec::new(), file: None });
    let session = format!("{{\"project\":{},\"palette\":{palette}}}", project.to_json().write());
    let mut app = App::new();
    app.resume(&session);
    app
}

fn click(app: &mut App, rect: Rect, clicks: i32) {
    let (x, y) = (rect.x + rect.w / 2, rect.y + rect.h / 2);
    app.event(Event::Down { x, y, clicks });
    app.event(Event::Up { x, y });
}

fn palette_flag(app: &mut App) -> bool {
    session(app).get("palette").and_then(Json::as_bool).unwrap_or(false)
}

fn lattice_rows(fills: &[Fill]) -> usize {
    fills.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.h <= LINE && fill.rect.w >= WIDTH * 3 / 4).count()
}

#[test]
fn a_double_click_leaves_the_clip_whole() {
    let mut app = open(false);
    let clip = clip_block(&painted(&mut app));
    click(&mut app, clip, 1);
    click(&mut app, clip, 2);
    let length = remembered(&mut app).tracks[0].clips[0].length;
    assert_eq!(length, LENGTH, "wanted the clip still {BARS} bars long after a double-click, got {length} ticks");
}

#[test]
fn clicking_a_clip_with_the_palette_up_hides_it_for_the_roll() {
    let mut app = open(true);
    let clip = clip_block(&painted(&mut app));
    click(&mut app, clip, 1);
    assert!(!palette_flag(&mut app), "wanted the palette hidden after clicking the clip with it up");
    let rows = lattice_rows(&painted(&mut app));
    assert!(rows >= OCTAVE, "wanted the roll's lattice across the stage after clicking the clip with the palette up, got {rows} rows");
}
