use bp::arp::{Arp, Articulation};
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::core::Params as CoreParams;
use bp::division::Division;
use bp::face::Drawn;
use bp::fx::params::{Freak, FreakSlope};
use bp::fx::{Kind, Params, Slot};
use bp::project::{Aim, Clip, Curve, LaneLfo, LfoClock, LfoShape, Node, Note, Project, Row, TICKS_PER_BEAT, Track, gain_to_position};
use bp::wav::Wave;
use std::sync::Arc;
use support::{BLOCK, FRAMES_PER_BEAT, NO_END, SAMPLE_RATE, beat_frame, core_layer, midi, peak_of};

const HELD: &str = "held";
const LEVEL: i16 = 8_192;
const HEIGHT: f32 = 0.25;
const FILE_BEATS: usize = 8;
const LINEAR: usize = 431;
const LIMITER: usize = 71;
const ALIKE: f32 = 1e-4;
const LEVELLED: f32 = 1e-3;
const OPEN: f32 = HEIGHT / 2.0;
const SEEK_AT: usize = 36_096;
const OLD_STEP_BEAT: f64 = 1.5;
const NEW_STEP_BEAT: f64 = 2.5;
const LANDING_BEAT: f64 = 3.0;
const PLAYED_ON: usize = 4_000;
const WATCH: usize = 2_000;
const HALF: f32 = 0.5;
const QUARTER: f32 = 0.25;
const NOTE_LOOP: (f64, f64) = (1.0, 3.0);
const NOTE_A: (u8, f32, f32) = (60, 1.0, 1.0);
const NOTE_B: (u8, f32, f32) = (67, 2.75, 0.25);
const CORE_LEVEL: f64 = 0.25;
const GAIN_ROW: usize = 3;
const WATCHED: usize = 4_800;
const RETRIGGER_ALIKE: f32 = 0.02;
const LANE_STEP: usize = 64;
const AUDIBLE: f32 = 0.01;
const TICK_LOOP: (f64, f64) = (1.0, 3.01);
const LAST_TICK_BEAT: f64 = 3.0;
const RINGING: usize = 1_000;

fn held() -> Arc<Wave> {
    Arc::new(Wave::sound(String::from(HELD), SAMPLE_RATE, 1, vec![LEVEL; beat_frame(FILE_BEATS)]))
}

fn playing(rack: Vec<Row>) -> Track {
    Track { rack, clips: vec![Clip::audio(0, FILE_BEATS as i32 * TICKS_PER_BEAT, String::from(HELD), String::from(HELD))], ..Track::audio() }
}

fn linear_crossover() -> Row {
    Row::Effect(Slot { params: Params::Freak(Freak { slope: FreakSlope::Linear, oversample: false, ..Freak::DEFAULT }), ..Slot::new(Kind::Freak) })
}

fn limiter(bypass: bool) -> Row {
    let mut slot = Slot::new(Kind::Utility);
    if let Params::Utility(p) = &mut slot.params {
        p.limit = true;
        p.hard = false;
        p.ceiling_db = 0.0;
        p.lookahead_ms = 1.5;
    }
    Row::Effect(Slot { bypass, ..slot })
}

fn project(tracks: Vec<Track>) -> Project {
    let mut project = Project::new();
    project.tracks = tracks;
    project
}

fn frame_of(beat: f64) -> usize {
    (beat * FRAMES_PER_BEAT).round() as usize
}

fn ticks(beat: f64) -> i32 {
    (beat * f64::from(TICKS_PER_BEAT)).round() as i32
}

struct Session {
    audio: Audio,
    renderer: Renderer,
    out: Vec<f32>,
}

