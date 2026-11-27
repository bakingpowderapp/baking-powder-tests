use bp::app::{App, Event};
use bp::keyboard::Key;
use bp::audio::{Audio, Renderer};
use bp::audio::vision::Seer;
use bp::draw::{Fill, Rect, Tone};
use bp::grid::{BEAT_W, PLAYHEAD_W};
use bp::project::Project;
use bp::ruler;
use support::{BLOCK, FRAMES_PER_BEAT, Onsets, SAMPLE_RATE, WITHIN, beat_frame, frames, painted};

const MAX_FRAMES: usize = 4_096;
const IDLE_SECONDS: f64 = 30.0 * 60.0;
const PLAYED_BEATS: usize = 3_600;
const PAUSE_SECONDS: f64 = 2.0;
const WATCHED_BEATS: usize = 16;
const SHOWN_BEATS: usize = 8;

fn project() -> Project {
    let mut project = Project::new();
    project.metronome = true;
    project
}

fn open() -> (Audio, Renderer) {
    let (mut audio, renderer) = Audio::open(SAMPLE_RATE, MAX_FRAMES, Seer::Inline);
    audio.sync(&project(), None, &[], &[]);
    (audio, renderer)
}

fn shown_frame(audio: &Audio) -> i64 {
    (audio.position().beat * FRAMES_PER_BEAT).round() as i64
}

fn slack(rendered: usize) -> i64 {
    1 + (rendered as f64 * f64::from(f32::EPSILON)).ceil() as i64
}

fn reads_start(audio: &Audio) {
    let shown = shown_frame(audio);
    assert_eq!(shown, 0, "the moment Play is pressed the UI reads frame {shown}; wanted frame 0");
}

fn render_stopped(audio: &Audio, renderer: &mut Renderer, total: usize) {
    let (mut l, mut r) = (vec![0.0; MAX_FRAMES], vec![0.0; MAX_FRAMES]);
    let mut done = 0;
    while done < total {
        let n = (total - done).min(MAX_FRAMES);
        renderer.render(&mut l[..n], &mut r[..n]);
        done += n;
        let shown = shown_frame(audio);
        assert_eq!(shown, 0, "stopped for {done} frames, the UI reads frame {shown}; wanted frame 0");
    }
}

fn watch(audio: &Audio, renderer: &mut Renderer, beats: usize, block: usize) -> Onsets {
    let (mut l, mut r) = (vec![0.0; MAX_FRAMES], vec![0.0; MAX_FRAMES]);
    let mut onsets = Onsets::default();
    let total = ((beats as f64 + 0.5) * FRAMES_PER_BEAT) as usize;
    let mut last = shown_frame(audio);
    while onsets.frame < total {
        let n = (total - onsets.frame).min(block);
        renderer.render(&mut l[..n], &mut r[..n]);
        onsets.feed(&l[..n]);
        let (rendered, shown) = (onsets.frame, shown_frame(audio));
        let wanted = rendered as i64;
        assert!((shown - wanted).abs() <= slack(rendered), "{rendered} frames into playback the UI reads beat {}, frame {shown}; wanted frame {wanted}", audio.position().beat);
        assert!(shown >= last, "{rendered} frames into playback the UI stepped back from frame {last} to frame {shown}");
        last = shown;
    }
    onsets
}

fn check(onsets: &[usize], beats: usize) {
    let missed: Vec<usize> = (0..=beats).filter(|&beat| !onsets.iter().any(|&at| at.abs_diff(beat_frame(beat)) <= WITHIN)).collect();
    let stray: Vec<usize> = onsets.iter().copied().filter(|&at| !(0..=beats).any(|beat| at.abs_diff(beat_frame(beat)) <= WITHIN)).collect();
    assert!(missed.is_empty() && stray.is_empty(), "missed clicks on beats {missed:?}, stray clicks at frames {stray:?}");
}

fn open_app() -> App {
    let session = format!("{{\"project\":{},\"automation\":false,\"keyboard\":false,\"palette\":false}}", project().to_json().write());
    let mut app = App::new();
    app.resume(&session);
    app
}

