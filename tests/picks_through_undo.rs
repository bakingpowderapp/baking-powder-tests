use bp::app::{App, Event};
use bp::arp::Articulation;
use bp::json::Json;
use bp::keyboard::Key;
use bp::project::{Clip, Note, Project, VELOCITY};
use support::{HEIGHT, NO_END, WIDTH, call, session};

const BASS: usize = 0;
const LEAD: usize = 1;
const NAMES: [&str; 2] = ["Bass", "Lead"];
const PITCHES: [u8; 2] = [36, 72];

fn song() -> Project {
    let mut project = Project::new();
    project.tracks.truncate(NAMES.len());
    for (track, (name, pitch)) in NAMES.iter().zip(PITCHES).enumerate() {
        project.tracks[track].name = String::from(*name);
        let note = Note { pitch, start: 0.0, length: 1.0, velocity: VELOCITY, articulation: Articulation::PLAIN };
        project.tracks[track].clips = vec![Clip::midi(0, String::from(*name), vec![note], NO_END)];
    }
    project
}

fn key(app: &mut App, key: Key, command: bool) {
    app.event(Event::Key { key, shift: false, command, repeat: false });
}

fn picked_tracks(session: &Json) -> Vec<i64> {
    session.get("picked").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(|pick| pick.as_array().and_then(|pair| pair.first()).and_then(Json::as_i64)).collect()
}

fn clips_on(session: &Json) -> Vec<(String, usize)> {
    let project = Project::from_json(session.get("project").expect("wanted the session to hold the project")).expect("wanted the remembered project to read");
    project.tracks.iter().map(|track| (track.name.clone(), track.clips.len())).collect()
}

#[test]
fn delete_after_a_track_move_is_undone_removes_the_clip_picked_on_that_track() {
    let text = format!("{{\"project\":{},\"keyboard\":false,\"palette\":false,\"picked\":[[{BASS},0]]}}", song().to_json().write());
    let mut app = App::new();
    assert!(app.resume(&text), "wanted the session to resume");
    app.draw(WIDTH, HEIGHT);
    app.take_requests();
    let moved = call(&mut app, "write", &format!(r#"{{"path":"tracks/{BASS}","move":{LEAD}}}"#));
    assert!(!moved.failed, "wanted Bass moved below Lead, got {}", moved.text);
    assert_eq!(picked_tracks(&session(&mut app)), [LEAD as i64], "wanted Bass's picked clip carried below Lead with its track");
    key(&mut app, Key::Char('z'), true);
    assert_eq!(picked_tracks(&session(&mut app)), [BASS as i64], "wanted the undo to carry Bass's picked clip back up with its track");
    key(&mut app, Key::Delete, false);
    let wanted: Vec<(String, usize)> = NAMES.iter().zip([0, 1]).map(|(name, clips)| (String::from(*name), clips)).collect();
    assert_eq!(clips_on(&session(&mut app)), wanted, "wanted Delete after the undo to remove Bass's picked clip and leave Lead's");
}
