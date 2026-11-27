use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer, Tail};
use bp::fx::params::{Delay, DelayMode, Freak, FreakSlope, Lookahead};
use bp::fx::{Kind, Params, Slot};
use bp::project::{Aim, Clip, Curve, Node, Project, Row, TICKS_PER_BEAT, TRACKS, Timing, Track, gain_to_position};
use bp::wav::Wave;
use std::f64::consts::TAU;
use std::sync::Arc;
use support::heap::heap_calls;
use support::{BLOCK, SAMPLE_RATE, SILENT, beat_frame};

const CLICK: i16 = 8_192;
const LEVEL: f32 = 0.25;
const HALF_LEVEL: f32 = 0.125;
const TONE_HZ: f64 = 1_000.0;
const LIN: usize = 431;
const LIMITER: usize = 71;
const LOOK: usize = 479;
const LOOK_MS: f64 = 10.0;
const SPLICE: usize = 64;
const FADE: usize = 64;
const QUEUE: usize = 4_096;
const TICK_SECONDS: f32 = 1.0 / 60.0;
const PHASES: [usize; 6] = [0, 8, 16, 24, 32, 40];
const BEFORE: usize = 94 * BLOCK;
const HELD: usize = 8 * BLOCK;
const WATCH: usize = 6_000;
const SETTLED: usize = 6_000;
const AROUND: usize = 1_024;
const STEPS: f32 = 4.0;
const ALIGNED: f32 = 1e-3;
const EXACT: f32 = 1e-6;
const CLOSE: f32 = 1e-4;
const SPAN_BEATS: i32 = 4;
const HALF_TICKS: i32 = 20;
const FILE_BEATS: usize = 8;
const DUCK_FROM_DB: f64 = -60.0;
const DUCK_RATIO: f64 = 20.0;
const FAST_MS: f64 = 0.1;
const BURST: usize = 301;
const HEADER: usize = 68;
const BYTES_PER_FRAME: usize = 6;
const SCALE: f32 = 8_388_608.0;
const SHOWN: usize = 8;

fn mono(name: &str, pcm: Vec<i16>) -> Arc<Wave> {
    Arc::new(Wave::sound(String::from(name), SAMPLE_RATE, 1, pcm))
}

fn file_frames() -> usize {
    beat_frame(FILE_BEATS)
}

fn clicks(name: &str, at: &[usize]) -> Arc<Wave> {
    let mut pcm = vec![0; file_frames()];
    for &frame in at {
        pcm[frame] = CLICK;
    }
    mono(name, pcm)
}

fn sine(name: &str, hz: f64) -> Arc<Wave> {
    mono(name, (0..file_frames()).map(|frame| ((TAU * hz * frame as f64 / SAMPLE_RATE).sin() * f64::from(CLICK)).round() as i16).collect())
}

fn held(name: &str) -> Arc<Wave> {
    mono(name, vec![CLICK; file_frames()])
}

fn playing(name: &str, rack: Vec<Row>) -> Track {
    Track { rack, clips: vec![Clip::audio(0, FILE_BEATS as i32 * TICKS_PER_BEAT, String::from(name), String::from(name))], ..Track::audio() }
}

fn silent(rack: Vec<Row>) -> Track {
    Track { rack, ..Track::audio() }
}

fn project(tracks: Vec<Track>) -> Project {
    let mut project = Project::new();
    project.tracks = tracks;
    project
}

fn effect(params: Params) -> Row {
    Row::Effect(Slot { params, ..Slot::new(params.kind()) })
}

fn dry() -> Row {
    Row::Effect(Slot::new(Kind::Utility))
}

fn limiter(ms: f64) -> Row {
    let mut row = dry();
    if let Row::Effect(slot) = &mut row
        && let Params::Utility(p) = &mut slot.params
    {
        p.limit = true;
        p.hard = false;
        p.ceiling_db = 0.0;
        p.lookahead_ms = ms;
    }
    row
}

fn linear_crossover() -> Row {
    Row::Effect(Slot { params: Params::Freak(Freak { slope: FreakSlope::Linear, oversample: false, ..Freak::DEFAULT }), ..Slot::new(Kind::Freak) })
}

