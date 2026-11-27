use bp::app::{App, Event};
use bp::arp::Arp;
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::draw::{Fill, Rect, Tone};
use bp::face::LABEL_SCALE;
use bp::font::{self, ADVANCE, GLYPH_H, GLYPH_W};
use bp::fx::{self, Kind, Slot};
use bp::json::{self, Json};
use bp::project::{Aim, Clip, Curve, LaneLfo, Node, Project, Row, TICKS_PER_BEAT, Timing, Track};
use bp::wav::Wave;
use std::sync::Arc;
use support::{BLOCK, Onsets, SAMPLE_RATE, WIDTH, call, centre, core, midi};

const RAIL: i32 = 45;
const STEP: usize = 2;
const SETTINGS_ITEM: i32 = 10;
const ITEM_H: i32 = 32;
const MENU_INSET: i32 = 12;
const COLUMN_X: i32 = 40;
const ROW: i32 = 33;
const CONTROL_BORDER: i32 = 2;
const BORDER: i32 = 8;
const TEXT_PAD: i32 = 10;
const TALL: i32 = 1400;
const LINE_GAP: i32 = 3;
const HELP_SCALE: i32 = 2 * LABEL_SCALE;
const LIMITED: usize = 71;
const CLICK_PCM: i16 = 8_192;
const CLICK_FRAME: usize = 48_000;
const WAVE_FRAMES: usize = 2 * 48_000;
const LISTEN: usize = 60_000;
const CLIP_BEATS: i32 = 4;
const WAVE_NAME: &str = "click.wav";
const BAND: &str = "LIMIT";
const PARAM: &str = "LOOKAHEAD";
const LONGEST: f32 = 1.0;
const CLOSE: f64 = 1e-6;

fn limiter() -> Row {
    let mut slot = Slot::new(Kind::Utility);
    if let fx::Params::Utility(utility) = &mut slot.params {
        utility.limit = true;
    }
    Row::Effect(slot)
}

fn latent() -> Project {
    let mut project = Project::new();
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), core(1.0), limiter()];
    project
}

fn resumed(project: &Project) -> (App, Renderer) {
    let session = format!("{{\"project\":{},\"automation\":false,\"keyboard\":false,\"palette\":false}}", project.to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    let renderer = app.audio_started(SAMPLE_RATE, BLOCK, BLOCK as u32);
    app.take_requests();
    (app, renderer)
}

fn read(app: &mut App) -> Json {
    let reply = call(app, "read", "{}");
    assert!(!reply.failed, "wanted read answered, got {}", reply.text);
    json::parse(&reply.text).unwrap_or_else(|| panic!("wanted read to answer JSON, got {}", reply.text))
}

fn number(json: &Json, key: &str) -> f64 {
    json.get(key).and_then(Json::as_f64).unwrap_or_else(|| panic!("wanted a number at {key} in {}", json.write()))
}

fn click_wave() -> Arc<Wave> {
    let mut pcm = vec![0i16; WAVE_FRAMES];
    pcm[CLICK_FRAME] = CLICK_PCM;
    Arc::new(Wave::sound(String::from(WAVE_NAME), SAMPLE_RATE, 1, pcm))
}

fn clicked() -> Project {
    let mut track = Track::audio();
    track.rack = vec![limiter()];
    track.clips.push(Clip::audio(0, CLIP_BEATS * TICKS_PER_BEAT, String::from("click"), String::from(WAVE_NAME)));
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0] = track;
    project
}

fn lookahead_curve() -> Curve {
    Curve { aim: Aim::knob(0, BAND, PARAM), lfo: None, nodes: vec![Node { tick: 0, travel: LONGEST, bend: 0.0 }] }
}

fn lookahead_lfo() -> LaneLfo {
    LaneLfo { amount: 1.0, sync: false, rate_hz: 2.0, ..LaneLfo::blank(Aim::knob(0, BAND, PARAM)) }
}

fn saved(project: &Project) -> Project {
    let json = json::parse(&project.to_json().write()).expect("wanted the saved project to parse");
    Project::from_json(&json).expect("wanted the saved project to open")
}

fn heard(project: &Project, wave: &Arc<Wave>) -> (Timing, Vec<usize>) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, None, &[], std::slice::from_ref(wave));
    audio.play(0.0);
    let mut onsets = Onsets::default();
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    while onsets.frame < LISTEN {
        renderer.render(&mut l, &mut r);
        onsets.feed(&l);
    }
    (audio.timing().clone(), onsets.at)
}

fn drawn(app: &mut App) -> Vec<Fill> {
    app.draw(WIDTH, TALL).to_vec()
}

fn click(app: &mut App, (x, y): (i32, i32)) {
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    app.event(Event::Leave);
}

fn rail_wash(fills: &[Fill]) -> Option<Rect> {
    fills.iter().find(|fill| fill.tone == Tone::Ink25 && fill.rect.y < RAIL && fill.rect.h >= 30).map(|fill| fill.rect)
}

