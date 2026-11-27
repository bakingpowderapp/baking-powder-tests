use bp::app::{App, Event, Request};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::draw::{Fill, Rect, Tone};
use bp::face::{CELL, FRAME};
use bp::grid::PLAYHEAD_W;
use bp::json::{self, Json};
use bp::keyboard::Key;
use bp::project::{Clip, Project, TICKS_PER_BEAT, Track};
use bp::wav::Wave;
use std::sync::Arc;
use support::{BLOCK, RATE, SAMPLE_RATE, centre, clip_block, painted, worst_delta};

const PATH: &str = "/x/tone.wav";
const NAME: &str = "tone";
const CUT: &str = "tone cut";
const CUT_AGAIN: &str = "tone cut 2";
const HALF: i16 = 16_384;
const QUARTER: i16 = 8_192;
const FILE_SECONDS: usize = 2;
const CLIP_BEATS: i32 = 2;
const OTHER_START: i32 = 4;
const OTHER_BEATS: i32 = 1;
const SONG_BEATS: usize = 6;
const CLIP_FRAMES: usize = 48_000;
const EDGE: usize = 128;
const STEP: f32 = 1.0 / 32_768.0;
const SELECTED: (i32, i32) = (4, 2);

fn tone() -> Vec<u8> {
    let samples: Vec<i16> = (0..FILE_SECONDS * RATE as usize).map(|frame| if frame < RATE as usize { HALF } else { QUARTER }).collect();
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
    project.tracks[0].clips.push(Clip::audio(OTHER_START * TICKS_PER_BEAT, OTHER_BEATS * TICKS_PER_BEAT, String::from(NAME), String::from(NAME)));
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

fn command(app: &mut App, letter: char) {
    app.event(Event::Key { key: Key::Char(letter), shift: false, command: true, repeat: false });
}

fn saved(app: &mut App) -> (Project, Vec<Arc<Wave>>) {
    command(app, 's');
    let text = app.take_requests().into_iter().find_map(|request| if let Request::SaveAs { json, .. } = request { Some(json) } else { None }).expect("wanted the project offered for saving after Cmd+S");
    let document = json::parse(&text).expect("wanted the saved project to parse");
    let project = Project::from_json(&document).expect("wanted the saved project to read");
    let waves = document.get("waves").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(Wave::from_json).map(Arc::new).collect();
    (project, waves)
}

fn file_name(project: &Project, index: usize) -> String {
    project.tracks[0].clips[index].file.as_ref().map(|file| file.name.clone()).expect("wanted an audio clip")
}

fn frames_of(waves: &[Arc<Wave>], name: &str) -> Option<usize> {
    waves.iter().find(|wave| wave.name == name).map(|wave| wave.frames())
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

fn select(app: &mut App, body: Rect) {
    let (from, to) = (body.x + body.w / SELECTED.0, body.x + body.w / SELECTED.1);
    let y = body.y + body.h / 2;
    app.event(Event::Down { x: from, y, clicks: 1 });
    app.event(Event::Move { x: to, y });
    app.event(Event::Up { x: to, y });
}

fn render(project: &Project, waves: &[Arc<Wave>]) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, None, &[], waves);
    audio.play(0.0);
    let frames = SONG_BEATS * CLIP_FRAMES / CLIP_BEATS as usize;
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(frames);
    while out.len() < frames {
        let n = BLOCK.min(frames - out.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    out
}

fn sounds_the_same(before: &[f32], after: &[f32]) {
    for (from, to) in [(EDGE, CLIP_FRAMES - EDGE), (CLIP_FRAMES + EDGE, before.len())] {
        let delta = worst_delta(&before[from..to], &after[from..to]);
        assert!(delta <= STEP, "wanted the song unchanged to the ear between frames {from} and {to} after the cut, got a sample off by {delta}");
    }
}

#[test]
fn command_j_cuts_the_file_down_to_the_selected_clip() {
    let original = Arc::new(Wave::parse(String::from(NAME), String::from(PATH), &tone()).expect("wanted the tone file to read"));
    let before = render(&project(), &[original]);
    let mut app = open();
    let clip = clip_block(&painted(&mut app));
    click(&mut app, centre(clip));
    command(&mut app, 'j');
    let (project, waves) = saved(&mut app);
    let (name, other) = (file_name(&project, 0), file_name(&project, 1));
    assert_eq!(name, CUT, "wanted the picked clip on a file of its own, it plays {name}");
    assert_eq!(other, NAME, "wanted the other clip left on the whole file, it plays {other}");
    let (start, length) = (project.tracks[0].clips[0].start, project.tracks[0].clips[0].length);
    assert_eq!((start, length), (0, CLIP_BEATS * TICKS_PER_BEAT), "wanted the clip where it was, {CLIP_BEATS} beats from 0, got {length} ticks from {start}");
    let frames = frames_of(&waves, CUT);
    assert_eq!(frames, Some(CLIP_FRAMES), "wanted the cut file {CLIP_FRAMES} frames long, just the clip, got {frames:?}");
    let whole = frames_of(&waves, NAME);
    assert_eq!(whole, Some(FILE_SECONDS * RATE as usize), "wanted the whole file still in the pool for the other clip, got {whole:?} frames");
    let after = render(&project, &waves);
    sounds_the_same(&before, &after);
}

#[test]
fn command_j_again_leaves_a_cut_clip_alone() {
    let mut app = open();
    let clip = clip_block(&painted(&mut app));
    click(&mut app, centre(clip));
    command(&mut app, 'j');
    command(&mut app, 'j');
    let (project, waves) = saved(&mut app);
    let name = file_name(&project, 0);
    assert_eq!(name, CUT, "wanted the clip still on its first cut, it plays {name}");
    assert_eq!(frames_of(&waves, CUT_AGAIN), None, "wanted no second cut of a file that already fits its clip");
}

#[test]
fn a_selection_in_the_editor_does_not_shorten_the_clip() {
    let mut app = open();
    let clip = clip_block(&painted(&mut app));
    click(&mut app, centre(clip));
    let body = editor_body(&painted(&mut app));
    select(&mut app, body);
    command(&mut app, 'j');
    let (project, waves) = saved(&mut app);
    let (start, length) = (project.tracks[0].clips[0].start, project.tracks[0].clips[0].length);
    assert_eq!((start, length), (0, CLIP_BEATS * TICKS_PER_BEAT), "wanted the clip untouched by the editor's selection, {CLIP_BEATS} beats from 0, got {length} ticks from {start}");
    let frames = frames_of(&waves, CUT);
    assert_eq!(frames, Some(CLIP_FRAMES), "wanted the cut to cover the whole clip, not the selection, got {frames:?} frames");
}
