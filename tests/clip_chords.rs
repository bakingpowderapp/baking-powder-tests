use bp::app::{App, Event};
use bp::arp::Articulation;
use bp::draw::{DrawList, Fill, Rect, Tone};
use bp::font;
use bp::project::{Clip, Note, Project, VELOCITY};
use support::{NO_END, WIDTH, clip_block, painted};

const PALETTE_SCALE: f32 = 1.4;
const INSET: i32 = 3;
const TICK: f32 = 1.0 / 60.0;
const EDGE_H: i32 = 3;
const CELL_W_AT_LEAST: i32 = 150;
const HEADER_H: i32 = 30;
const HEADER_GAP: i32 = 3;
const RAIL: i32 = 45;
const WASH_H: i32 = 30;
const STEP: usize = 2;
const CHORD_BUTTON: usize = 12;
const SPANS: [(f32, f32); 3] = [(0.0, 16.0), (16.0, 24.0), (24.0, 32.0)];
const FIRST: [u8; 4] = [49, 56, 61, 64];
const SECOND: [u8; 5] = [52, 56, 59, 64, 66];
const THIRD: [u8; 4] = [54, 58, 63, 66];
const TRIAD: [u8; 3] = [60, 64, 67];
const FOURTH: [u8; 3] = [65, 69, 72];
const FIFTH: [u8; 3] = [67, 71, 74];

fn note(pitch: u8, start: f32, length: f32) -> Note {
    Note { pitch, start, length, velocity: VELOCITY, articulation: Articulation::PLAIN }
}

fn hit(pitches: &[u8], start: f32, length: f32) -> Vec<Note> {
    pitches.iter().map(|&pitch| note(pitch, start, length)).collect()
}

fn progression() -> Vec<Note> {
    let mut notes = Vec::new();
    for start in [0.0, 4.0, 8.0, 12.0] {
        notes.extend(hit(&FIRST, start, 4.0));
    }
    for start in [16.0, 20.0] {
        notes.extend(hit(&SECOND, start, 4.0));
    }
    notes.extend(hit(&THIRD, 24.0, 8.0));
    notes
}

fn cadence(chords: &[&[u8]]) -> Vec<Note> {
    chords.iter().enumerate().flat_map(|(index, pitches)| hit(pitches, 4.0 * index as f32, 4.0)).collect()
}

fn spans(clip: &Clip) -> Vec<(f32, f32)> {
    clip.chords().iter().map(|chord| (chord.start, chord.end)).collect()
}

fn click(app: &mut App, x: i32, y: i32) {
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    app.event(Event::Leave);
}

fn chord_button(app: &mut App) -> Rect {
    let mut washes: Vec<Rect> = Vec::new();
    for x in (0..WIDTH).step_by(STEP) {
        app.event(Event::Move { x, y: RAIL / 2 });
        if let Some(wash) = painted(app).iter().find(|fill| fill.tone == Tone::Ink25 && fill.rect.y < RAIL && fill.rect.h >= WASH_H).map(|fill| fill.rect)
            && washes.last() != Some(&wash)
        {
            washes.push(wash);
        }
    }
    washes.get(CHORD_BUTTON).copied().unwrap_or_else(|| panic!("wanted CHORD among the header's buttons, found {washes:?}"))
}

fn open(notes: Vec<Note>) -> App {
    let mut project = Project::new();
    project.tracks[0].clips.push(Clip::midi(0, String::from("changes"), notes, NO_END));
    let session = format!("{{\"project\":{},\"keyboard\":false}}", project.to_json().write());
    let mut app = App::new();
    app.resume(&session);
    let clip = clip_block(&painted(&mut app));
    click(&mut app, clip.x + clip.w / 2, clip.y + clip.h / 2);
    let chord = chord_button(&mut app);
    click(&mut app, chord.x + chord.w / 2, chord.y + chord.h / 2);
    app
}

fn palette_rect(fills: &[Fill]) -> Rect {
    fills.iter().filter(|fill| fill.tone == Tone::Ground && fill.rect.w < WIDTH).map(|fill| fill.rect).max_by_key(|rect| rect.w * rect.h).expect("wanted the palette's ground painted on the stage")
}

