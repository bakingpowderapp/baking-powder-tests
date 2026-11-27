use bp::app::{App, Event};
use bp::arp::Arp;
use bp::keyboard::Key;
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::fx::{Kind, Params, Slot};
use bp::project::{Project, Row, Track, VELOCITY};
use support::{AUDIBLE, BLOCK, BLOCKS, HEIGHT, LEVEL, SAMPLE_RATE, SILENT, WIDTH, core, core_layer, peak, remembered, struck};

const SETTLE: usize = 24;
const ROUNDING: f32 = 1e-5;
const DUCK_FROM_DB: f64 = -40.0;
const DUCK_RATIO: f64 = 20.0;
const DUCKED_BY: f32 = 4.0;
const GROUP: usize = 0;
const KICK: usize = 1;
const SNARE: usize = 2;
const BASS: usize = 3;
const OFF: f64 = 0.0;
const NAMES: [&str; 3] = ["kick", "snare", "bass"];
const MIDI: [u8; 3] = [36, 40, 48];

fn number(track: usize) -> f64 {
    (track + 1) as f64
}

fn ducking(source: f64, bypass: bool) -> Row {
    let mut slot = Slot::new(Kind::Sidechain);
    slot.bypass = bypass;
    if let Params::Sidechain(sidechain) = &mut slot.params {
        sidechain.signal.source = source;
        sidechain.threshold_db = DUCK_FROM_DB;
        sidechain.ratio = DUCK_RATIO;
    }
    Row::Effect(slot)
}

fn key_of(project: &Project, track: usize) -> Option<f64> {
    project.tracks[track].rack.iter().find_map(|row| if let Row::Effect(slot) = row && let Params::Sidechain(sidechain) = &slot.params { Some(sidechain.signal.source) } else { None })
}

fn keyed_from(project: &Project, track: usize) -> Option<String> {
    let source = (key_of(project, track)?.round() as usize).checked_sub(1)?;
    Some(project.tracks[source].name.clone())
}

fn set_key(project: &mut Project, track: usize, source: f64) {
    for row in &mut project.tracks[track].rack {
        if let Row::Effect(slot) = row
            && let Params::Sidechain(sidechain) = &mut slot.params
        {
            sidechain.signal.source = source;
        }
    }
}

fn bare() -> Project {
    let mut project = Project::new();
    project.tracks.clear();
    for (track, name) in NAMES.iter().enumerate() {
        project.add_track(track, Track { rack: vec![struck(Arp::DEFAULT), core(LEVEL)], ..Track::blank() });
        project.tracks[track].name = String::from(*name);
    }
    project
}

fn bass_keyed_from_snare() -> Project {
    let mut project = bare();
    project.tracks[SNARE - 1].rack.push(ducking(number(BASS - 1), true));
    project.tracks[BASS - 1].rack.push(ducking(number(SNARE - 1), false));
    project
}

fn grouped(project: &Project) -> Project {
    let mut grouped = project.clone();
    grouped.add_track(GROUP, Track::group());
    grouped.tracks[KICK].inside = true;
    grouped.tracks[SNARE].inside = true;
    grouped
}

fn silenced(project: &Project, tracks: &[usize]) -> Project {
    let mut quiet = project.clone();
    for &track in tracks {
        quiet.tracks[track].rack = vec![struck(Arp::DEFAULT), Row::Core(core_layer(0.0, bp::core::Params::DEFAULT))];
    }
    quiet
}

fn run(project: &Project) -> Vec<Vec<f32>> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, None, &[], &[]);
    let sounding = project.tracks.iter().enumerate().filter(|(_, track)| !track.group).map(|(track, _)| track);
    for (track, midi) in sounding.zip(MIDI) {
        audio.note_on(track, midi, VELOCITY);
    }
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    (0..BLOCKS)
        .map(|_| {
            renderer.render(&mut l, &mut r);
            l.clone()
        })
        .collect()
}

fn difference(a: &[Vec<f32>], b: &[Vec<f32>]) -> Vec<Vec<f32>> {
    a.iter().zip(b).map(|(x, y)| x.iter().zip(y).map(|(p, q)| p - q).collect()).collect()
}

fn first_difference(a: &[Vec<f32>], b: &[Vec<f32>]) -> Option<usize> {
    a.iter().zip(b).position(|(x, y)| x.iter().zip(y).any(|(p, q)| (p - q).abs() > ROUNDING))
}

fn heard_alone(project: &Project, track: usize, others: &[usize]) -> f32 {
    let mut quiet = project.clone();
    for &other in others {
        quiet.tracks[other].mute = true;
    }
    peak(&difference(&run(&quiet), &run(&silenced(&quiet, &[track])))[SETTLE..])
}

