use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::fx::params::{Freak, FreakSlope, Lookahead};
use bp::fx::{self, Kind, Slot};
use bp::project::{Clip, Project, Row, TICKS_PER_BEAT, Timing, Track};
use bp::wav::Wave;
use std::f64::consts::TAU;
use std::sync::Arc;
use support::{AUDIBLE, BLOCK, SAMPLE_RATE, beat_frame, peak_of, wav};

const CLICK_PATH: &str = "/x/click.wav";
const TONE_PATH: &str = "/x/tone.wav";
const CLICK: i16 = 8_192;
const TONE_HZ: f64 = 1_000.0;
const CLICK_BEAT: usize = 1;
const FILE_BEATS: i32 = 2;
const FRAMES: usize = 36_000;
const LIMITER: usize = 71;
const COMPRESSOR: usize = 240;
const LINEAR: usize = 431;
const DUCK_FROM_DB: f64 = -60.0;
const DUCK_RATIO: f64 = 20.0;
const FAST_MS: f64 = 0.1;
const DIP: f32 = 1e-3;
const NEAR: usize = 2;
const SETTLE: usize = 1_000;
const SOURCE: usize = 0;
const DUCKED: usize = 1;
const SIBLING: usize = 2;
const FIRST: usize = 0;
const THIRD: usize = 2;
const RETURN: usize = 0;
const SENT: f32 = 1.0;
const ROUNDING: f32 = 1e-5;

fn wave(path: &str, samples: &[i16]) -> Arc<Wave> {
    Arc::new(Wave::parse(String::from(path), String::from(path), &wav(samples)).expect("wanted the test file to read"))
}

fn waves() -> [Arc<Wave>; 2] {
    let frames = FILE_BEATS as usize * beat_frame(1);
    let mut click = vec![0; frames];
    click[beat_frame(CLICK_BEAT)] = CLICK;
    let tone: Vec<i16> = (0..frames).map(|frame| ((TAU * TONE_HZ * frame as f64 / SAMPLE_RATE).cos() * f64::from(CLICK)).round() as i16).collect();
    [wave(CLICK_PATH, &click), wave(TONE_PATH, &tone)]
}

fn playing(path: &str, rack: Vec<Row>) -> Track {
    Track { rack, clips: vec![Clip::audio(0, FILE_BEATS * TICKS_PER_BEAT, String::from("file"), String::from(path))], ..Track::audio() }
}

fn clicking(rack: Vec<Row>) -> Track {
    playing(CLICK_PATH, rack)
}

fn toning(rack: Vec<Row>) -> Track {
    playing(TONE_PATH, rack)
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

fn linear_crossover() -> Row {
    Row::Effect(Slot { params: fx::Params::Freak(Freak { slope: FreakSlope::Linear, oversample: false, ..Freak::DEFAULT }), ..Slot::new(Kind::Freak) })
}

fn ducker(source: usize, pre: bool, lookahead: Lookahead) -> Row {
    let mut slot = Slot::new(Kind::Sidechain);
    if let fx::Params::Sidechain(p) = &mut slot.params {
        p.threshold_db = DUCK_FROM_DB;
        p.ratio = DUCK_RATIO;
        p.attack_ms = FAST_MS;
        p.lookahead = lookahead;
    }
    slot.wire(Some(source), pre);
    Row::Effect(slot)
}

fn unwire(rack: &mut [Row]) {
    for row in rack {
        if let Row::Effect(slot) = row
            && slot.kind() == Kind::Sidechain
        {
            slot.wire(None, false);
        }
    }
}

fn unkeyed(project: &Project) -> Project {
    let mut plain = project.clone();
    for track in &mut plain.tracks {
        unwire(&mut track.rack);
    }
    for ret in plain.returns_mut() {
        unwire(&mut ret.rack);
    }
    plain
}

fn keyed_only(project: &Project, track: usize, row: usize) -> Project {
    let mut one = project.clone();
    let kept = one.tracks[track].rack[row];
    unwire(&mut one.tracks[track].rack);
    one.tracks[track].rack[row] = kept;
    one
}

fn audio_project(tracks: Vec<Track>) -> Project {
    let mut project = Project::new();
    project.tracks.clear();
    for (at, track) in tracks.into_iter().enumerate() {
        project.add_track(at, track);
    }
    project
}

fn render(project: &Project) -> (Vec<f32>, usize) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, None, &[], &waves());
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

fn duck(project: &Project) -> (Option<usize>, usize) {
    let (ducked, output) = render(project);
    let (plain, plain_output) = render(&unkeyed(project));
    let dip = ducked[output..].iter().zip(&plain[plain_output..]).position(|(a, b)| (a - b).abs() > DIP);
    (dip.map(|frame| frame + output), output)
}

fn ducks_at(project: &Project, late: usize, frame: usize, what: &str) {
    let (dip, output) = duck(project);
    assert_eq!(output, late, "{what}: wanted the output latency at {late}, got {output}");
    assert!(dip.is_some_and(|dip| dip.abs_diff(frame) <= NEAR), "{what}: wanted the duck to begin at frame {frame}, got {dip:?}");
}

#[test]
fn a_key_from_a_latent_source_ducks_where_its_click_is_heard() {
    let n = beat_frame(CLICK_BEAT);
    let post = audio_project(vec![clicking(vec![linear_crossover()]), toning(vec![ducker(SOURCE, false, Lookahead::Off)])]);
    ducks_at(&post, LINEAR, n + LINEAR, "post tap");
    let pre = audio_project(vec![clicking(vec![linear_crossover()]), toning(vec![ducker(SOURCE, true, Lookahead::Off)])]);
    ducks_at(&pre, LINEAR, n + LINEAR, "pre tap");
}