fn ducker(source: usize, pre: bool) -> Row {
    let mut slot = Slot::new(Kind::Sidechain);
    if let Params::Sidechain(p) = &mut slot.params {
        p.threshold_db = DUCK_FROM_DB;
        p.ratio = DUCK_RATIO;
        p.attack_ms = FAST_MS;
        p.lookahead = Lookahead::Off;
    }
    slot.wire(Some(source), pre);
    Row::Effect(slot)
}

fn reversed_delay() -> Row {
    effect(Params::Delay(Delay { mode: DelayMode::Clean, time_l_ms: 500.0, time_r_ms: 500.0, feedback: 0.0, forward: 0.0, reverse: 1.0, ..Delay::DEFAULT }))
}


fn step(aim: Aim, at_ticks: i32, from: f32, to: f32) -> Curve {
    let nodes = vec![Node { tick: 0, travel: from, bend: 0.0 }, Node { tick: at_ticks, travel: from, bend: 0.0 }, Node { tick: at_ticks, travel: to, bend: 0.0 }];
    Curve { aim, lfo: None, nodes }
}

struct Session {
    audio: Audio,
    renderer: Renderer,
    waves: Vec<Arc<Wave>>,
    out: Vec<f32>,
}

impl Session {
    fn open(project: &Project, waves: &[Arc<Wave>]) -> Session {
        let (mut audio, renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
        audio.sync(project, None, &[], waves);
        Session { audio, renderer, waves: waves.to_vec(), out: Vec::new() }
    }

    fn playing(project: &Project, waves: &[Arc<Wave>]) -> Session {
        let mut session = Session::open(project, waves);
        session.audio.play(0.0);
        session
    }

    fn sync(&mut self, project: &Project) {
        self.audio.sync(project, None, &[], &self.waves);
    }

    fn render(&mut self, frames: usize) {
        let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
        let mut done = 0;
        while done < frames {
            let n = BLOCK.min(frames - done);
            self.renderer.render(&mut l[..n], &mut r[..n]);
            self.out.extend_from_slice(&l[..n]);
            done += n;
        }
    }

    fn output(&self) -> usize {
        self.audio.timing().output
    }
}

fn rendered(project: &Project, waves: &[Arc<Wave>], frames: usize) -> (Vec<f32>, usize) {
    let mut session = Session::playing(project, waves);
    session.render(frames);
    let output = session.output();
    (session.out, output)
}

fn left(wav: &[u8]) -> Vec<f32> {
    wav[HEADER..].as_chunks::<BYTES_PER_FRAME>().0.iter().map(|frame| (i32::from_le_bytes([0, frame[0], frame[1], frame[2]]) >> 8) as f32 / SCALE).collect()
}

fn exported(project: &Project, waves: &[Arc<Wave>], end_ticks: i32) -> Vec<f32> {
    left(&support::bounce(project, waves, (0, end_ticks), Tail::Cut).expect("wanted the project to export"))
}

fn worst(a: &[f32], b: &[f32], from: usize) -> (usize, f32) {
    a.iter().zip(b).enumerate().skip(from).map(|(frame, (x, y))| (frame, (x - y).abs())).fold((from, 0.0), |worst, next| if next.1 > worst.1 { next } else { worst })
}

fn loud(out: &[f32], floor: f32) -> Vec<(usize, f32)> {
    out.iter().enumerate().filter(|(_, sample)| sample.abs() > floor).map(|(frame, &sample)| (frame, sample)).collect()
}

fn largest_step(out: &[f32], from: usize, to: usize) -> (usize, f32) {
    (from..to - 1).map(|t| (t + 1, (out[t + 1] - out[t]).abs())).fold((from, 0.0), |worst, next| if next.1 > worst.1 { next } else { worst })
}

fn own_step(wave: &Wave) -> f32 {
    wave.pcm.windows(2).map(|pair| (f32::from(pair[1]) - f32::from(pair[0])).abs() / 32_768.0).fold(0.0f32, f32::max)
}

fn misaligned(out: &[f32], wave: &Wave, late: usize, from: usize, to: usize) -> (usize, f32) {
    (from..to).map(|t| (t, (out[t] - 2.0 * f32::from(wave.pcm[t - late]) / 32_768.0).abs())).fold((from, 0.0), |worst, next| if next.1 > worst.1 { next } else { worst })
}

#[test]
fn an_export_reads_the_master_gain_of_the_audio_still_in_its_lines() {
    let waves = [held("held")];
    let end = SPAN_BEATS * TICKS_PER_BEAT;
    let mut project = project(vec![playing("held", vec![limiter(LOOK_MS)])]);
    let (unity, half) = (gain_to_position(1.0), gain_to_position(0.5));
    let nodes = vec![Node { tick: 0, travel: unity, bend: 0.0 }, Node { tick: end - HALF_TICKS, travel: unity, bend: 0.0 }, Node { tick: end - HALF_TICKS, travel: half, bend: 0.0 }, Node { tick: end, travel: half, bend: 0.0 }, Node { tick: end, travel: 0.0, bend: 0.0 }];
    project.master.automation.curves = vec![Curve { aim: Aim::Gain, lfo: None, nodes }];
    let output = Timing::of(&project, SAMPLE_RATE).output;
    assert_eq!(output, LOOK, "wanted a 10 ms limiter to set the output latency to {LOOK}, got {output}");
    let song = beat_frame(SPAN_BEATS as usize);
    let short = exported(&project, &waves, end);
    let long = exported(&project, &waves, end + TICKS_PER_BEAT);
    assert_eq!(short.len(), song, "wanted the export {song} frames long, got {}", short.len());
    let flushed = song - LOOK / 2;
    assert!((short[flushed] - HALF_LEVEL).abs() <= CLOSE, "wanted file frame {flushed}, rendered after the transport stopped while still in the limiter's line, at the half gain of its own time, {HALF_LEVEL}, got {}", short[flushed]);
    let (frame, delta) = worst(&short[..song - FADE], &long[..song - FADE], 0);
    assert!(delta <= EXACT, "wanted the export to match one whose transport plays on past it before the cut fade, got a difference of {delta} at file frame {frame}: {} against {}", short[frame], long[frame]);
}

#[test]
fn a_stop_leaves_the_audio_in_the_lines_under_the_gain_of_its_own_time() {
    let waves = [held("held")];
    let mut project = project(vec![playing("held", vec![limiter(LOOK_MS)])]);
    project.master.automation.curves = vec![step(Aim::Gain, TICKS_PER_BEAT, gain_to_position(1.0), gain_to_position(0.5))];
    let stop = BEFORE;
    let mut played = Session::playing(&project, &waves);
    played.render(stop + 2 * LOOK);
    let mut stopped = Session::playing(&project, &waves);
    stopped.render(stop);
    stopped.audio.stop();
    stopped.render(2 * LOOK);
    assert_eq!(stopped.output(), LOOK, "wanted the output latency at {LOOK}, got {}", stopped.output());
    let landed = beat_frame(1) + LOOK;
    let halved = (stop..stop + LOOK).find(|&frame| stopped.out[frame].abs() < (LEVEL + HALF_LEVEL) / 2.0);
    assert_eq!(halved, Some(landed), "wanted the master gain step at beat 1 heard at frame {landed} with its audio after a stop at frame {stop}, got it at {halved:?}");
    let (frame, delta) = worst(&stopped.out[stop..stop + LOOK], &played.out[stop..stop + LOOK], 0);
    assert!(delta <= EXACT, "wanted the {LOOK} frames still in the lines after a stop at frame {stop} as when the transport plays on, got a difference of {delta} at frame {}", stop + frame);
}

#[test]
fn a_sync_split_by_a_full_queue_installs_its_rows_only_with_their_plan() {
    let wave = sine("sine", TONE_HZ);
    let waves = [Arc::clone(&wave)];
    let first = project(vec![playing("sine", Vec::new()), playing("sine", Vec::new())]);
    let second = project(vec![playing("sine", Vec::new()), playing("sine", vec![limiter(1.5)])]);
    let own = own_step(&wave);
    for phase in PHASES {
        let mut session = Session::playing(&first, &waves);
        let change = BEFORE + phase;
        session.render(change);
        for _ in 0..QUEUE - 1 {
            session.audio.preview_gain(1.0);
        }
        session.sync(&second);
        session.render(HELD);
        let (frame, delta) = misaligned(&session.out, &wave, 0, change, change + HELD);
        assert!(delta <= ALIGNED, "phase {phase}: wanted nothing of a sync installed while its plan was still held back by the full queue, the two sines still summing in time until frame {}, got a difference of {delta} at frame {frame}", change + HELD);
        session.audio.tick(TICK_SECONDS);
        session.render(WATCH + SETTLED);
        let (at, largest) = largest_step(&session.out, change - AROUND, change + HELD + WATCH);
        assert!(largest <= STEPS * own, "phase {phase}: wanted the limiter inserted behind one splice, no step past {STEPS} times the sine's own {own}, got {largest} at frame {at}");
        let settled = change + HELD + WATCH;
        let (frame, delta) = misaligned(&session.out, &wave, LIMITER, settled, settled + SETTLED);
        assert!(delta <= ALIGNED, "phase {phase}: wanted both sines aligned {LIMITER} late once the plan landed, got a difference of {delta} at frame {frame}");
    }
}

fn keyed_rows(name: &str, before: Vec<Row>, after: Vec<Row>) {
    let n = beat_frame(1);
    let waves = [clicks("clicks", &[n])];
    let keyed = |rack: Vec<Row>| project(vec![silent(vec![linear_crossover()]), silent(vec![linear_crossover(), linear_crossover()]), playing("clicks", rack)]);
    let (first, second) = (keyed(before), keyed(after));
    let timing = Timing::of(&second, SAMPLE_RATE);
    let lines: Vec<usize> = timing.tracks[2].rack.rows.iter().map(|row| row.before).filter(|&before| before > 0).collect();
    assert_eq!(lines, vec![LIN, LIN], "{name}: wanted two keyed rows each behind a {LIN}-sample line, got lines {lines:?}");
    let frames = n + 4 * LIN;
    let (reference, output) = rendered(&second, &waves, frames);
    let heard = loud(&reference, SILENT);
    assert_eq!(heard, vec![(n + 2 * LIN, LEVEL)], "{name}: wanted the click through both keyed rows heard once at frame {}, got {heard:?}", n + 2 * LIN);
    let mut session = Session::playing(&first, &waves);
    let change = BEFORE;
    session.render(change);
    session.sync(&second);
    session.render(frames - change);
    assert_eq!(session.output(), output, "{name}: wanted the output latency at {output}, got {}", session.output());
    let (frame, delta) = worst(&session.out, &reference, change + 2 * SPLICE);
    assert!(delta <= EXACT, "{name}: wanted the click inside the first keyed row's line at the change, frame {change}, to carry on with that row, heard as without the change after the splice, got a difference of {delta} at frame {frame}: {} against {}", session.out[frame], reference[frame]);
}

#[test]
fn a_row_inserted_above_two_keyed_rows_leaves_each_its_own_line() {
    keyed_rows("an insert above", vec![ducker(0, false), ducker(1, false)], vec![dry(), ducker(0, false), ducker(1, false)]);
}

#[test]
fn a_row_removed_above_two_keyed_rows_leaves_each_its_own_line() {
    keyed_rows("a removal above", vec![dry(), ducker(0, false), ducker(1, false)], vec![ducker(0, false), ducker(1, false)]);
}

#[test]
fn a_row_moved_above_two_keyed_rows_leaves_each_its_own_line() {
    keyed_rows("a move to the top", vec![ducker(0, false), ducker(1, false), dry()], vec![dry(), ducker(0, false), ducker(1, false)]);
}

#[test]
fn a_reversed_delay_behind_a_latent_row_lands_shifted_by_exactly_its_latency() {
    let n = beat_frame(2);
    let waves = [clicks("clicks", &[n])];
    let frames = n + LIN + beat_frame(1);
    let (dry_out, dry_output) = rendered(&project(vec![playing("clicks", vec![reversed_delay()])]), &waves, frames);
    let (late_out, late_output) = rendered(&project(vec![playing("clicks", vec![linear_crossover(), reversed_delay()])]), &waves, frames);
    assert_eq!((dry_output, late_output), (0, LIN), "wanted the reversed delay alone at no latency and behind the crossover at {LIN}, got {dry_output} and {late_output}");
    let echo = loud(&dry_out[..n - beat_frame(1) / 2], ALIGNED).first().map(|(frame, _)| *frame);
    assert!(echo.is_some(), "wanted the reversed echo heard before the click with no latent row, got nothing above {ALIGNED} before frame {}", n - beat_frame(1) / 2);
    let (frame, delta) = worst(&late_out[LIN..], &dry_out[..frames - LIN], 0);
    assert!(delta <= CLOSE, "wanted the reversed delay behind the crossover to sound as alone, shifted by exactly {LIN}, got a difference of {delta} at frame {}: {} against {}", frame + LIN, late_out[frame + LIN], dry_out[frame]);
}

#[test]
fn a_latent_member_keyed_from_its_own_group_plans_and_renders() {
    let waves = [clicks("clicks", &[beat_frame(1)])];
    let mut member = playing("clicks", vec![limiter(1.5), ducker(0, false)]);
    member.inside = true;
    let grouped = project(vec![Track::group(), member]);
    assert_eq!(grouped.group_of(1), Some(0), "wanted the member inside the group, got {:?}", grouped.group_of(1));
    let timing = Timing::of(&grouped, SAMPLE_RATE);
    let (to_group, uncompensated) = (timing.tracks[1].to_group, timing.tracks[1].uncompensated);
    assert_eq!((to_group, uncompensated), (0, 0), "wanted the member planned before its group, its {LIMITER} compensated as any member's is, got to_group {to_group} and uncompensated {uncompensated}");
    let (out, _) = rendered(&grouped, &waves, beat_frame(2));
    assert_eq!(out.len(), beat_frame(2), "wanted the grouped key cycle to render {} frames, got {}", beat_frame(2), out.len());
    assert!(out.iter().any(|sample| sample.abs() > 0.0), "wanted the member heard through the group it keys from, got silence");
}

#[test]
fn an_empty_return_plays_its_line_out_and_replays_nothing_when_its_send_returns() {
    let n = beat_frame(1);
    let waves = [clicks("clicks", &[n])];
    let sending = |amount: f32| {
        let mut sender = playing("clicks", Vec::new());
        sender.sends[0] = amount;
        let mut project = project(vec![sender]);
        project.sends[1].rack = vec![linear_crossover()];
        project
    };
    let to_master = Timing::of(&sending(1.0), SAMPLE_RATE).return_to_master[0];
    assert_eq!(to_master, LIN, "wanted the empty return held {LIN} into the master beside the crossover on the other, got {to_master}");
    let mut session = Session::playing(&sending(1.0), &waves);
    let stop = BEFORE;
    session.render(stop);
    session.sync(&sending(0.0));
    let again = 2 * BEFORE;
    session.render(again - stop);
    session.sync(&sending(1.0));
    session.render(beat_frame(1));
    let heard = loud(&session.out, SILENT);
    assert_eq!(heard, vec![(n + LIN, 2.0 * LEVEL)], "wanted the click at frame {n} heard once at frame {}, its send still in the return's line when the send stopped at frame {stop} and nothing replayed when it returned at frame {again}, got {:?}", n + LIN, &heard[..heard.len().min(SHOWN)]);
}

#[test]
fn a_burst_of_plans_never_frees_memory_on_the_audio_thread() {
    let n = beat_frame(1);
    let waves = [clicks("clicks", &[n])];
    let burst = |limited: bool| {
        let mut tracks = vec![playing("clicks", vec![if limited { limiter(1.5) } else { dry() }])];
        tracks.extend((1..TRACKS).map(|_| silent(Vec::new())));
        project(tracks)
    };
    let mut session = Session::open(&burst(false), &waves);
    session.render(BLOCK);
    session.audio.reclaim();
    for at in 0..BURST {
        session.sync(&burst(at % 2 == 0));
    }
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let freed = heap_calls(|| {
        for _ in 0..4 {
            session.renderer.render(&mut l, &mut r);
        }
    });
    assert_eq!(freed, 0, "wanted {BURST} plans synced without a reclaim between them never to free memory in the render, got {freed} heap calls in four blocks");
    for _ in 0..64 {
        session.audio.reclaim();
        session.renderer.render(&mut l, &mut r);
    }
    session.audio.play(0.0);
    session.out.clear();
    session.render(n + 2 * BLOCK);
    let heard = loud(&session.out, SILENT);
    assert_eq!(heard, vec![(n + LIMITER, LEVEL)], "wanted the last of the burst's plans in force once reclaimed, the click at frame {n} heard {LIMITER} late at frame {}, got {heard:?}", n + LIMITER);
}
