use bp::app::{App, Event, Request};
use bp::arp::{Arp, Articulation};
use bp::audio::{Export, Tail};
use bp::core::Params;
use bp::draw::Tone;
use bp::fx::{Kind, Slot};
use bp::keyboard::Key;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY, beats};
use std::time::{Duration, Instant};
use support::{AUDIBLE, HEIGHT, WIDTH, beat_frame, core_layer, midi, painted, peak_of};

const HEADER: usize = 68;
const SCALE: f32 = 8_388_608.0;
const BLOCK: usize = 4096;
const FADE: f32 = 64.0;
const HOLD: usize = 2 * 48_000;
const SILENCE: f32 = 1e-3;
const FAINT: f32 = 1e-4;
const LOOP_BEATS: usize = 8;
const LOOP: (i32, i32) = (0, LOOP_BEATS as i32 * TICKS_PER_BEAT);
const STRAY: i32 = 64 * TICKS_PER_BEAT;
const TWO_MINUTES: (i32, i32) = (0, 240 * TICKS_PER_BEAT);
const MID_NOTE: (i32, i32) = (0, 2 * TICKS_PER_BEAT);
const AT_ONCE: usize = 8;
const EXPORTS_EACH: usize = 25;
const PANEL_W: i32 = 400;
const TICK: f32 = 1.0 / 60.0;
const PATIENCE: Duration = Duration::from_secs(60);
const PATH: &str = "/x/exported.wav";
const DECAY_TRAVEL: f64 = 0.828;
const FILE_CELL_CENTRE: (i32, i32) = (60, 26);
const EXPORT_AUDIO_ITEM_CENTRE: (i32, i32) = (60, 357);

fn held(length: f32) -> Clip {
    Clip { start: 0, length: length.ceil() as i32 * TICKS_PER_BEAT, label: String::new(), notes: vec![Note { pitch: 60, start: 0.0, length, velocity: VELOCITY, articulation: Articulation::PLAIN }], file: None }
}

fn patch(release_ms: f64) -> Params {
    let mut patch = Params::DEFAULT;
    patch.amp.sustain = 1.0;
    patch.amp.release_ms = release_ms;
    patch
}

fn project(clips: Vec<Clip>, rack: Vec<Row>) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = rack;
    project.tracks[0].clips = clips;
    project
}

fn looped() -> Project {
    project(vec![held(8.0), Clip::chord(STRAY, String::new(), &[60, 64, 67])], vec![midi(Arp::DEFAULT), Row::Core(core_layer(0.8, patch(40.0)))])
}

fn frames(wav: &[u8]) -> usize {
    (wav.len() - HEADER) / 6
}

