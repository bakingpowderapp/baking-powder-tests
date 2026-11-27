use bp::app::{App, Event, Request};
use bp::json;
use bp::keyboard::Key;
use bp::project::{Project, Track};
use support::{HEIGHT, WIDTH};

const RAIL: i32 = 45;
const TILE: i32 = 120;
const BORDER: i32 = 8;
const PITCH: i32 = TILE - BORDER;
const TITLE_X: i32 = 60;
const TITLE_DROP: i32 = 21;
const DRUMS: &str = "drums";

fn plain(name: &str) -> Track {
    Track { name: String::from(name), ..Track::blank() }
}

fn member(name: &str) -> Track {
    Track { inside: true, ..plain(name) }
}

fn group(name: &str, folded: bool) -> Track {
    Track { name: String::from(name), folded, ..Track::group() }
}

fn open(tracks: Vec<Track>, selected: usize) -> App {
    let mut project = Project::new();
    project.tracks = tracks;
    let session = format!("{{\"project\":{},\"automation\":false,\"keyboard\":false,\"palette\":false,\"selected\":{selected}}}", project.to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.draw(WIDTH, HEIGHT);
    app.take_requests();
    app
}

fn press_title(app: &mut App, slot: usize) {
    let (x, y) = (TITLE_X, RAIL + slot as i32 * PITCH + TITLE_DROP);
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
}

fn type_name(app: &mut App, name: &str) {
    for letter in name.chars() {
        app.event(Event::Key { key: Key::Char(letter), shift: false, command: false, repeat: false });
        app.event(Event::KeyUp { key: Key::Char(letter) });
    }
    app.event(Event::Key { key: Key::Return, shift: false, command: false, repeat: false });
    app.event(Event::KeyUp { key: Key::Return });
}

fn names(app: &mut App) -> Vec<String> {
    let text = app.take_requests().into_iter().rev().find_map(|request| if let Request::Remember { json } = request { Some(json) } else { None }).expect("wanted the session remembered after the name was typed");
    let session = json::parse(&text).expect("wanted the remembered session to parse");
    let project = Project::from_json(session.get("project").expect("wanted the session to hold the project")).expect("wanted the remembered project to read");
    project.tracks.iter().map(|track| track.name.clone()).collect()
}

#[test]
fn a_selected_track_renames_from_a_press_on_its_title() {
    let mut app = open(vec![plain("A"), plain("B")], 1);
    press_title(&mut app, 1);
    type_name(&mut app, DRUMS);
    assert_eq!(names(&mut app), ["A", DRUMS], "wanted a press on the selected second track's title to open the name for typing");
}

#[test]
fn a_track_below_a_folded_group_renames_from_its_own_slot() {
    let tracks = vec![group("PERCUS", true), member("KICK"), member("SNARE"), plain("VOX")];
    let mut app = open(tracks, 3);
    press_title(&mut app, 1);
    type_name(&mut app, DRUMS);
    assert_eq!(names(&mut app), ["PERCUS", "KICK", "SNARE", DRUMS], "wanted VOX, fourth in the project but second in the column under a folded group, renamed from a press on the second slot's title");
}

#[test]
fn a_group_below_a_folded_group_renames_too() {
    let tracks = vec![group("PERCUS", true), member("KICK"), group("GROUP 2", false), member("VOX"), member("AUDIO 6")];
    let mut app = open(tracks, 2);
    press_title(&mut app, 1);
    type_name(&mut app, DRUMS);
    assert_eq!(names(&mut app), ["PERCUS", "KICK", DRUMS, "VOX", "AUDIO 6"], "wanted the second group, third in the project but second in the column, renamed from a press beside its fold mark");
}