fn file_button(app: &mut App) -> Rect {
    for x in (0..WIDTH).step_by(STEP) {
        app.event(Event::Move { x, y: RAIL / 2 });
        if let Some(rect) = rail_wash(&drawn(app)) {
            return rect;
        }
    }
    panic!("wanted the File button at the left of the rail");
}

fn settings_opened(project: &Project) -> (App, Renderer) {
    let (mut app, renderer) = resumed(project);
    let file = file_button(&mut app);
    click(&mut app, centre(file));
    click(&mut app, (file.x + MENU_INSET, file.y + file.h + SETTINGS_ITEM * ITEM_H + ITEM_H / 2));
    (app, renderer)
}

fn column(app: &mut App) -> Vec<Rect> {
    let mut washes: Vec<Rect> = Vec::new();
    for y in (RAIL..TALL).step_by(STEP) {
        app.event(Event::Move { x: COLUMN_X, y });
        if let Some(rect) = drawn(app).iter().find(|fill| fill.tone == Tone::Ink25 && fill.rect.y >= RAIL && fill.rect.contains(COLUMN_X, y)).map(|fill| fill.rect)
            && washes.last() != Some(&rect)
        {
            washes.push(rect);
        }
    }
    assert!(!washes.is_empty(), "wanted the settings controls to wash under the pointer");
    washes
}

fn load_readout(app: &mut App) -> Vec<Fill> {
    let low = *column(app).last().expect("wanted a lowest control");
    app.event(Event::Move { x: COLUMN_X, y: low.y + ROW + low.h / 2 });
    let left = low.x + low.w + 2 * BORDER;
    drawn(app).into_iter().filter(|fill| fill.rect.y >= RAIL && fill.rect.x >= left && fill.rect.x < left + low.w + 2 * BORDER).collect()
}

fn glyphs() -> Vec<([u8; 9], char)> {
    (b' '..=b'~').map(char::from).chain(['\u{2026}']).filter_map(|c| font::glyph_char(c).map(|rows| (*rows, c))).collect()
}

fn decoded_line(runs: &[Rect], x0: i32, origin: i32, glyphs: &[([u8; 9], char)]) -> Option<String> {
    if runs.iter().any(|run| run.y < origin || run.y >= origin + GLYPH_H) {
        return None;
    }
    let right = runs.iter().map(|run| run.x + run.w).max()?;
    let cells = (right - x0 + ADVANCE - 1) / ADVANCE;
    let lit = |x: i32, y: i32| runs.iter().any(|run| run.y == y && run.x <= x && x < run.x + run.w);
    let mut line = String::new();
    for cell in 0..cells {
        let left = x0 + cell * ADVANCE;
        if (0..GLYPH_H).any(|row| lit(left + GLYPH_W, origin + row)) {
            return None;
        }
        let mut rows = [0u8; 9];
        for (row, bits) in rows.iter_mut().enumerate() {
            for col in 0..GLYPH_W {
                if lit(left + col, origin + row as i32) {
                    *bits |= 1 << (GLYPH_W - 1 - col);
                }
            }
        }
        line.push(glyphs.iter().find(|(shape, _)| *shape == rows).map(|(_, c)| *c)?);
    }
    Some(line.trim_end().to_string())
}

fn readout_text(fills: &[Fill]) -> String {
    let frame = fills.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.h == CONTROL_BORDER).map(|fill| fill.rect).max_by_key(|rect| rect.w).expect("wanted the readout framed in the help column");
    let x0 = frame.x + CONTROL_BORDER + TEXT_PAD;
    let runs: Vec<Rect> = fills.iter().filter(|fill| fill.tone == Tone::Ink75 && fill.rect.h == HELP_SCALE).map(|fill| Rect::new(x0 + (fill.rect.x - x0) / HELP_SCALE, fill.rect.y / HELP_SCALE, fill.rect.w / HELP_SCALE, LABEL_SCALE)).collect();
    let mut ys: Vec<i32> = runs.iter().map(|run| run.y).collect();
    ys.sort_unstable();
    ys.dedup();
    let mut lines: Vec<(i32, i32)> = Vec::new();
    for y in ys {
        match lines.last_mut() {
            Some((_, bottom)) if y - *bottom < LINE_GAP => *bottom = y,
            _ => lines.push((y, y)),
        }
    }
    let glyphs = glyphs();
    lines
        .iter()
        .map(|&(top, bottom)| {
            let line: Vec<Rect> = runs.iter().copied().filter(|run| run.y >= top && run.y <= bottom).collect();
            (bottom + 1 - GLYPH_H..=top).find_map(|origin| decoded_line(&line, x0, origin, &glyphs)).unwrap_or_else(|| format!("<unread line at y {top}>"))
        })
        .collect::<Vec<String>>()
        .join(" ")
}