fn stereo(wav: &[u8]) -> Vec<f32> {
    wav[HEADER..].as_chunks::<3>().0.iter().map(|b| (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / SCALE).collect()
}

fn left(wav: &[u8]) -> Vec<f32> {
    stereo(wav).into_iter().step_by(2).collect()
}

fn last_frame_over(stereo: &[f32], level: f32) -> usize {
    stereo.iter().rposition(|sample| sample.abs() > level).expect("wanted the export to carry sound") / 2
}

fn settled(export: &Export) {
    let started = Instant::now();
    while !export.finished() {
        assert!(started.elapsed() < PATIENCE, "wanted the background export finished within {PATIENCE:?}");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn session(project: &Project, range: Option<(i32, i32)>) -> String {
    let range = range.map_or_else(String::new, |(start, end)| format!(",\"loopStartBeat\":{},\"loopEndBeat\":{}", beats(start), beats(end)));
    format!("{{\"project\":{}{range}}}", project.to_json().write())
}

fn resumed(project: &Project, range: Option<(i32, i32)>) -> App {
    let mut app = App::new();
    assert!(app.resume(&session(project, range)), "wanted the session to resume");
    app.draw(WIDTH, HEIGHT);
    app.take_requests();
    app
}

fn panel_shown(app: &mut App) -> bool {
    painted(app).iter().any(|fill| fill.tone == Tone::Ground && fill.rect.w == PANEL_W && fill.rect.x == (WIDTH - PANEL_W) / 2)
}

fn delivered(app: &mut App) -> (String, Vec<u8>) {
    let started = Instant::now();
    loop {
        app.tick(TICK);
        for request in app.take_requests() {
            if let Request::Exported { path, bytes } = request {
                return (path, bytes);
            }
        }
        assert!(started.elapsed() < PATIENCE, "wanted the app to hand the export over within {PATIENCE:?}");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn click(app: &mut App, (x, y): (i32, i32)) {
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    app.draw(WIDTH, HEIGHT);
}

#[test]
fn a_loop_export_is_exactly_the_loop_long_and_fades_at_the_cut() {
    let wav = support::bounce(&looped(), &[], LOOP, Tail::Cut).expect("wanted the loop to export");
    let wanted = beat_frame(LOOP_BEATS);
    assert_eq!(frames(&wav), wanted, "wanted the loop export {wanted} frames long with the stray clip left out, got {}", frames(&wav));
    let samples = left(&wav);
    let end = samples.len();
    let loud = peak_of(&samples[end - BLOCK..end - FADE as usize]);
    assert!(loud > AUDIBLE, "wanted the held note still sounding at the loop's end, got a peak of {loud}");
    let last = samples[end - 1].abs();
    assert!(last <= loud / FADE * 2.0, "wanted the cut faded to a sixty-fourth of {loud} at frame {}, got {last}", end - 1);
}

#[test]
fn a_song_export_rings_out_and_ends_two_seconds_into_silence() {
    let project = project(vec![held(4.0)], vec![midi(Arp::DEFAULT), Row::Core(core_layer(0.8, patch(40.0)))]);
    let wav = support::bounce(&project, &[], (0, project.end()), Tail::Rings).expect("wanted the song to export");
    let song = beat_frame(4);
    let got = frames(&wav);
    assert!(got >= song + HOLD, "wanted at least {} frames, the song and two seconds, got {got}", song + HOLD);
    assert!(got <= song + HOLD + 3 * BLOCK, "wanted the export to end within three blocks of frame {}, got {got}", song + HOLD);
    let samples = left(&wav);
    let tail = peak_of(&samples[got - HOLD..]);
    assert!(tail <= SILENCE, "wanted the last two seconds under {SILENCE}, got a peak of {tail}");
    assert!(peak_of(&samples[..song]) > AUDIBLE, "wanted the note heard before frame {song}");
}

#[test]
fn the_ring_out_counts_a_tail_under_sixty_decibels_as_silence() {
    let mut reverb = Slot { wet: 0.5, ..Slot::new(Kind::Reverb) };
    assert!(reverb.params.set_travel("SPACE", "DECAY", DECAY_TRAVEL), "wanted the reverb's decay set");
    let project = project(vec![held(4.0)], vec![midi(Arp::DEFAULT), Row::Core(core_layer(0.8, patch(40.0))), Row::Effect(reverb)]);
    let wav = support::bounce(&project, &[], (0, project.end()), Tail::Rings).expect("wanted the song to export");
    let samples = stereo(&wav);
    let got = frames(&wav);
    let loud_until = last_frame_over(&samples, SILENCE);
    assert!(got >= loud_until + HOLD, "wanted two seconds past the last sample over {SILENCE} at frame {loud_until}, got {got} frames");
    assert!(got <= loud_until + HOLD + 2 * BLOCK, "wanted the export to end within two blocks of frame {}, got {got}", loud_until + HOLD);
    let faint = peak_of(&samples[2 * (got - HOLD)..]);
    assert!(faint > FAINT, "wanted the reverb still ringing under {SILENCE} through the hold, got a peak of {faint}");
}

#[test]
fn a_background_export_counts_up_and_delivers_the_inline_bytes() {
    let project = looped();
    let inline = support::bounce(&project, &[], LOOP, Tail::Cut).expect("wanted the loop to export");
    let mut export = Export::start(&project, &[], LOOP, Tail::Cut).expect("wanted the background export to start");
    settled(&export);
    let progress = export.progress();
    assert_eq!(progress, 1.0, "wanted the finished export at full progress, got {progress}");
    let wav = export.take().expect("wanted the background export to deliver its bytes");
    let divergence = inline.iter().zip(&wav).position(|(a, b)| a != b);
    assert!(wav.len() == inline.len() && divergence.is_none(), "wanted the background bytes to match the inline ones, differ at {divergence:?}, lengths {} and {}", inline.len(), wav.len());
}

#[test]
fn a_cancelled_export_delivers_nothing() {
    let mut export = Export::start(&looped(), &[], TWO_MINUTES, Tail::Cut).expect("wanted the background export to start");
    export.cancel();
    settled(&export);
    let progress = export.progress();
    assert!(progress < 1.0, "wanted the cancelled export stopped short, got progress {progress}");
    assert!(export.take().is_none(), "wanted no bytes from a cancelled export");
}

#[test]
fn one_song_exports_the_same_bytes_every_time_with_several_exports_running_at_once() {
    let project = looped();
    let first = support::bounce(&project, &[], MID_NOTE, Tail::Rings).expect("wanted the song to export");
    assert!(peak_of(&left(&first)[beat_frame(2)..beat_frame(2) + BLOCK]) > SILENCE, "wanted the held note still ringing past the span's end, so the tail hangs on when the stop is taken");
    let apart: usize = std::thread::scope(|scope| {
        let runs: Vec<_> = (0..AT_ONCE).map(|_| scope.spawn(|| (0..EXPORTS_EACH).filter(|_| support::bounce(&project, &[], MID_NOTE, Tail::Rings).as_ref() != Some(&first)).count())).collect();
        runs.into_iter().map(|run| run.join().expect("wanted every export to finish")).sum()
    });
    assert_eq!(apart, 0, "wanted each of {} exports of one song, {AT_ONCE} at a time, to give the first export's bytes, but {apart} came out different", AT_ONCE * EXPORTS_EACH);
}

#[test]
fn the_app_shows_the_export_bar_and_hands_the_loop_to_the_shell() {
    let project = looped();
    let mut app = resumed(&project, Some(LOOP));
    app.export_to(String::from(PATH));
    assert!(panel_shown(&mut app), "wanted the export bar painted centred while the export runs");
    let (path, wav) = delivered(&mut app);
    assert_eq!(path, PATH, "wanted the file handed over at the chosen path");
    let wanted = beat_frame(LOOP_BEATS);
    assert_eq!(frames(&wav), wanted, "wanted the app's loop export {wanted} frames long, got {}", frames(&wav));
    assert!(!panel_shown(&mut app), "wanted the export bar gone once the file is handed over");
}

#[test]
fn escape_cancels_the_export_and_nothing_is_handed_over() {
    let mut app = resumed(&looped(), Some(TWO_MINUTES));
    app.export_to(String::from(PATH));
    assert!(panel_shown(&mut app), "wanted the export bar painted before the cancel");
    app.event(Event::Key { key: Key::Escape, shift: false, command: false, repeat: false });
    assert!(!panel_shown(&mut app), "wanted the export bar gone on Escape");
    for _ in 0..60 {
        app.tick(TICK);
    }
    let handed = app.take_requests().into_iter().filter(|request| matches!(request, Request::Exported { .. })).count();
    assert_eq!(handed, 0, "wanted nothing handed over after the cancel, got {handed} files");
}

#[test]
fn export_audio_asks_the_shell_where_the_loop_goes() {
    let mut app = resumed(&looped(), Some(LOOP));
    click(&mut app, FILE_CELL_CENTRE);
    click(&mut app, EXPORT_AUDIO_ITEM_CENTRE);
    let asked: Vec<(String, String)> = app.take_requests().into_iter().filter_map(|request| match request {
        Request::Export { name, message } => Some((name, message)),
        _ => None,
    }).collect();
    assert_eq!(asked.len(), 1, "wanted one place asked for after File then Export Audio, got {}", asked.len());
    assert_eq!(asked[0].1, "Export Loop · 2 Bars", "wanted the panel told the span, got {:?}", asked[0].1);
    assert!(asked[0].0.ends_with(".wav"), "wanted a WAV name offered, got {:?}", asked[0].0);
}