fn space(app: &mut App) {
    app.event(Event::Key { key: Key::Space, shift: false, command: false, repeat: false });
    app.tick(BLOCK as f32 / SAMPLE_RATE as f32);
}

fn advance(app: &mut App, renderer: &mut Renderer, total: usize, block: usize) {
    let (mut l, mut r) = (vec![0.0; MAX_FRAMES], vec![0.0; MAX_FRAMES]);
    let mut done = 0;
    while done < total {
        let n = (total - done).min(block);
        renderer.render(&mut l[..n], &mut r[..n]);
        done += n;
        app.tick(n as f32 / SAMPLE_RATE as f32);
    }
}

fn lines(playing: &[Fill], stopped: &[Fill]) -> Vec<Rect> {
    playing.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.w == PLAYHEAD_W && fill.rect.h > ruler::H && !stopped.iter().any(|other| other.rect == fill.rect)).map(|fill| fill.rect).collect()
}

fn line(app: &mut App, stopped: &[Fill], when: &str) -> Rect {
    let lines = lines(&painted(app), stopped);
    let [line] = lines.as_slice() else {
        panic!("wanted one playhead line {when}, found {}: {lines:?}", lines.len());
    };
    *line
}

fn beat_x(start: Rect, beats: usize) -> i32 {
    start.x + (beats as f64 * BEAT_W).round() as i32
}

#[test]
fn play_pressed_after_thirty_idle_minutes_shows_the_frame_just_rendered() {
    let (mut audio, mut renderer) = open();
    render_stopped(&audio, &mut renderer, frames(IDLE_SECONDS));
    audio.play(0.0);
    reads_start(&audio);
    let onsets = watch(&audio, &mut renderer, WATCHED_BEATS, BLOCK);
    check(&onsets.at, WATCHED_BEATS);
}

#[test]
fn play_pressed_again_after_thirty_minutes_of_playing_starts_over_at_once() {
    let (mut audio, mut renderer) = open();
    audio.play(0.0);
    watch(&audio, &mut renderer, PLAYED_BEATS, MAX_FRAMES);
    audio.stop();
    audio.place(0.0);
    render_stopped(&audio, &mut renderer, frames(PAUSE_SECONDS));
    audio.play(0.0);
    reads_start(&audio);
    let onsets = watch(&audio, &mut renderer, WATCHED_BEATS, BLOCK);
    check(&onsets.at, WATCHED_BEATS);
}

#[test]
fn the_playhead_line_follows_the_audio_after_a_press_thirty_idle_minutes_in() {
    let mut app = open_app();
    let mut renderer = app.audio_started(SAMPLE_RATE, MAX_FRAMES, BLOCK as u32);
    app.take_requests();
    let stopped = painted(&mut app);
    advance(&mut app, &mut renderer, frames(IDLE_SECONDS), MAX_FRAMES);
    let idle = lines(&painted(&mut app), &stopped);
    assert!(idle.is_empty(), "wanted no playhead line while stopped, found {idle:?}");
    space(&mut app);
    let start = line(&mut app, &stopped, "the moment Play is pressed");
    advance(&mut app, &mut renderer, beat_frame(SHOWN_BEATS), BLOCK);
    let moved = line(&mut app, &stopped, &format!("{SHOWN_BEATS} beats in"));
    assert_eq!(moved.x, beat_x(start, SHOWN_BEATS), "wanted the line at x {} after {SHOWN_BEATS} beats, got x {}", beat_x(start, SHOWN_BEATS), moved.x);
    space(&mut app);
    let halted = lines(&painted(&mut app), &stopped);
    assert!(halted.is_empty(), "wanted the line gone after Stop, found {halted:?}");
    space(&mut app);
    let back = line(&mut app, &stopped, "the moment Play is pressed again");
    assert_eq!(back.x, start.x, "wanted the second press to put the line back at x {}, got x {}", start.x, back.x);
    advance(&mut app, &mut renderer, beat_frame(1), BLOCK);
    let onward = line(&mut app, &stopped, "a beat after the second press");
    assert_eq!(onward.x, beat_x(start, 1), "wanted the line at x {} a beat after the second press, got x {}", beat_x(start, 1), onward.x);
}
