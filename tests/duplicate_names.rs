use bp::app::{App, Event, Request};
use bp::json;
use bp::keyboard::Key;
use bp::project::{Project, Track};

const VOX: &str = "vox";
const LEAD: &str = "lead";
const AUDIO_TRACK: usize = 1;

fn open(project: &Project, selected: usize) -> App {
    let session = format!("{{\"project\":{},\"selected\":{selected},\"palette\":false,\"keyboard\":false}}", project.to_json().write());
    let mut app = App::new();
    app.resume(&session);
    app.take_requests();
    app
}

fn duplicate(app: &mut App) -> Project {
    app.event(Event::Key { key: Key::Char('d'), shift: false, command: true, repeat: false });
    let text = app.take_requests().into_iter().rev().find_map(|request| if let Request::Remember { json } = request { Some(json) } else { None }).expect("wanted the session remembered after Cmd+D");
    let session = json::parse(&text).expect("wanted the remembered session to parse");
    Project::from_json(session.get("project").expect("wanted the remembered session to hold the project")).expect("wanted the remembered project to read")
}

fn names(project: &Project) -> Vec<&str> {
    project.tracks.iter().map(|track| track.name.as_str()).collect()
}

#[test]
fn a_duplicated_audio_track_keeps_its_name() {
    let mut project = Project::new();
    project.add_track(AUDIO_TRACK, Track { name: String::from(VOX), ..Track::audio() });
    let mut app = open(&project, AUDIO_TRACK);
    let after = duplicate(&mut app);
    assert_eq!(after.tracks.len(), project.tracks.len() + 1, "wanted one track more after Cmd+D, got {:?}", names(&after));
    let copy = &after.tracks[AUDIO_TRACK + 1];
    assert!(copy.audio, "wanted the copy to be an audio track like the original");
    assert_eq!(copy.name, VOX, "wanted the copy named {VOX} like the original, got {}", copy.name);
}

#[test]
fn a_duplicated_midi_track_keeps_its_name_too_and_fresh_tracks_still_take_a_number() {
    let mut project = Project::new();
    project.tracks[0].name = String::from(LEAD);
    let mut app = open(&project, 0);
    let after = duplicate(&mut app);
    assert_eq!(after.tracks[1].name, LEAD, "wanted the copy named {LEAD} like the original, got {}", after.tracks[1].name);
    let mut fresh = Project::new();
    fresh.add_track(2, Track::audio());
    assert_eq!(fresh.tracks[2].name, "AUDIO 3", "wanted a fresh audio track numbered after the two MIDI tracks, got {}", fresh.tracks[2].name);
}