fn inside(outer: Rect, inner: Rect) -> bool {
    outer.intersect(inner) == inner
}

fn cells(fills: &[Fill], palette: Rect) -> Vec<Rect> {
    let mut edges: Vec<Rect> = fills.iter().filter(|fill| matches!(fill.tone, Tone::Ink25 | Tone::Ink100) && fill.rect.h <= EDGE_H && fill.rect.w >= CELL_W_AT_LEAST && inside(palette, fill.rect)).map(|fill| fill.rect).collect();
    edges.sort_by_key(|edge| (edge.x, edge.w, edge.y));
    let mut cells: Vec<Rect> = edges
        .chunk_by(|a, b| (a.x, a.w) == (b.x, b.w))
        .flat_map(|column| {
            assert!(column.len() % 2 == 0, "wanted every cell's top edge matched by a bottom edge, got {column:?}");
            column.chunks(2).map(|pair| Rect::new(pair[0].x, pair[0].y, pair[0].w, pair[1].y + pair[1].h - pair[0].y)).collect::<Vec<Rect>>()
        })
        .collect();
    cells.sort_by_key(|cell| (cell.y, cell.x));
    cells
}

fn units(palette: Rect, cell: Rect) -> Rect {
    let unit = |device: i32, origin: i32| ((device - origin) as f32 / PALETTE_SCALE).round() as i32;
    let (x, y) = (unit(cell.x, palette.x), unit(cell.y, palette.y));
    Rect::new(x, y, unit(cell.x + cell.w, palette.x) - x, unit(cell.y + cell.h, palette.y) - y)
}

fn number_fills(palette: Rect, cell: Rect, places: &str) -> Vec<Rect> {
    let header = cell.h >= HEADER_H;
    let cell = units(palette, cell);
    let line = if header { Rect::new(cell.x, cell.y + (cell.h - (3 * font::GLYPH_H + HEADER_GAP)) / 2, cell.w, 2 * font::GLYPH_H) } else { cell };
    let mut list = DrawList::new();
    list.transform((palette.x, palette.y), PALETTE_SCALE);
    list.text(cell.x + cell.w - INSET - font::width(places, 1), line.y + (line.h - font::GLYPH_H) / 2, places, Tone::Ink100, 1);
    list.fills().iter().map(|fill| fill.rect).collect()
}

fn shows(fills: &[Fill], palette: Rect, cell: Rect, places: &str) -> bool {
    number_fills(palette, cell, places).iter().all(|wanted| fills.iter().any(|fill| fill.tone == Tone::Ink100 && fill.rect == *wanted))
}

fn candidates(count: usize) -> Vec<String> {
    (1..1u32 << count).map(|mask| (1..=count).filter(|place| mask & (1 << (place - 1)) != 0).map(|place| place.to_string()).collect::<Vec<String>>().join(" ")).collect()
}

fn numbered(fills: &[Fill], count: usize) -> Vec<(Rect, String)> {
    let palette = palette_rect(fills);
    cells(fills, palette).into_iter().filter_map(|cell| candidates(count).into_iter().filter(|places| shows(fills, palette, cell, places)).max_by_key(String::len).map(|places| (cell, places))).collect()
}

fn beneath(numbered: &[(Rect, String)], header: Rect) -> &(Rect, String) {
    numbered.iter().filter(|(cell, _)| (cell.x, cell.w) == (header.x, header.w) && cell.y > header.y).min_by_key(|(cell, _)| cell.y).unwrap_or_else(|| panic!("wanted a numbered row beneath the numbered header {header:?}"))
}

#[test]
fn a_clip_counts_its_chords_in_order_and_hears_a_repeated_chord_as_one() {
    let clip = Clip::midi(0, String::from("III"), progression(), NO_END);
    let chords = clip.chords();
    let found = spans(&clip);
    assert_eq!(found, SPANS.to_vec(), "wanted three chords over beats 0–16, 16–24 and 24–32, got {found:?}");
    let pitches: Vec<Vec<u8>> = chords.iter().map(|chord| chord.pitches.clone()).collect();
    assert_eq!(pitches, vec![FIRST.to_vec(), SECOND.to_vec(), THIRD.to_vec()], "wanted each chord to hold its own notes, got {pitches:?}");
}

