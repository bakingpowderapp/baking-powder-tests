use bp::arp::Arp;
use bp::audio::{Audio, Renderer};
use bp::audio::vision::Seer;
use bp::fx::{Kind, Params, Slot};
use bp::project::{Host, Project, Row, VELOCITY};
use support::{AUDIBLE, BLOCK, BLOCKS, CORE_ROW, LEVEL, SAMPLE_RATE, core, peak, struck};

const SETTLE: usize = 24;
const ROUNDING: f32 = 1e-5;
const SOURCE: usize = 0;
const DUCKED: usize = 1;
const SOURCE_NUMBER: f64 = 1.0;
const OFF: f64 = 0.0;
const SIDECHAIN_ROW: usize = CORE_ROW + 1;
const DUCK_FROM_DB: f64 = -40.0;
const DUCK_RATIO: f64 = 20.0;
const DUCKED_BY: f32 = 4.0;
const NOTES: [(usize, u8); 2] = [(SOURCE, 36), (DUCKED, 60)];
const TICK_SECONDS: f32 = 0.05;

fn ducking(source: f64) -> Row {
    let mut slot = Slot::new(Kind::Sidechain);
    if let Params::Sidechain(sidechain) = &mut slot.params {
        sidechain.signal.source = source;
        sidechain.threshold_db = DUCK_FROM_DB;
        sidechain.ratio = DUCK_RATIO;
    }
    Row::Effect(slot)
}

fn keyed(source: f64) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(2);
    project.tracks[SOURCE].rack = vec![struck(Arp::DEFAULT), core(LEVEL)];
    project.tracks[DUCKED].rack = vec![struck(Arp::DEFAULT), core(LEVEL), ducking(source)];
    project
}

fn open(project: &Project) -> (Audio, Renderer) {
    let (mut audio, renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, None, &[], &[]);
    for (track, midi) in NOTES {
        audio.note_on(track, midi, VELOCITY);
    }
    audio.play(0.0);
    (audio, renderer)
}

fn render(renderer: &mut Renderer, blocks: usize) -> Vec<Vec<f32>> {
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    (0..blocks)
        .map(|_| {
            renderer.render(&mut l, &mut r);
            l.clone()
        })
        .collect()
}

fn run(project: &Project) -> Vec<Vec<f32>> {
    let (_audio, mut renderer) = open(project);
    render(&mut renderer, BLOCKS)
}

fn difference(a: &[Vec<f32>], b: &[Vec<f32>]) -> Vec<Vec<f32>> {
    a.iter().zip(b).map(|(x, y)| x.iter().zip(y).map(|(p, q)| p - q).collect()).collect()
}

fn first_difference(a: &[Vec<f32>], b: &[Vec<f32>]) -> Option<usize> {
    a.iter().zip(b).position(|(x, y)| x.iter().zip(y).any(|(p, q)| (p - q).abs() > ROUNDING))
}

fn source_alone() -> Vec<Vec<f32>> {
    let mut project = keyed(SOURCE_NUMBER);
    project.tracks[DUCKED].mute = true;
    run(&project)
}

#[test]
fn the_key_ducks_the_track_it_feeds() {
    let alone = source_alone();
    let ducked = difference(&run(&keyed(SOURCE_NUMBER)), &alone);
    let unkeyed = difference(&run(&keyed(OFF)), &alone);
    let (ducked, unkeyed) = (peak(&ducked[SETTLE..]), peak(&unkeyed[SETTLE..]));
    assert!(unkeyed > AUDIBLE, "the ducked track is silent on its own");
    assert!(ducked * DUCKED_BY < unkeyed, "keyed from {DUCK_FROM_DB} dB at {DUCK_RATIO}:1 the track peaks at {ducked} against {unkeyed} unkeyed");
}

#[test]
fn soloing_the_ducked_track_keeps_the_pump() {
    let mut soloed = keyed(SOURCE_NUMBER);
    soloed.tracks[DUCKED].solo = true;
    let heard = run(&soloed);
    let ducked = difference(&run(&keyed(SOURCE_NUMBER)), &source_alone());
    assert!(peak(&heard[SETTLE..]) > AUDIBLE, "the soloed track is silent");
    assert_eq!(first_difference(&heard, &ducked), None, "the ducked track soloed is not exactly the ducked track alone, so the solo silenced its key, block");
}

#[test]
fn a_muted_source_keys_as_with_nothing_muted() {
    let mut muted = keyed(SOURCE_NUMBER);
    muted.tracks[SOURCE].mute = true;
    let heard = run(&muted);
    let ducked = difference(&run(&keyed(SOURCE_NUMBER)), &source_alone());
    assert!(peak(&heard[SETTLE..]) > AUDIBLE, "the ducked track is silent beside its muted source");
    assert_eq!(first_difference(&heard, &ducked), None, "the track keyed from a muted source is not exactly the ducked track as heard with nothing muted, so the mute changed its key, block");
}

#[test]
fn soloing_the_source_still_keys_the_track_it_silences() {
    let mut soloed = keyed(SOURCE_NUMBER);
    soloed.tracks[SOURCE].solo = true;
    let (mut audio, mut renderer) = open(&soloed);
    let heard = render(&mut renderer, BLOCKS);
    assert_eq!(first_difference(&heard, &source_alone()), None, "the source soloed is not exactly the source alone, block");
    audio.tick(TICK_SECONDS);
    let reduction = audio.gauge(Host::Track(DUCKED), SIDECHAIN_ROW);
    assert!(reduction > 0.0, "with its source soloed the unheard track's Sidechain reports {reduction} dB of reduction");
    let (left, right) = audio.level(DUCKED);
    assert_eq!((left, right), (0.0, 0.0), "the unheard track's meter moves while it is only keyed");
}
