use bp::json::{self, Json};
use bp::project::{Project, TRACKS, Track};

const OVER: usize = 3;

fn full() -> Project {
    let mut project = Project::new();
    while project.tracks.len() < TRACKS {
        assert!(project.add_midi_track(project.tracks.len()), "wanted room for track {}", project.tracks.len() + 1);
    }
    project
}

fn with_tracks(count: usize) -> Json {
    let mut project = Project::new();
    project.tracks.truncate(1);
    let mut json = project.to_json();
    if let Json::Object(fields) = &mut json
        && let Some((_, Json::Array(tracks))) = fields.iter_mut().find(|(key, _)| key == "tracks")
    {
        let one = tracks[0].clone();
        tracks.resize(count, one);
    }
    json
}

#[test]
fn the_project_refuses_a_track_past_the_cap() {
    let mut project = full();
    let names: Vec<String> = project.tracks.iter().map(|track| track.name.clone()).collect();
    assert!(!project.add_midi_track(0), "wanted track {} refused at the top", TRACKS + 1);
    assert!(!project.add_track(TRACKS, Track::audio()), "wanted track {} refused at the bottom", TRACKS + 1);
    assert_eq!(project.tracks.len(), TRACKS, "wanted the cap kept");
    let kept: Vec<String> = project.tracks.iter().map(|track| track.name.clone()).collect();
    assert_eq!(kept, names, "wanted the refused tracks to leave the others as they were");
}

#[test]
fn a_saved_project_opens_with_at_most_the_cap() {
    let text = with_tracks(TRACKS + OVER).write();
    let json = json::parse(&text).expect("a saved project is valid JSON");
    let opened = Project::from_json(&json).expect("a saved project opens");
    assert_eq!(opened.tracks.len(), TRACKS, "wanted {} saved tracks to open as {TRACKS}", TRACKS + OVER);
}