#[test]
fn single_notes_and_octaves_make_no_chord() {
    let kick = Clip::midi(0, String::from("kick"), vec![note(25, 0.0, 16.0), note(28, 16.0, 8.0), note(30, 24.0, 8.0)], NO_END);
    let found = spans(&kick);
    assert!(found.is_empty(), "wanted no chords in a line of single notes, got {found:?}");
    let sub = Clip::midi(0, String::from("sub"), hit(&[36, 48], 0.0, 4.0), NO_END);
    let found = spans(&sub);
    assert!(found.is_empty(), "wanted no chord from an octave alone, got {found:?}");
}

#[test]
fn a_rest_keeps_a_repeated_chord_whole_and_a_new_chord_starts_another() {
    let mut notes = hit(&TRIAD, 0.0, 1.5);
    notes.extend(hit(&TRIAD, 2.0, 1.5));
    notes.extend(hit(&FOURTH, 4.0, 2.0));
    notes.extend(hit(&TRIAD, 6.0, 2.0));
    let found = spans(&Clip::midi(0, String::from("changes"), notes, NO_END));
    assert_eq!(found, vec![(0.0, 3.5), (4.0, 6.0), (6.0, 8.0)], "wanted the rested repeat counted once, then IV, then I again, got {found:?}");
}

#[test]
fn the_palette_up_numbers_a_picked_clips_chords_in_order_at_their_right() {
    let mut app = open(cadence(&[&TRIAD, &FOURTH, &FIFTH]));
    let fills = painted(&mut app);
    let numbered = numbered(&fills, SPANS.len());
    let headers: Vec<&(Rect, String)> = numbered.iter().filter(|(cell, _)| cell.h >= HEADER_H).collect();
    assert_eq!(headers.len(), 3, "wanted the I, IV and V headers numbered, got {headers:?}");
    for (index, (header, places)) in headers.iter().enumerate() {
        let wanted = (index + 1).to_string();
        assert_eq!(*places, wanted, "wanted the header at x {} to read {wanted} at its right, got {places}", header.x);
        let (row, read) = beneath(&numbered, *header);
        assert_eq!(*read, wanted, "wanted the triad row {row:?} beneath it to read {wanted}, got {read}");
    }
    for (cell, places) in &numbered {
        assert_eq!(places.len(), 1, "wanted every numbered cell, a spelling of I, IV or V such as Em(♭6) for C, to carry one place, got {places} on {cell:?}");
    }
    assert!(!app.tick(TICK), "wanted no redraw asked for by the numbers alone");
}

#[test]
fn a_chord_heard_twice_wears_both_its_places() {
    let mut app = open(cadence(&[&TRIAD, &FOURTH, &TRIAD]));
    let numbered = numbered(&painted(&mut app), SPANS.len());
    let headers: Vec<&(Rect, String)> = numbered.iter().filter(|(cell, _)| cell.h >= HEADER_H).collect();
    let read: Vec<&str> = headers.iter().map(|(_, places)| places.as_str()).collect();
    assert_eq!(read, ["1 3", "2"], "wanted the I header to read 1 3 and the IV header 2, got {read:?}");
    for (header, places) in &headers {
        let (row, read) = beneath(&numbered, *header);
        assert_eq!(read, places, "wanted the row {row:?} beneath the header to read {places}, got {read}");
    }
}

#[test]
fn a_line_of_single_notes_numbers_nothing() {
    let mut app = open(vec![note(60, 0.0, 4.0), note(64, 4.0, 4.0), note(67, 8.0, 4.0)]);
    let found = numbered(&painted(&mut app), SPANS.len());
    assert!(found.is_empty(), "wanted no cell numbered for a melody, found {found:?}");
    assert!(!app.tick(TICK), "wanted no redraw asked for with nothing to number");
}