#[test]
fn the_read_document_carries_the_latency_and_a_write_carrying_it_back_changes_nothing() {
    let project = latent();
    let (mut app, _renderer) = resumed(&project);
    let planned = Timing::of(&project, SAMPLE_RATE);
    let document = read(&mut app);
    let latency = document.get("latency").unwrap_or_else(|| panic!("wanted a latency object in the document, got {}", document.write()));
    let samples = number(latency, "outputSamples");
    assert_eq!(samples, planned.output as f64, "wanted outputSamples {} in {}", planned.output, latency.write());
    let ms = number(latency, "outputMs");
    let wanted = samples * 1000.0 / SAMPLE_RATE;
    assert!((ms - wanted).abs() < CLOSE, "wanted outputMs {wanted} within {CLOSE}, got {ms}");
    assert_eq!(number(latency, "sampleRate"), SAMPLE_RATE, "wanted sampleRate {SAMPLE_RATE} in {}", latency.write());
    let tracks: Vec<f64> = latency.get("tracks").and_then(Json::as_array).unwrap_or_else(|| panic!("wanted tracks listed in {}", latency.write())).iter().map(|track| track.as_f64().unwrap_or(f64::NAN)).collect();
    let wanted: Vec<f64> = planned.tracks.iter().map(|track| track.latency as f64).collect();
    assert_eq!(tracks, wanted, "wanted each track's latency listed");
    let reply = call(&mut app, "write", &format!("{{\"value\":{}}}", document.write()));
    assert!(!reply.failed, "wanted the document written back with its latency object, got {}", reply.text);
    let again = read(&mut app);
    assert_eq!(again, document, "wanted the document unchanged by writing it back with latency included");
}

#[test]
fn a_saved_curve_on_the_limiters_lookahead_moves_neither_the_figure_nor_the_sound() {
    let wave = click_wave();
    let (plain_timing, plain_onsets) = heard(&clicked(), &wave);
    assert_eq!(plain_onsets, vec![CLICK_FRAME + LIMITED], "wanted the click heard {LIMITED} frames after frame {CLICK_FRAME} through the limiter alone, got onsets {plain_onsets:?}");
    let mut project = clicked();
    project.tracks[0].automation.curves.push(lookahead_curve());
    let project = saved(&project);
    let kept = project.tracks[0].automation.curves.len();
    let (timing, onsets) = heard(&project, &wave);
    assert_eq!((timing.tracks[0].latency, timing.output), (plain_timing.tracks[0].latency, plain_timing.output), "wanted the track's and the output's figures unchanged by the saved LOOKAHEAD curve, {kept} kept by the save");
    assert_eq!(onsets, plain_onsets, "wanted the click heard at the same frame with the saved LOOKAHEAD curve at travel {LONGEST} as without it, {kept} curve kept by the save, got onsets {onsets:?} against {plain_onsets:?}");
}

#[test]
fn a_saved_lfo_lane_on_the_limiters_lookahead_moves_neither_the_figure_nor_the_sound() {
    let wave = click_wave();
    let (plain_timing, plain_onsets) = heard(&clicked(), &wave);
    assert_eq!(plain_onsets, vec![CLICK_FRAME + LIMITED], "wanted the click heard {LIMITED} frames after frame {CLICK_FRAME} through the limiter alone, got onsets {plain_onsets:?}");
    let mut project = clicked();
    project.tracks[0].automation.lfos.push(lookahead_lfo());
    let project = saved(&project);
    let kept = project.tracks[0].automation.lfos.len();
    let (timing, onsets) = heard(&project, &wave);
    assert_eq!((timing.tracks[0].latency, timing.output), (plain_timing.tracks[0].latency, plain_timing.output), "wanted the track's and the output's figures unchanged by the saved LOOKAHEAD lane, {kept} kept by the save");
    assert_eq!(onsets, plain_onsets, "wanted the click heard at the same frame with the saved LOOKAHEAD LFO lane as without it, {kept} lane kept by the save, got onsets {onsets:?} against {plain_onsets:?}");
}

#[test]
fn the_load_readout_says_how_many_samples_after_the_playhead_the_sound_leaves() {
    let project = latent();
    let (mut app, _renderer) = settings_opened(&project);
    let text = readout_text(&load_readout(&mut app));
    let planned = Timing::of(&project, SAMPLE_RATE);
    let wanted = format!("The sound leaves {} samples after the playhead, {:.1} ms", planned.output, planned.ms(planned.output));
    assert!(text.contains(&wanted), "wanted the LOAD readout to name {wanted:?}, got {text:?}");
}

#[test]
fn the_load_readout_says_no_row_delays_a_dry_project() {
    let (mut app, _renderer) = settings_opened(&Project::new());
    let text = readout_text(&load_readout(&mut app));
    assert!(text.contains("No row delays the sound"), "wanted the LOAD readout to say no row delays the sound, got {text:?}");
}
