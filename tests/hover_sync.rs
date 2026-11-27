use bp::app::{App, Event};
use bp::arp::{Arp, Articulation};
use bp::audio::Renderer;
use bp::core::Params;
use bp::core::params::Envelope;
use bp::draw::{Fill, Rect, Tone};
use bp::keyboard::Key;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{AUDIBLE, BLOCK, HEIGHT, SAMPLE_RATE, SILENT, WIDTH, clip_block, core_layer, midi, painted, peak_of};

const RAIL: i32 = 45;
const TILE: i32 = 120;
const VOL_W: i32 = 28;
const BORDER: i32 = 8;
const COLUMN_W: i32 = TILE + VOL_W + BORDER;
const HANDLE_H: i32 = 6;
const BEATS: i32 = 64;
const SETTLE: usize = 24;
const LISTEN: usize = 8;
const STEP: usize = 32;
const DROP: i32 = TILE;
const HUSH: f32 = 0.1;
const STEADY: f32 = 0.05;

fn open() -> (App, Renderer) {
    let mut patch = Params::DEFAULT;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    let mut project = Project::new();
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch))];
    project.tracks[0].clips.push(Clip { start: 0, length: BEATS * TICKS_PER_BEAT, label: String::from("held"), notes: vec![Note { pitch: 60, start: 0.0, length: BEATS as f32, velocity: VELOCITY, articulation: Articulation::PLAIN }], file: None });
    let session = format!("{{\"project\":{},\"automation\":false,\"keyboard\":false,\"palette\":false}}", project.to_json().write());
    let mut app = App::new();
    app.resume(&session);
    let renderer = app.audio_started(SAMPLE_RATE, BLOCK, BLOCK as u32);
    painted(&mut app);
    (app, renderer)
}

fn play(app: &mut App) {
    app.event(Event::Key { key: Key::Space, shift: false, command: false, repeat: false });
}

fn settled(renderer: &mut Renderer) -> f32 {
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut peak: f32 = 0.0;
    for block in 0..SETTLE {
        renderer.render(&mut l, &mut r);
        if block + LISTEN >= SETTLE {
            peak = peak.max(peak_of(&l));
        }
    }
    peak
}

fn sweep(app: &mut App, renderer: &mut Renderer) -> f32 {
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut peak: f32 = 0.0;
    for y in (0..HEIGHT).step_by(STEP) {
        for x in (0..WIDTH).step_by(STEP) {
            app.event(Event::Move { x, y });
        }
        renderer.render(&mut l, &mut r);
        peak = peak.max(peak_of(&l));
    }
    app.event(Event::Leave);
    peak
}

fn fader_handle(fills: &[Fill]) -> Rect {
    let tile = Rect::new(0, RAIL, COLUMN_W, TILE);
    fills
        .iter()
        .filter(|fill| fill.tone == Tone::Ink100 && fill.rect.h == HANDLE_H && fill.rect.w <= VOL_W && tile.contains(fill.rect.x, fill.rect.y))
        .map(|fill| fill.rect)
        .min_by_key(|rect| rect.y)
        .unwrap_or_else(|| panic!("wanted a {HANDLE_H} px fader handle painted in the first tile"))
}

#[test]
fn a_fader_dragged_by_the_mouse_is_heard_before_the_release() {
    let (mut app, mut renderer) = open();
    play(&mut app);
    let before = settled(&mut renderer);
    assert!(before > AUDIBLE, "wanted the held note heard once play is pressed, got a peak of {before}");
    let handle = fader_handle(&painted(&mut app));
    let (x, y) = (handle.x + handle.w / 2, handle.y + handle.h / 2);
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Move { x, y: y + DROP });
    let dragged = settled(&mut renderer);
    app.event(Event::Up { x, y: y + DROP });
    assert!(dragged < before * HUSH, "wanted the fader dragged {DROP} px down heard before the release, got a peak of {dragged} against {before}");
}

#[test]
fn a_hover_sweep_leaves_a_held_note_as_it_was() {
    let (mut app, mut renderer) = open();
    play(&mut app);
    let before = settled(&mut renderer);
    assert!(before > AUDIBLE, "wanted the held note heard once play is pressed, got a peak of {before}");
    let during = sweep(&mut app, &mut renderer);
    let after = settled(&mut renderer);
    assert!((during - before).abs() <= before * STEADY, "wanted the note steady under the sweep, got a peak of {during} against {before}");
    assert!((after - before).abs() <= before * STEADY, "wanted the note as it was after the sweep, got a peak of {after} against {before}");
}

#[test]
fn a_hover_sweep_over_the_open_roll_sounds_nothing() {
    let (mut app, mut renderer) = open();
    let clip = clip_block(&painted(&mut app));
    let (x, y) = (clip.x + clip.w / 2, clip.y + clip.h / 2);
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    app.event(Event::Leave);
    let heard = sweep(&mut app, &mut renderer);
    let after = settled(&mut renderer);
    assert!(heard <= SILENT && after <= SILENT, "wanted nothing heard from hovering the roll's note, got peaks of {heard} during the sweep and {after} after");
}
