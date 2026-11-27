use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::fx::params::Lookahead;
use bp::fx::{self, Kind, Slot};
use bp::project::{Clip, Project, Row, TICKS_PER_BEAT, Track};
use bp::wav::Wave;
use std::sync::Arc;
use support::{BLOCK, SAMPLE_RATE, beat_frame, wav};

const PATH: &str = "/x/click.wav";
const CLICK: i16 = 8_192;
const QUARTER: f32 = 0.25;
const CLICK_BEAT: usize = 1;
const FILE_BEATS: i32 = 2;
const FRAMES: usize = 36_000;
const LIMITER: usize = 71;
const COMPRESSOR: usize = 240;
const EXACT: f32 = 1e-6;
const SHOWN: usize = 8;
const SEND_A: usize = 0;
const UNITY: f32 = 1.0;

fn click_wave() -> Arc<Wave> {
    let mut samples = vec![0; FILE_BEATS as usize * beat_frame(1)];
    samples[beat_frame(CLICK_BEAT)] = CLICK;
    Arc::new(Wave::parse(String::from(PATH), String::from(PATH), &wav(&samples)).expect("wanted the click file to read"))
}

fn sending(rack: Vec<Row>) -> Track {
    let mut track = Track { rack, clips: vec![Clip::audio(0, FILE_BEATS * TICKS_PER_BEAT, String::from("click"), String::from(PATH))], ..Track::audio() };
    track.sends[SEND_A] = UNITY;
    track
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

fn held_compressor() -> Row {
    let mut slot = Slot::new(Kind::Compressor);
    if let fx::Params::Compressor(p) = &mut slot.params {
        p.lookahead = Lookahead::Ms5;
        p.ratio = 1.0;
        p.threshold_db = 0.0;
    }
    Row::Effect(slot)
}

fn returning(tracks: Vec<Track>, return_rack: Vec<Row>) -> Project {
    let mut project = Project::new();
    project.tracks.clear();
    for (at, track) in tracks.into_iter().enumerate() {
        project.add_track(at, track);
    }
    project.sends[SEND_A].rack = return_rack;
    project
}

fn render(project: &Project, waves: &[Arc<Wave>]) -> (Vec<f32>, usize) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, None, &[], waves);
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

fn heard(out: &[f32], floor: f32) -> Vec<(usize, f32)> {
    out.iter().enumerate().filter(|(_, sample)| sample.abs() > floor).map(|(frame, sample)| (frame, *sample)).collect()
}

fn one_click(out: &[f32], frame: usize, height: f32, within: f32, what: &str) {
    let heard = heard(out, within);
    let alone = heard.len() == 1 && heard[0].0 == frame && (heard[0].1 - height).abs() <= within;
    assert!(alone, "{what}: wanted one click of {height} at frame {frame} and nothing else above {within}, got {} frames above it, the first {:?}", heard.len(), &heard[..heard.len().min(SHOWN)]);
}

#[test]
fn a_latent_return_lands_with_the_direct_signal() {
    let n = beat_frame(CLICK_BEAT);
    let (out, output) = render(&returning(vec![sending(Vec::new())], vec![limiter()]), &[click_wave()]);
    assert_eq!(output, LIMITER, "wanted the output latency at {LIMITER}, got {output}");
    one_click(&out, n + LIMITER, 2.0 * QUARTER, EXACT, "a dry track sent at unity to a limited return");
}

#[test]
fn two_senders_of_different_latency_meet_in_one_latent_return() {
    let n = beat_frame(CLICK_BEAT);
    let (out, output) = render(&returning(vec![sending(Vec::new()), sending(vec![limiter()])], vec![held_compressor()]), &[click_wave()]);
    let wanted = LIMITER + COMPRESSOR;
    assert_eq!(output, wanted, "wanted the output latency at {wanted}, got {output}");
    one_click(&out, n + wanted, 4.0 * QUARTER, EXACT, "a dry sender and a limited sender into a return holding a look-ahead compressor");
}