#[test]
fn a_key_later_than_its_audio_grows_the_tracks_latency_and_still_lands() {
    let n = beat_frame(CLICK_BEAT);
    let project = audio_project(vec![clicking(vec![linear_crossover()]), toning(vec![limiter(), ducker(SOURCE, false, Lookahead::Off)])]);
    let timing = Timing::of(&project, SAMPLE_RATE);
    let (latency, before) = (timing.tracks[DUCKED].latency, timing.tracks[DUCKED].rack.rows[1].before);
    assert_eq!((latency, before), (LINEAR, LINEAR - LIMITER), "wanted the ducked track grown from {LIMITER} to {LINEAR} by a line of {} before its Sidechain, got {latency} and {before}", LINEAR - LIMITER);
    ducks_at(&project, LINEAR, n + LINEAR, "a key later than the audio it ducks");
}

#[test]
fn two_ducks_in_one_rack_both_align_with_their_key() {
    let n = beat_frame(CLICK_BEAT);
    let both = audio_project(vec![clicking(vec![linear_crossover()]), toning(vec![ducker(SOURCE, false, Lookahead::Off), limiter(), ducker(SOURCE, false, Lookahead::Off)])]);
    ducks_at(&keyed_only(&both, DUCKED, FIRST), LINEAR + LIMITER, n + LINEAR + LIMITER, "the Sidechain before the limiter");
    ducks_at(&keyed_only(&both, DUCKED, THIRD), LINEAR, n + LINEAR, "the Sidechain after the limiter");
    ducks_at(&both, LINEAR + LIMITER, n + LINEAR + LIMITER, "both Sidechains");
}

#[test]
fn a_duck_on_the_master_lands_on_the_key_as_heard() {
    let n = beat_frame(CLICK_BEAT);
    let mut project = audio_project(vec![clicking(Vec::new()), toning(Vec::new()), Track { rack: vec![linear_crossover()], ..Track::audio() }]);
    project.master.rack = vec![ducker(SOURCE, false, Lookahead::Off)];
    let to_master = Timing::of(&project, SAMPLE_RATE).tracks[SOURCE].to_master;
    assert_eq!(to_master, LINEAR, "wanted the dry source held {LINEAR} to the master beside track {SIBLING}, got {to_master}");
    ducks_at(&project, LINEAR, n + LINEAR, "a Sidechain on the master keyed from a dry track");
}

#[test]
fn a_duck_on_a_return_lands_on_the_key_as_heard() {
    let n = beat_frame(CLICK_BEAT);
    let mut sender = toning(Vec::new());
    sender.sends[RETURN] = SENT;
    let mut project = audio_project(vec![clicking(Vec::new()), sender, Track { rack: vec![linear_crossover()], ..Track::audio() }]);
    project.sends[RETURN].rack = vec![ducker(SOURCE, false, Lookahead::Off)];
    let timing = Timing::of(&project, SAMPLE_RATE);
    let (to_returns, key) = (timing.tracks[DUCKED].to_returns, timing.returns[RETURN].rows[FIRST].signal);
    assert_eq!((to_returns, key), (LINEAR, LINEAR), "wanted the dry sender held {LINEAR} into the return beside track {SIBLING} and the return's Sidechain reading its dry key {LINEAR} back, got {to_returns} and {key}");
    ducks_at(&project, LINEAR, n + LINEAR, "a Sidechain on a return keyed from a dry track");
}

#[test]
fn a_look_ahead_duck_lands_its_look_ahead_before_the_key() {
    let n = beat_frame(CLICK_BEAT);
    let project = audio_project(vec![clicking(Vec::new()), toning(vec![ducker(SOURCE, false, Lookahead::Ms5)])]);
    ducks_at(&project, COMPRESSOR, n + COMPRESSOR - COMPRESSOR, "a Sidechain with 5 ms of look-ahead");
}

#[test]
fn a_keyed_cycle_keeps_both_tracks_playing_as_with_neither_muted() {
    let project = audio_project(vec![toning(vec![ducker(DUCKED, false, Lookahead::Off)]), toning(vec![ducker(SOURCE, false, Lookahead::Off)])]);
    let (out, _) = render(&project);
    assert!(peak_of(&out[SETTLE..]) > AUDIBLE, "wanted the keyed cycle heard, got a peak of {}", peak_of(&out[SETTLE..]));
    let alone = |track: usize| {
        let mut alone = project.clone();
        alone.tracks[DUCKED - track].mute = true;
        render(&alone).0
    };
    let (first, second) = (alone(SOURCE), alone(DUCKED));
    for (heard, name) in [(&first, "the first"), (&second, "the second")] {
        assert!(peak_of(&heard[SETTLE..]) > 0.0, "wanted {name} track of a keyed cycle playing with the other muted, got silence");
    }
    let worst = out.iter().zip(first.iter().zip(&second)).map(|(both, (one, other))| (both - (one + other)).abs()).fold(0.0f32, f32::max);
    assert!(worst < ROUNDING, "wanted each track of a keyed cycle to play with the other muted as it does with neither muted, a muted track still keying the other, got a worst delta of {worst} from the cycle as heard");
}