#[test]
fn a_member_keying_a_track_outside_is_heard_in_its_group() {
    let keyed = grouped(&bass_keyed_from_snare());
    let heard = run(&keyed);
    assert_eq!(first_difference(&heard, &run(&bass_keyed_from_snare())), None, "grouped, the drums keying the bass sound different from ungrouped, block");
    let snare = heard_alone(&keyed, SNARE, &[BASS]);
    assert!(snare > AUDIBLE, "the snare keying the bass peaks at {snare} inside its group");
    let mut unkeyed = keyed.clone();
    set_key(&mut unkeyed, BASS, OFF);
    let ducked = peak(&difference(&heard, &run(&silenced(&keyed, &[BASS])))[SETTLE..]);
    let free = peak(&difference(&run(&unkeyed), &run(&silenced(&unkeyed, &[BASS])))[SETTLE..]);
    assert!(ducked * DUCKED_BY < free, "keyed from a snare inside a group the bass peaks at {ducked} against {free} unkeyed");
}

#[test]
fn a_key_loop_through_a_group_silences_no_member() {
    let mut looped = bass_keyed_from_snare();
    for row in &mut looped.tracks[SNARE - 1].rack {
        if let Row::Effect(slot) = row {
            slot.bypass = false;
        }
    }
    let heard = run(&grouped(&looped));
    assert_eq!(first_difference(&heard, &run(&looped)), None, "grouped, a key loop sounds different from ungrouped, block");
    for member in [KICK, SNARE] {
        let alone = heard_alone(&grouped(&looped), member, &[BASS]);
        assert!(alone > AUDIBLE, "{} peaks at {alone} inside its group under a key loop", NAMES[member - 1]);
    }
    let mut snare_unkeyed = grouped(&looped);
    set_key(&mut snare_unkeyed, SNARE, OFF);
    assert_eq!(first_difference(&heard, &run(&snare_unkeyed)), None, "the upper track of a key loop kept its key, block");
    let mut bass_unkeyed = grouped(&looped);
    set_key(&mut bass_unkeyed, BASS, OFF);
    assert!(first_difference(&heard, &run(&bass_unkeyed)).is_some(), "the lower track of a key loop lost its key");
}

#[test]
fn a_key_from_a_member_taps_the_member_before_the_group() {
    let mut keyed = bare();
    keyed.tracks[BASS - 1].rack.push(ducking(number(KICK - 1), false));
    let full = grouped(&keyed);
    let mut faded = full.clone();
    faded.tracks[GROUP].gain = 0.0;
    let heard = run(&faded);
    assert!(peak(&heard[SETTLE..]) > AUDIBLE, "the bass is silent beside a group faded to nothing");
    let bass_in_full = difference(&run(&full), &run(&silenced(&full, &[BASS])));
    assert_eq!(first_difference(&heard, &bass_in_full), None, "the bass keyed from a member differs between the group at full and faded to nothing, block");
    let mut unkeyed = faded.clone();
    set_key(&mut unkeyed, BASS, OFF);
    let (ducked, free) = (peak(&heard[SETTLE..]), peak(&run(&unkeyed)[SETTLE..]));
    assert!(ducked * DUCKED_BY < free, "keyed from a kick nobody hears the bass peaks at {ducked} against {free} unkeyed");
    let mut muted = full.clone();
    muted.tracks[GROUP].mute = true;
    assert_eq!(first_difference(&run(&muted), &bass_in_full), None, "the bass keyed from a member of a muted group differs from the bass with nothing muted, block");
}

fn gating(source: usize) -> Row {
    let mut slot = Slot::new(Kind::Gate);
    slot.wire(Some(source), false);
    Row::Effect(slot)
}

#[test]
fn a_gate_keyed_from_a_group_opens_the_same_with_the_gated_track_soloed_or_the_group_muted_and_a_muted_member_keys_nothing() {
    let mut full = grouped(&bare());
    full.tracks[BASS].rack.push(gating(GROUP));
    let bass = difference(&run(&full), &run(&silenced(&full, &[BASS])));
    assert!(peak(&bass[SETTLE..]) > AUDIBLE, "the bass gated by the drums' group peaks at {} with nothing muted or soloed", peak(&bass[SETTLE..]));
    let shut = run(&silenced(&full, &[KICK, SNARE]));
    assert!(peak(&shut) < SILENT, "the bass sounds at {} through a gate keyed from a silent group", peak(&shut));
    let mut soloed = full.clone();
    soloed.tracks[BASS].solo = true;
    assert_eq!(first_difference(&run(&soloed), &bass), None, "the bass soloed differs from the bass with nothing soloed, so the solo shut the group's key, block");
    let mut muted = full.clone();
    muted.tracks[GROUP].mute = true;
    assert_eq!(first_difference(&run(&muted), &bass), None, "the bass beside its muted group differs from the bass with nothing muted, so the mute shut the group's key, block");
    let kick_alone = silenced(&full, &[SNARE]);
    let opened = difference(&run(&kick_alone), &run(&silenced(&full, &[SNARE, BASS])));
    assert!(peak(&opened[SETTLE..]) > AUDIBLE, "the bass gated by a group whose one sounding member is the kick peaks at {}", peak(&opened[SETTLE..]));
    let mut kick_muted = kick_alone;
    kick_muted.tracks[KICK].mute = true;
    let heard = run(&kick_muted);
    assert!(peak(&heard) < SILENT, "the bass sounds at {} through a gate keyed from a group whose one sounding member is muted, so the muted kick was heard in its group's key", peak(&heard));
}

