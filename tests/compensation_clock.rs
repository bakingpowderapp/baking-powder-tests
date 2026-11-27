use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::fx::params::{Freak, FreakSlope};
use bp::fx::{self, Kind, Slot};
use bp::project::{Aim, Clip, Curve, Node, Project, Row, TICKS_PER_BEAT, Timing, Track, gain_to_position};
use bp::wav::Wave;
use std::sync::Arc;
use support::{BLOCK, SAMPLE_RATE, beat_frame};

const HELD_PATH: &str = "held.wav";
const CLICK_PATH: &str = "click.wav";
const LEVEL: i16 = 8_192;
const HEIGHT: f32 = 0.25;
const STEPPED_TO: f32 = 0.5;
const STEP_BEAT: usize = 1;
const FILE_BEATS: i32 = 4;
const FRAMES: usize = 72_000;
const SETTLED: usize = 4_800;
const LINEAR: usize = 431;
const LIMITER: usize = 71;
const RETURN: usize = 0;
const SENT: f32 = 1.0;
const SENDER: usize = 0;

fn held() -> Arc<Wave> {
    Arc::new(Wave::sound(String::from(HELD_PATH), SAMPLE_RATE, 1, vec![LEVEL; FILE_BEATS as usize * beat_frame(1)]))
}

fn click() -> Arc<Wave> {
    let mut pcm = vec![0; FILE_BEATS as usize * beat_frame(1)];
    pcm[beat_frame(STEP_BEAT)] = LEVEL;
    Arc::new(Wave::sound(String::from(CLICK_PATH), SAMPLE_RATE, 1, pcm))
}

fn waves() -> [Arc<Wave>; 2] {
    [held(), click()]
}

fn playing(path: &str, rack: Vec<Row>) -> Track {
    Track { rack, clips: vec![Clip::audio(0, FILE_BEATS * TICKS_PER_BEAT, String::from(path), String::from(path))], ..Track::audio() }
}

fn silent(rack: Vec<Row>) -> Track {
    Track { rack, ..Track::audio() }
}

fn linear_crossover() -> Row {
    Row::Effect(Slot { params: fx::Params::Freak(Freak { slope: FreakSlope::Linear, oversample: false, ..Freak::DEFAULT }), ..Slot::new(Kind::Freak) })
}

fn limiter() -> Row {
    let mut slot = Slot::new(Kind::Utility);
    if let fx::Params::Utility(p) = &mut slot.params {
        p.limit = true;
        p.hard = false;
        p.ceiling_db = 0.0;
        p.lookahead_ms = 1.5;
    }
    Row::Effect(slot)
}

fn step_at_the_beat() -> Curve {
    let at = STEP_BEAT as i32 * TICKS_PER_BEAT;
    let (from, to) = (gain_to_position(1.0), gain_to_position(STEPPED_TO));
    let nodes = vec![Node { tick: 0, travel: from, bend: 0.0 }, Node { tick: at, travel: from, bend: 0.0 }, Node { tick: at, travel: to, bend: 0.0 }];
    Curve { aim: Aim::Gain, lfo: None, nodes }
}

fn project(tracks: Vec<Track>) -> Project {
    let mut project = Project::new();
    project.tracks = tracks;
    project
}

fn render(project: &Project, solo: Option<usize>) -> (Vec<f32>, usize) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, solo, &[], &waves());
    let output = audio.timing().output;
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(FRAMES + BLOCK);
    while out.len() < FRAMES {
        renderer.render(&mut l, &mut r);
        out.extend_from_slice(&l);
    }
    out.truncate(FRAMES);
    (out, output)
}

fn loudest(out: &[f32]) -> usize {
    (0..out.len()).max_by(|&a, &b| out[a].abs().total_cmp(&out[b].abs())).unwrap_or(0)
}

fn stepped(out: &[f32]) -> Option<usize> {
    let between = HEIGHT * (1.0 + STEPPED_TO) / 2.0;
    (SETTLED..out.len()).find(|&frame| out[frame].abs() < between)
}

