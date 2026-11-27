use bp::app::{App, Event, Request};
use bp::audio::{Audio, Cue};
use bp::audio::vision::Seer;
use bp::browser::{Found, Item};
use bp::draw::{Fill, Rect};
use bp::face::{CELL, PITCH};
use bp::project::Project;
use bp::wav::Wave;
use std::sync::Arc;
use support::{BLOCK, RATE, SAMPLE_RATE, SILENT, browser_rect, painted};

const MAX_FRAMES: usize = 512;
const FADE: usize = 64;
const HALF: i16 = 16_384;
const QUARTER: i16 = 8_192;
const KICK_SECONDS: f64 = 0.5;
const ROOT: &str = "/x/Samples";
const KICK: &str = "/x/Samples/kick.wav";
fn wav(level: i16, seconds: f64) -> Vec<u8> {
    let data: Vec<u8> = (0..(seconds * f64::from(RATE)) as usize).flat_map(|_| level.to_le_bytes()).collect();
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

fn cue(path: &str, level: i16, seconds: f64) -> Box<Cue> {
    let wave = Arc::new(Wave::parse(String::from(path), String::from(path), &wav(level, seconds)).expect("wanted the test file to read"));
    Box::new(Cue { wave, warp: None, once: false })
}

fn blocks(renderer: &mut bp::audio::Renderer, count: usize) -> Vec<f32> {
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(count * BLOCK);
    for _ in 0..count {
        renderer.render(&mut l, &mut r);
        out.extend_from_slice(&l);
    }
    out
}

fn first_sound(samples: &[f32]) -> Option<usize> {
    samples.iter().position(|sample| sample.abs() > SILENT)
}

fn last_sound(samples: &[f32]) -> Option<usize> {
    samples.iter().rposition(|sample| sample.abs() > SILENT)
}

#[test]
fn an_audition_plays_the_file_at_once_with_the_transport_parked_and_comes_round_at_its_end() {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, MAX_FRAMES, Seer::Inline);
    audio.sync(&Project::new(), None, &[], &[]);
    audio.preview(Some(cue(KICK, HALF, KICK_SECONDS)));
    let out = blocks(&mut renderer, 120);
    let length = (KICK_SECONDS * SAMPLE_RATE) as usize;
    assert_eq!(first_sound(&out), Some(0), "wanted the audition sounding from the first frame, got {:?}", first_sound(&out));
    assert!((out[0] - 0.5 / FADE as f32).abs() < SILENT, "wanted the first frame one step into the fade, got {}", out[0]);
    assert_eq!(out[FADE], 0.5, "wanted the file at its level {FADE} frames in, got {}", out[FADE]);
    assert!(out[length - 1] < 0.02, "wanted the file's end faded near silence before it comes round, got {}", out[length - 1]);
    assert!((out[length] - 0.5 / FADE as f32).abs() < SILENT, "wanted the file starting again at frame {length}, one step into its fade, got {}", out[length]);
    assert_eq!(out[length + FADE], 0.5, "wanted the file back at its level {FADE} frames into its second pass, got {}", out[length + FADE]);
    assert!(last_sound(&out).is_some_and(|last| last + BLOCK > out.len()), "wanted the audition still sounding in the render's last block, got its last sound at {:?} of {}", last_sound(&out), out.len());
}

#[test]
fn a_second_audition_takes_over_within_a_block_and_none_fades_out() {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, MAX_FRAMES, Seer::Inline);
    audio.sync(&Project::new(), None, &[], &[]);
    audio.preview(Some(cue(KICK, HALF, 2.0)));
    blocks(&mut renderer, 4);
    audio.preview(Some(cue("/x/other.wav", QUARTER, 2.0)));
    let swap = blocks(&mut renderer, 1);
    assert!(swap[0] > 0.45 && swap[0] < 0.55, "wanted the old file still near its level as the block begins, got {}", swap[0]);
    assert!((swap[BLOCK - 1] - 0.25).abs() < 0.01, "wanted only the new file by the block's end, got {}", swap[BLOCK - 1]);
    let settled = blocks(&mut renderer, 1);
    assert_eq!(settled[0], 0.25, "wanted the new file alone in the next block, got {}", settled[0]);
    audio.preview(None);
    let fade = blocks(&mut renderer, 1);
    assert!(fade[0] > 0.2 && fade[BLOCK - 1] < 0.01, "wanted the audition to fade out across the block after it was stopped, got {} then {}", fade[0], fade[BLOCK - 1]);
    let after = blocks(&mut renderer, 1);
    assert_eq!(first_sound(&after), None, "wanted silence after the stopped audition, got sound at {:?}", first_sound(&after));
}

fn open() -> App {
    let session = format!("{{\"project\":{},\"aside\":\"browser\",\"keyboard\":false,\"folders\":[\"{ROOT}\"],\"unfolded\":[\"{ROOT}\"]}}", Project::new().to_json().write());
    let mut app = App::new();
    app.resume(&session);
    app.take_requests();
    app.listed(ROOT, vec![Found { path: String::from(KICK), name: String::from("kick.wav"), item: Item::Wave(u64::MAX) }]);
    app.take_requests();
    app
}

fn serve(app: &mut App) -> usize {
    let mut reads = 0;
    loop {
        let requests = app.take_requests();
        if requests.is_empty() {
            return reads;
        }
        for request in requests {
            if let Request::Read { path } = request {
                reads += 1;
                app.read(path, Some(wav(HALF, KICK_SECONDS)));
            }
        }
    }
}

fn wave_row(fills: &[Fill]) -> Rect {
    let browser = browser_rect(fills);
    let mark = Rect::new(browser.x + PITCH, browser.y + PITCH, CELL, CELL);
    Rect::new(mark.x + PITCH, mark.y, browser.x + browser.w - mark.x - PITCH, CELL)
}

fn press(app: &mut App, rect: Rect) {
    let (x, y) = (rect.x + rect.w / 2, rect.y + rect.h / 2);
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
}

#[test]
fn pressing_a_wave_row_plays_it_and_pressing_again_stops_it() {
    let mut app = open();
    let mut renderer = app.audio_started(SAMPLE_RATE, MAX_FRAMES, BLOCK as u32);
    app.take_requests();
    let name = wave_row(&painted(&mut app));
    press(&mut app, name);
    let reads = serve(&mut app);
    assert_eq!(reads, 1, "wanted the press to read the file once, got {reads} reads");
    let out = blocks(&mut renderer, 2);
    assert_eq!(first_sound(&out), Some(0), "wanted the audition heard from the first frame after the press, got {:?}", first_sound(&out));
    assert_eq!(out[FADE], 0.5, "wanted the file at its level {FADE} frames in, got {}", out[FADE]);
    press(&mut app, name);
    let reads = serve(&mut app);
    assert_eq!(reads, 0, "wanted the second press to read nothing, got {reads} reads");
    let fade = blocks(&mut renderer, 1);
    assert!(fade[0] > 0.2 && fade[BLOCK - 1] < 0.01, "wanted the second press to fade the audition out across a block, got {} then {}", fade[0], fade[BLOCK - 1]);
    let after = blocks(&mut renderer, 1);
    assert_eq!(first_sound(&after), None, "wanted silence after the second press, got sound at {:?}", first_sound(&after));
    press(&mut app, name);
    serve(&mut app);
    let again = blocks(&mut renderer, 1);
    assert_eq!(first_sound(&again), Some(0), "wanted a third press to play the file again from the store, got {:?}", first_sound(&again));
}
