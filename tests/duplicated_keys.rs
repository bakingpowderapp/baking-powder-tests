use bp::app::{App, Event};
use bp::arp::Arp;
use bp::fx::{Kind, Params, Slot};
use bp::keyboard::Key;
use bp::project::{Project, Row, Track};
use support::{HEIGHT, LEVEL, WIDTH, core, midi, remembered};

const KICK: usize = 0;
const SNARE: usize = 1;
const BASS: usize = 2;
const NAMES: [&str; 3] = ["kick", "snare", "bass"];

fn number(track: usize) -> f64 {
    (track + 1) as f64
}

fn ducking(source: f64) -> Row {
    let mut slot = Slot::new(Kind::Sidechain);
    if let Params::Sidechain(sidechain) = &mut slot.params {
        sidechain.signal.source = source;
    }
    Row::Effect(slot)
}

fn key_of(project: &Project, track: usize) -> Option<f64> {
    project.tracks[track].rack.iter().find_map(|row| if let Row::Effect(slot) = row && let Params::Sidechain(sidechain) = &slot.params { Some(sidechain.signal.source) } else { None })
}

fn keyed_from(project: &Project, track: usize) -> Option<usize> {
    (key_of(project, track)?.round() as usize).checked_sub(1)
}

fn bare() -> Project {
    let mut project = Project::new();
    project.tracks.clear();
    for (track, name) in NAMES.iter().enumerate() {
        project.add_track(track, Track { rack: vec![midi(Arp::DEFAULT), core(LEVEL)], ..Track::blank() });
        project.tracks[track].name = String::from(*name);
    }
    project
}

fn drums_on_the_bass() -> Project {
    let mut project = bare();
    for drum in [KICK, SNARE] {
        project.tracks[drum].rack.push(ducking(number(BASS)));
        project.tracks[drum].follows = Some(BASS);
    }
    project
}

fn session_with(project: &Project, picked: &[usize]) -> App {
    let picks: Vec<String> = picked.iter().map(usize::to_string).collect();
    let session = format!("{{\"project\":{},\"automation\":false,\"keyboard\":false,\"palette\":false,\"picked_tracks\":[{}]}}", project.to_json().write(), picks.join(","));
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.draw(WIDTH, HEIGHT);
    app.take_requests();
    app
}

fn command(app: &mut App, letter: char) {
    app.event(Event::Key { key: Key::Char(letter), shift: false, command: true, repeat: false });
}

fn names(project: &Project) -> Vec<&str> {
    project.tracks.iter().map(|track| track.name.as_str()).collect()
}

fn sorted_names(project: &Project) -> Vec<&str> {
    let mut sorted = names(project);
    sorted.sort_unstable();
    sorted
}

fn bass_at(project: &Project) -> usize {
    project.tracks.iter().position(|track| track.name == NAMES[BASS]).expect("wanted the bass among the tracks")
}

fn which(project: &Project, track: Option<usize>) -> String {
    track.map_or_else(|| String::from("nothing"), |at| format!("the {} at {at}", project.tracks[at].name))
}

fn astray(project: &Project, bass: usize) -> Vec<String> {
    let mut astray = Vec::new();
    for (at, track) in project.tracks.iter().enumerate() {
        let wanted = (at != bass).then_some(bass);
        let key = keyed_from(project, at);
        if key != wanted {
            astray.push(format!("the {} at {at} keyed from {}", track.name, which(project, key)));
        }
        if track.follows != wanted {
            astray.push(format!("the {} at {at} following {}", track.name, which(project, track.follows)));
        }
    }
    astray
}

#[test]
fn command_d_on_one_track_keeps_its_key_and_follow_on_the_bass() {
    let mut app = session_with(&drums_on_the_bass(), &[KICK]);
    command(&mut app, 'd');
    let project = remembered(&mut app);
    assert_eq!(sorted_names(&project), ["bass", "kick", "kick", "snare"], "wanted Cmd+D to add a second kick, got {:?}", names(&project));
    let bass = bass_at(&project);
    let astray = astray(&project, bass);
    assert!(astray.is_empty(), "wanted the kick, its copy and the snare keyed from and following the bass at {bass} after Cmd+D, got {}", astray.join(", "));
}

#[test]
fn command_d_on_two_tracks_keeps_every_key_and_follow_on_the_bass() {
    let mut app = session_with(&drums_on_the_bass(), &[KICK, SNARE]);
    command(&mut app, 'd');
    let project = remembered(&mut app);
    assert_eq!(sorted_names(&project), ["bass", "kick", "kick", "snare", "snare"], "wanted Cmd+D to add a second kick and a second snare, got {:?}", names(&project));
    let bass = bass_at(&project);
    let astray = astray(&project, bass);
    assert!(astray.is_empty(), "wanted both kicks and both snares, originals and copies, keyed from and following the bass at {bass} after Cmd+D, got {}", astray.join(", "));
}

#[test]
fn command_c_then_v_on_two_tracks_keeps_every_key_and_follow_on_the_bass() {
    let mut app = session_with(&drums_on_the_bass(), &[KICK, SNARE]);
    command(&mut app, 'c');
    command(&mut app, 'v');
    let project = remembered(&mut app);
    assert_eq!(sorted_names(&project), ["bass", "kick", "kick", "snare", "snare"], "wanted Cmd+C then Cmd+V to add a second kick and a second snare, got {:?}", names(&project));
    let bass = bass_at(&project);
    let astray = astray(&project, bass);
    assert!(astray.is_empty(), "wanted both kicks and both snares, originals and copies, keyed from and following the bass at {bass} after Cmd+C then Cmd+V, got {}", astray.join(", "));
}
