use bp::app::{App, Event};
use bp::arp::{Arp, Articulation};
use bp::audio::Renderer;
use bp::core::Params;
use bp::core::params::Envelope;
use bp::draw::{Fill, Rect, Tone};
use bp::face::{CELL, FRAME, PITCH};
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{AUDIBLE, BLOCK, SAMPLE_RATE, SILENT, clip_block, core_layer, midi, painted, peak_of};

const KEYS: i32 = 2 * PITCH;
const HEADER_CELLS: usize = 4;
const ROW: i32 = 12;
const NOTE_W_AT_LEAST: i32 = 30;
const LISTEN: usize = 8;
const FADE: usize = 200;

fn open() -> (App, Renderer) {
    let mut patch = Params::DEFAULT;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    let mut project = Project::new();
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch))];
    project.tracks[0].clips.push(Clip { start: 0, length: 16 * TICKS_PER_BEAT, label: String::from("line"), notes: vec![Note { pitch: 60, start: 4.0, length: 1.0, velocity: VELOCITY, articulation: Articulation::PLAIN }], file: None });
    let session = format!("{{\"project\":{},\"palette\":false}}", project.to_json().write());
    let mut app = App::new();
    app.resume(&session);
    let renderer = app.audio_started(SAMPLE_RATE, BLOCK, BLOCK as u32);
    let clip = clip_block(&painted(&mut app));
    click(&mut app, clip);
    (app, renderer)
}

fn click(app: &mut App, rect: Rect) {
    let (x, y) = (rect.x + rect.w / 2, rect.y + rect.h / 2);
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    app.event(Event::Leave);
}

fn blocks(renderer: &mut Renderer, count: usize) -> Vec<f32> {
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(count * BLOCK);
    for _ in 0..count {
        renderer.render(&mut l, &mut r);
        out.extend_from_slice(&l);
    }
    out
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

fn note_block(fills: &[Fill], header: Rect) -> Rect {
    fills.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.y >= header.y + CELL && fill.rect.h == ROW - FRAME && fill.rect.w >= NOTE_W_AT_LEAST).map(|fill| fill.rect).next().expect("wanted the note painted on the roll")
}

fn pressed_peak(app: &mut App, renderer: &mut Renderer, note: Rect) -> f32 {
    let (x, y) = (note.x + note.w / 2, note.y + note.h / 2);
    app.event(Event::Down { x, y, clicks: 1 });
    let heard = peak_of(&blocks(renderer, LISTEN));
    app.event(Event::Up { x, y });
    let rest = blocks(renderer, FADE);
    let settled = peak_of(&rest[rest.len() - BLOCK..]);
    assert!(settled <= SILENT, "wanted the released note faded within {FADE} blocks, got a peak of {settled} in the last block");
    heard
}

#[test]
fn preview_off_keeps_a_pressed_note_silent_and_on_sounds_it_again() {
    let (mut app, mut renderer) = open();
    let fills = painted(&mut app);
    let [_, preview, ..] = header_cells(&fills);
    let note = note_block(&fills, preview);
    let heard = pressed_peak(&mut app, &mut renderer, note);
    assert!(heard > AUDIBLE, "wanted a pressed note heard with PREVIEW on, got a peak of {heard}");
    click(&mut app, preview);
    let quiet = pressed_peak(&mut app, &mut renderer, note);
    assert!(quiet <= SILENT, "wanted no sound from a pressed note with PREVIEW off, got a peak of {quiet}");
    click(&mut app, preview);
    let again = pressed_peak(&mut app, &mut renderer, note);
    assert!(again > AUDIBLE, "wanted the pressed note heard once PREVIEW is on again, got a peak of {again}");
}