#[test]
fn a_duck_keyed_from_a_group_hears_every_member_while_one_of_them_is_soloed() {
    let mut full = grouped(&bare());
    full.tracks[BASS].rack.push(ducking(number(GROUP), false));
    let bass = |project: &Project| difference(&run(project), &run(&silenced(project, &[BASS])));
    let ducked = bass(&full);
    let mut unkeyed = full.clone();
    set_key(&mut unkeyed, BASS, OFF);
    let (heard, free) = (peak(&ducked[SETTLE..]), peak(&bass(&unkeyed)[SETTLE..]));
    assert!(heard > AUDIBLE && heard * DUCKED_BY < free, "keyed from the drums' group the bass peaks at {heard} against {free} unkeyed");
    let mut soloed = full.clone();
    soloed.tracks[SNARE].solo = true;
    soloed.tracks[BASS].solo = true;
    assert_eq!(first_difference(&bass(&soloed), &ducked), None, "with the snare and the bass soloed the bass differs from the bass with nothing soloed, so the solo left the kick out of the group's signal, block");
    let mut kick_soloed = full.clone();
    kick_soloed.tracks[KICK].solo = true;
    kick_soloed.tracks[BASS].solo = true;
    assert_eq!(first_difference(&bass(&kick_soloed), &ducked), None, "with the kick and the bass soloed the bass differs from the bass with nothing soloed, so the solo left the snare out of the group's signal, block");
}

#[test]
fn grouping_moving_and_ungrouping_keep_every_key_on_its_track() {
    let mut project = grouped(&bass_keyed_from_snare());
    let keys = |project: &Project, kick: usize, snare: usize, bass: usize| (keyed_from(project, kick), keyed_from(project, snare), keyed_from(project, bass));
    let named = |kick: Option<&str>, snare: Option<&str>, bass: Option<&str>| (kick.map(String::from), snare.map(String::from), bass.map(String::from));
    assert_eq!(keys(&project, KICK, SNARE, BASS), named(None, Some("bass"), Some("snare")), "grouping moved a key off its track");
    project.tracks[KICK].rack.push(ducking(number(GROUP), false));
    assert_eq!(keyed_from(&project, KICK).as_deref(), Some("GROUP 6"), "the kick's key does not name the group");
    project.move_track(BASS, 0);
    let (bass, group, kick, snare) = (0, 1, 2, 3);
    assert_eq!(keys(&project, kick, snare, bass), named(Some("GROUP 6"), Some("bass"), Some("snare")), "moving the bass above the group moved a key off its track");
    assert!(project.remove_track(group), "the group would not go");
    let (kick, snare) = (1, 2);
    assert_eq!(keys(&project, kick, snare, bass), named(None, Some("bass"), Some("snare")), "ungrouping moved a key off its track");
    assert_eq!(key_of(&project, kick), Some(OFF), "a key from the removed group did not go off, got {:?}", key_of(&project, kick));
}

fn session_with(project: &Project, picked: &[usize]) -> App {
    let picks: Vec<String> = picked.iter().map(usize::to_string).collect();
    let session = format!("{{\"project\":{},\"automation\":false,\"keyboard\":false,\"palette\":false,\"picked_tracks\":[{}]}}", project.to_json().write(), picks.join(","));
    let mut app = App::new();
    app.resume(&session);
    app.draw(WIDTH, HEIGHT);
    app.take_requests();
    app
}

fn command_g(app: &mut App) {
    app.event(Event::Key { key: Key::Char('g'), shift: false, command: true, repeat: false });
}

#[test]
fn command_g_keeps_every_key_on_its_track() {
    let mut app = session_with(&bass_keyed_from_snare(), &[SNARE - 1]);
    command_g(&mut app);
    let grouped = remembered(&mut app);
    let shape: Vec<(bool, bool)> = grouped.tracks.iter().map(|track| (track.group, track.inside)).collect();
    assert_eq!(shape, vec![(true, false), (false, true), (false, true), (false, false)], "wanted the kick and snare under a group with the bass outside, got {shape:?}");
    assert_eq!((keyed_from(&grouped, SNARE).as_deref(), keyed_from(&grouped, BASS).as_deref()), (Some("bass"), Some("snare")), "command G moved a key off its track");
    command_g(&mut app);
    let ungrouped = remembered(&mut app);
    assert_eq!(ungrouped.tracks.len(), 3, "wanted the group gone, got {} tracks", ungrouped.tracks.len());
    assert_eq!((keyed_from(&ungrouped, SNARE - 1).as_deref(), keyed_from(&ungrouped, BASS - 1).as_deref()), (Some("bass"), Some("snare")), "ungrouping with command G moved a key off its track");
}
