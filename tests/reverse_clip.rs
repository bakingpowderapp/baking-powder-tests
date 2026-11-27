use bp::app::{App, Event, Request};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::draw::{Fill, Rect, Tone};
use bp::face::{CELL, FRAME, PITCH};
use bp::grid::PLAYHEAD_W;
use bp::json::{self, Json};
use bp::keyboard::Key;
use bp::project::{Clip, Project, TICKS_PER_BEAT, Track};
use bp::wav::Wave;
use std::sync::Arc;
use support::{BLOCK, RATE, SAMPLE_RATE, centre, clip_block, inked, painted};

const PATH: &str = "/x/tone.wav";
const NAME: &str = "tone";
const REVERSED: &str = "tone reverse";
const RESTORED: &str = "tone 2";
const HALF: i16 = 16_384;
const QUARTER: i16 = 8_192;
const LOUD: f32 = 0.5;
const SOFT: f32 = 0.25;
const CLIP_BEATS: i32 = 2;
const CLIP_FRAMES: usize = 48_000;
const SWITCH: usize = CLIP_FRAMES / 2;
const EDGE: usize = 256;
const CLOSE: f32 = 0.002;
const BAND_CELLS: i32 = 10;
const ROWS_UNDER_WAVE: i32 = 3;

fn tone() -> Vec<u8> {
    let samples: Vec<i16> = (0..RATE as usize).map(|frame| if frame < SWITCH { HALF } else { QUARTER }).collect();
    let data: Vec<u8> = samples.iter().flat_map(|sample| sample.to_le_bytes()).collect();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&((36 + data.len()) as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&RATE.to_le_bytes());
    bytes.extend_from_slice(&(RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&data);
    bytes
}

fn project() -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0] = Track::audio();
    project.tracks[0].clips.push(Clip::audio(0, CLIP_BEATS * TICKS_PER_BEAT, String::from(NAME), String::from(NAME)));
    project
}

fn open() -> App {
    let session = format!("{{\"project\":{},\"keyboard\":false,\"palette\":false,\"automation\":false,\"beat_w\":120,\"origins\":[{{\"name\":\"{NAME}\",\"from\":\"{PATH}\"}}]}}", project().to_json().write());
    let mut app = App::new();
    app.resume(&session);
    loop {
        let requests = app.take_requests();
        if requests.is_empty() {
            break;
        }
        for request in requests {
            if let Request::Read { path } = request {
                app.read(path, Some(tone()));
            }
        }
    }
    app
}

fn click(app: &mut App, (x, y): (i32, i32)) {
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
}

fn saved(app: &mut App) -> (Project, Vec<Arc<Wave>>) {
    app.event(Event::Key { key: Key::Char('s'), shift: false, command: true, repeat: false });
    let text = app.take_requests().into_iter().find_map(|request| if let Request::SaveAs { json, .. } = request { Some(json) } else { None }).expect("wanted the project offered for saving after Cmd+S");
    let document = json::parse(&text).expect("wanted the saved project to parse");
    let project = Project::from_json(&document).expect("wanted the saved project to read");
    let waves = document.get("waves").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(Wave::from_json).map(Arc::new).collect();
    (project, waves)
}

fn file_name(project: &Project) -> String {
    project.tracks[0].clips[0].file.as_ref().map(|file| file.name.clone()).expect("wanted an audio clip")
}

fn editor_body(fills: &[Fill]) -> Rect {
    let inked = |fill: &&Fill| fill.tone == Tone::Ink100;
    let shelf_under = |line: Rect| fills.iter().filter(inked).find(|fill| fill.rect.h == FRAME && fill.rect.w > CELL && fill.rect.x == line.x - FRAME && fill.rect.y == line.y + line.h).map(|fill| fill.rect);
    let lines: Vec<(Rect, Rect)> = fills.iter().filter(inked).filter(|fill| fill.rect.w == PLAYHEAD_W && fill.rect.h >= CELL).filter_map(|fill| Some((fill.rect, shelf_under(fill.rect)?))).collect();
    let [(entry, shelf)] = lines.as_slice() else {
        panic!("wanted one entry line standing on the span lane's frame, found {}: {lines:?}", lines.len());
    };
    Rect::new(entry.x, entry.y, shelf.w - 2 * FRAME, entry.h)
}

fn reverse_cell(body: Rect) -> Rect {
    Rect::new(body.x - FRAME + (BAND_CELLS + 1) * PITCH, body.y + body.h + ROWS_UNDER_WAVE * PITCH, CELL, CELL)
}

fn render(project: &Project, waves: &[Arc<Wave>]) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, None, &[], waves);
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(CLIP_FRAMES);
    while out.len() < CLIP_FRAMES {
        let n = BLOCK.min(CLIP_FRAMES - out.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    out
}

fn level(samples: &[f32]) -> f32 {
    samples.iter().map(|sample| sample.abs()).sum::<f32>() / samples.len() as f32
}

fn halves(out: &[f32]) -> (f32, f32) {
    (level(&out[EDGE..SWITCH - EDGE]), level(&out[SWITCH + EDGE..CLIP_FRAMES - EDGE]))
}

#[test]
fn the_rev_cell_turns_the_clip_back_to_front_on_a_file_of_its_own_and_again_turns_it_back() {
    let original = Arc::new(Wave::parse(String::from(NAME), String::from(PATH), &tone()).expect("wanted the tone file to read"));
    let (loud, soft) = halves(&render(&project(), &[original]));
    assert!((loud - LOUD).abs() < CLOSE && (soft - SOFT).abs() < CLOSE, "wanted the clip loud then soft before any press, got levels {loud} then {soft}");
    let mut app = open();
    let clip = clip_block(&painted(&mut app));
    click(&mut app, centre(clip));
    let body = editor_body(&painted(&mut app));
    let cell = reverse_cell(body);
    assert!(inked(&painted(&mut app), Rect::new(cell.x, cell.y, CELL, FRAME)), "wanted the REV cell framed beside LOOP at {cell:?}");
    click(&mut app, centre(cell));
    let (project, waves) = saved(&mut app);
    let name = file_name(&project);
    assert_eq!(name, REVERSED, "wanted the clip on a reversed file of its own, it plays {name}");
    let (first, second) = halves(&render(&project, &waves));
    assert!((first - SOFT).abs() < CLOSE && (second - LOUD).abs() < CLOSE, "wanted the clip soft then loud once reversed, got levels {first} then {second}");
    click(&mut app, centre(cell));
    let (project, waves) = saved(&mut app);
    let name = file_name(&project);
    assert_eq!(name, RESTORED, "wanted a second press to turn the sound back on a new file, it plays {name}");
    let (first, second) = halves(&render(&project, &waves));
    assert!((first - LOUD).abs() < CLOSE && (second - SOFT).abs() < CLOSE, "wanted the clip loud then soft again after the second press, got levels {first} then {second}");
}
