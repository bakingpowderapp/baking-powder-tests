use bp::arp::Arp;
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::face::{Face, Faced};
use bp::fx::{Kind, Params, Slot};
use bp::json;
use bp::project::{DEFAULT_BPM, Project, Row, VELOCITY};
use support::{AUDIBLE, BLOCK, BLOCKS, LEVEL, SAMPLE_RATE, core, first_difference, midi, peak};

const SOURCE: usize = 0;
const DUCKED: usize = 1;
const SPARE: usize = 2;
const LAST: usize = 3;
const TRACKS: usize = 4;
const OFF: f64 = 0.0;
const FIRST_TRACK: f64 = 1.0;
const PAST_THE_TRACKS: f64 = 10.0;
const PAST_I32: f64 = 2_147_483_647.0;
const SIGNAL: (&str, &str) = ("SIGNAL", "SOURCE");
const CELLS: usize = 64;
const NAMES: [&str; 3] = ["off", "Synth 1", "Synth 2"];
const DUCK_FROM_DB: f64 = -40.0;
const DUCK_RATIO: f64 = 20.0;
const NOTES: [(usize, u8); 2] = [(SOURCE, 36), (DUCKED, 60)];

fn ducker(source: f64) -> Slot {
    let mut slot = Slot::new(Kind::Sidechain);
    if let Params::Sidechain(sidechain) = &mut slot.params {
        sidechain.signal.source = source;
        sidechain.threshold_db = DUCK_FROM_DB;
        sidechain.ratio = DUCK_RATIO;
    }
    slot
}

fn saved(source: f64) -> Slot {
    let mut song = Project::new();
    song.tracks[0].rack = vec![midi(Arp::DEFAULT), core(LEVEL), Row::Effect(ducker(source))];
    let text = song.to_json().write();
    let json = json::parse(&text).expect("a saved song is valid JSON");
    let opened = Project::from_json(&json).expect("a saved song opens");
    let Row::Effect(opened) = opened.tracks[0].rack[2] else {
        panic!("the Sidechain row opens as an effect");
    };
    opened
}

fn keyed(source: f64) -> Project {
    let mut project = Project::new();
    for at in project.tracks.len()..TRACKS {
        project.add_midi_track(at);
    }
    project.tracks[SOURCE].rack = vec![midi(Arp::DEFAULT), core(LEVEL)];
    project.tracks[DUCKED].rack = vec![midi(Arp::DEFAULT), core(LEVEL), Row::Effect(ducker(source))];
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

fn run(project: &Project) -> Vec<Vec<f32>> {
    let (_audio, mut renderer) = open(project);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    (0..BLOCKS)
        .map(|_| {
            renderer.render(&mut l, &mut r);
            l.clone()
        })
        .collect()
}

fn heard_as_unkeyed(change: fn(&mut Project), when: &str) {
    let mut wild = keyed(PAST_THE_TRACKS);
    change(&mut wild);
    let mut off = keyed(OFF);
    change(&mut off);
    let heard = run(&wild);
    assert!(peak(&heard) > AUDIBLE, "the ducked track is silent {when}");
    assert_eq!(first_difference(&heard, &run(&off)), None, "a source past the tracks ducked the track it sits on {when}, block");
}

fn source_cell(slot: &Slot) -> usize {
    (0..CELLS).find(|&cell| slot.setting_of(cell).is_some_and(|(band, param, _)| (band, param) == SIGNAL)).expect("wanted the SIGNAL SOURCE cell on the Sidechain's face")
}

fn picker_over(names: &[&str], slot: &Slot) -> Face {
    let mut face = Face::new();
    face.name_list(names.iter().map(|name| String::from(*name)).collect(), None);
    face.pick(source_cell(slot));
    face
}

fn source_of(slot: &Slot) -> f64 {
    slot.setting_of(source_cell(slot)).map(|(_, _, source)| source).expect("wanted the SIGNAL SOURCE cell")
}

#[test]
fn a_source_past_the_tracks_keys_nothing() {
    heard_as_unkeyed(|_| {}, "as set");
}

#[test]
fn a_source_past_the_tracks_survives_a_track_added() {
    heard_as_unkeyed(|project| {
        project.add_midi_track(SPARE);
    }, "after a track was added");
}

#[test]
fn a_source_past_the_tracks_survives_a_track_moved() {
    heard_as_unkeyed(|project| project.move_track(LAST, SPARE), "after a track was moved");
}

#[test]
fn a_source_past_the_tracks_survives_a_track_removed() {
    heard_as_unkeyed(|project| assert!(project.remove_track(LAST), "wanted the last track removed"), "after a track was removed");
}

#[test]
fn a_saved_number_past_the_tracks_reopens_as_off_and_steps_into_its_list() {
    let mut slot = saved(PAST_I32);
    let mut face = picker_over(&NAMES, &slot);
    let reopened = source_of(&slot);
    assert_eq!(reopened, OFF, "wanted a saved number past the tracks to reopen as off, got {reopened}");
    assert!(slot.nudge(&mut face, 1, false, DEFAULT_BPM), "wanted the picker to step on");
    let stepped = source_of(&slot);
    assert_eq!(stepped, FIRST_TRACK, "wanted a step on from off to land on the first track, got {stepped}");
    assert!(slot.nudge(&mut face, -1, false, DEFAULT_BPM), "wanted the picker to step back");
    let back = source_of(&slot);
    assert_eq!(back, OFF, "wanted a step back from the first track to land on off, got {back}");
}