impl Session {
    fn playing(project: &Project, waves: &[Arc<Wave>], range: Option<(f64, f64)>) -> Session {
        let (mut audio, renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
        audio.sync(project, None, &[], waves);
        audio.set_loop(range);
        audio.play(0.0);
        Session { audio, renderer, out: Vec::new() }
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

fn worst(a: &[f32], b: &[f32], from: usize, to: usize) -> (usize, f32) {
    (from..to).map(|frame| (frame, (a[frame] - b[frame]).abs())).fold((from, 0.0), |worst, next| if next.1 > worst.1 { next } else { worst })
}

fn farthest(out: &[f32], from: usize, to: usize, wanted: f32) -> (usize, f32) {
    (from..to).map(|frame| (frame, out[frame])).fold((from, wanted), |worst, next| if (next.1 - wanted).abs() > (worst.1 - wanted).abs() { next } else { worst })
}

#[test]
fn a_fader_curve_behind_latency_reads_the_old_position_for_the_audio_played_out_after_a_seek() {
    let (unity, half, quarter) = (gain_to_position(1.0), gain_to_position(HALF), gain_to_position(QUARTER));
    let (old_step, new_step) = (ticks(OLD_STEP_BEAT), ticks(NEW_STEP_BEAT));
    let nodes = vec![Node { tick: 0, travel: unity, bend: 0.0 }, Node { tick: old_step, travel: unity, bend: 0.0 }, Node { tick: old_step, travel: half, bend: 0.0 }, Node { tick: new_step, travel: half, bend: 0.0 }, Node { tick: new_step, travel: quarter, bend: 0.0 }];
    let mut fading = playing(vec![linear_crossover()]);
    fading.automation.curves = vec![Curve { aim: Aim::Gain, lfo: None, nodes }];
    let mut session = Session::playing(&project(vec![fading]), &[held()], None);
    session.render(SEEK_AT);
    session.audio.play(LANDING_BEAT);
    session.render(PLAYED_ON);
    assert_eq!(session.output(), LINEAR, "wanted the output latency at {LINEAR}, got {}", session.output());
    let out = &session.out;
    let (step_heard, played_out) = (frame_of(OLD_STEP_BEAT) + LINEAR, SEEK_AT + LINEAR);
    let (frame, got) = farthest(out, SEEK_AT, step_heard, HEIGHT);
    assert!((got - HEIGHT).abs() <= LEVELLED, "wanted the audio played out after the seek at frame {SEEK_AT} under the old position's unity fader until its step at beat {OLD_STEP_BEAT} is heard at frame {step_heard}, {HEIGHT}, got {got} at frame {frame}");
    let (frame, got) = farthest(out, step_heard, played_out, HEIGHT * HALF);
    assert!((got - HEIGHT * HALF).abs() <= LEVELLED, "wanted the old position's half fader from frame {step_heard} until its audio has played out at frame {played_out}, {}, got {got} at frame {frame}", HEIGHT * HALF);
    let settled = played_out + BLOCK;
    let (frame, got) = farthest(out, settled, settled + WATCH, HEIGHT * QUARTER);
    assert!((got - HEIGHT * QUARTER).abs() <= LEVELLED, "wanted the new position's quarter fader on its audio from frame {played_out}, past the seek's crossfade from frame {settled}, {}, got {got} at frame {frame}", HEIGHT * QUARTER);
}

fn ramp_restarting_on_each_note() -> LaneLfo {
    LaneLfo { amount: 0.125, shape: LfoShape::Ramp, sync: false, division: Division::Quarter, rate_hz: 4.0, phase: 0.0, centre: 0.5, retrigger: true, legato: false, once: true, clock: LfoClock::Song, audio: false, fade_ms: 0.0, lag_ms: 0.0, points: Drawn::RAMP, ..LaneLfo::blank(Aim::knob(GAIN_ROW, "STRIP", "GAIN")) }
}

fn retriggered(latent: bool) -> Project {
    let mut patch = CoreParams::DEFAULT;
    patch.amp.attack_ms = 1.0;
    patch.amp.release_ms = 1.0;
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(CORE_LEVEL, patch)), limiter(!latent), Row::Effect(Slot::new(Kind::Utility))];
    track.automation.lfos = vec![ramp_restarting_on_each_note()];
    let notes = [NOTE_A, NOTE_B].map(|(pitch, start, length)| Note { pitch, start, length, velocity: 127, articulation: Articulation::PLAIN }).to_vec();
    track.clips = vec![Clip::midi(0, String::new(), notes, NO_END)];
    project
}

#[test]
fn a_retriggered_lane_behind_a_limiter_follows_its_own_notes_across_a_loop_wrap() {
    let wrap = frame_of(NOTE_LOOP.1);
    let frames = wrap + WATCHED + LIMITER + BLOCK;
    let mut dry = Session::playing(&retriggered(false), &[], Some(NOTE_LOOP));
    let mut late = Session::playing(&retriggered(true), &[], Some(NOTE_LOOP));
    dry.render(frames);
    late.render(frames);
    assert_eq!((dry.output(), late.output()), (0, LIMITER), "wanted no latency with the limiter bypassed and {LIMITER} with it on, got {} and {}", dry.output(), late.output());
    let (from, to) = (wrap - WATCHED, wrap + WATCHED);
    let loudest = peak_of(&dry.out[from..to]);
    let tail = peak_of(&dry.out[wrap - LIMITER..wrap]);
    assert!(tail > AUDIBLE, "wanted the second note sounding through the dry track's last {LIMITER} frames before the wrap at frame {wrap}, got a peak of {tail}");
    let restarting = wrap - LANE_STEP..wrap + BLOCK;
    let (worst, at) = (from..to).filter(|frame| !restarting.contains(frame)).map(|frame| ((late.out[frame + LIMITER] - dry.out[frame]).abs(), frame)).fold((0.0f32, from), |(most, at), (delta, frame)| if delta > most { (delta, frame) } else { (most, at) });
    assert!(worst <= RETRIGGER_ALIKE * loudest, "wanted the latent track to follow the dry one {LIMITER} frames later around the wrap at frame {wrap}, its ramp on the old note through the old iteration's tail and restarting on the loop's first note after it, each lane restarting at the start of its own lane step and settling within a block, within {RETRIGGER_ALIKE} of the peak {loudest}; got {worst} at frame {at} of the dry track, {} against {}", late.out[at + LIMITER], dry.out[at]);
}

fn ticking(range: Option<(f64, f64)>, frames: usize) -> Session {
    let mut ticked = project(vec![Track { rack: vec![linear_crossover()], ..Track::audio() }]);
    ticked.metronome = true;
    let mut session = Session::playing(&ticked, &[], range);
    session.render(frames);
    session
}

#[test]
fn a_metronome_behind_latency_strikes_the_old_iterations_last_beat_in_its_play_out_after_a_wrap() {
    let wrap = frame_of(TICK_LOOP.1);
    let frames = wrap + LINEAR + RINGING;
    let (looped, straight) = (ticking(Some(TICK_LOOP), frames), ticking(None, frames));
    assert_eq!((looped.output(), straight.output()), (LINEAR, LINEAR), "wanted the output latency at {LINEAR}, got {} and {}", looped.output(), straight.output());
    let struck = frame_of(LAST_TICK_BEAT) + LINEAR;
    let rung = peak_of(&straight.out[struck..struck + RINGING]);
    assert!(wrap < struck && struck < wrap + LINEAR && rung > OPEN, "wanted the straight run's tick on beat {LAST_TICK_BEAT} heard at frame {struck}, inside the {LINEAR} frames after the wrap at frame {wrap}, got a peak of {rung} there");
    let (frame, delta) = worst(&looped.out, &straight.out, wrap, wrap + LINEAR);
    assert!(delta <= ALIKE, "wanted the metronome to strike beat {LAST_TICK_BEAT} at frame {struck} while the old iteration plays out after the wrap at frame {wrap}, as when the transport runs straight on, within {ALIKE}, got a difference of {delta} at frame {frame}: {} against {}", looped.out[frame], straight.out[frame]);
    let (first, head) = (frame_of(TICK_LOOP.0) + LINEAR, wrap + LINEAR);
    let (offset, delta) = (0..RINGING).map(|i| (i, (looped.out[head + i] - straight.out[first + i]).abs())).fold((0, 0.0f32), |worst, next| if next.1 > worst.1 { next } else { worst });
    assert!(delta <= ALIKE, "wanted the loop's first beat struck at frame {head}, {LINEAR} after the wrap, as the straight run strikes beat {} at frame {first}, within {ALIKE}, got a difference of {delta} {offset} frames in: {} against {}", TICK_LOOP.0, looped.out[head + offset], straight.out[first + offset]);
}