fn lands_with_its_click(stepped_project: &Project, clicked_project: &Project, solo: Option<usize>, late: usize, what: &str) {
    let n = beat_frame(STEP_BEAT);
    let (clicked, clicked_output) = render(clicked_project, solo);
    let (held, output) = render(stepped_project, solo);
    assert_eq!((output, clicked_output), (late, late), "{what}: wanted the output latency at {late}, got {output} with the step and {clicked_output} with the click");
    let click_at = loudest(&clicked);
    assert_eq!(click_at, n + late, "{what}: wanted the click at beat {STEP_BEAT} heard at frame {}, got frame {click_at}", n + late);
    let step_at = stepped(&held);
    assert_eq!(step_at, Some(click_at), "{what}: wanted the step automated at beat {STEP_BEAT} heard at the click's frame {click_at}, got {step_at:?}");
}

#[test]
fn a_fader_step_on_a_latent_track_lands_with_its_click() {
    let mut fading = playing(HELD_PATH, vec![linear_crossover()]);
    fading.automation.curves = vec![step_at_the_beat()];
    lands_with_its_click(&project(vec![fading]), &project(vec![playing(CLICK_PATH, vec![linear_crossover()])]), None, LINEAR, "a fader step behind a linear crossover");
}

#[test]
fn a_fader_step_on_a_latent_track_reaches_its_send_with_its_click() {
    let sending = |path: &str, curves: Vec<Curve>| {
        let mut sender = playing(path, vec![linear_crossover()]);
        sender.sends[RETURN] = SENT;
        sender.automation.curves = curves;
        project(vec![sender, silent(vec![linear_crossover(), limiter()])])
    };
    let timing = Timing::of(&sending(HELD_PATH, Vec::new()), SAMPLE_RATE);
    let (latency, to_returns) = (timing.tracks[SENDER].latency, timing.tracks[SENDER].to_returns);
    assert_eq!((latency, to_returns), (LINEAR, LIMITER), "wanted the sender {LINEAR} late and held {LIMITER} more into the return, got {latency} and {to_returns}");
    lands_with_its_click(&sending(HELD_PATH, vec![step_at_the_beat()]), &sending(CLICK_PATH, Vec::new()), Some(RETURN), LINEAR + LIMITER, "a fader step heard only through the send");
}

#[test]
fn a_return_gain_step_on_a_latent_return_lands_with_the_click_it_carries() {
    let returning = |path: &str, curves: Vec<Curve>| {
        let mut sender = playing(path, Vec::new());
        sender.sends[RETURN] = SENT;
        let mut project = project(vec![sender, silent(vec![linear_crossover()])]);
        project.sends[RETURN].rack = vec![limiter()];
        project.sends[RETURN].automation.curves = curves;
        project
    };
    let latency = Timing::of(&returning(HELD_PATH, Vec::new()), SAMPLE_RATE).returns[RETURN].latency;
    assert_eq!(latency, LINEAR + LIMITER, "wanted the return {LINEAR} late at its input and {LIMITER} more through its limiter, got {latency}");
    lands_with_its_click(&returning(HELD_PATH, vec![step_at_the_beat()]), &returning(CLICK_PATH, Vec::new()), Some(RETURN), LINEAR + LIMITER, "a return gain step after the return's limiter");
}

#[test]
fn a_master_gain_step_lands_with_the_click_through_a_latent_master_row() {
    let mastered = |path: &str, curves: Vec<Curve>| {
        let mut project = project(vec![playing(path, Vec::new())]);
        project.master.rack = vec![linear_crossover()];
        project.master.automation.curves = curves;
        project
    };
    lands_with_its_click(&mastered(HELD_PATH, vec![step_at_the_beat()]), &mastered(CLICK_PATH, Vec::new()), None, LINEAR, "a master gain step after a linear crossover on the master");
}

#[test]
fn with_no_latent_row_a_fader_step_lands_on_its_beat() {
    let mut fading = playing(HELD_PATH, Vec::new());
    fading.automation.curves = vec![step_at_the_beat()];
    lands_with_its_click(&project(vec![fading]), &project(vec![playing(CLICK_PATH, Vec::new())]), None, 0, "a fader step on a dry track");
}
