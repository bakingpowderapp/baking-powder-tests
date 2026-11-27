use bp::app::{App, Event, Opened, Request};
use bp::browser::Kind;
use bp::json::{self, Json};
use bp::keyboard::Key;
use bp::project::Project;
use bp::face;
use support::{call, face_rect, painted, wav};

const RAIL: i32 = 45;
const PITCH: i32 = 112;
const PLUS: (i32, i32) = (100, 100);
const FIRST_ITEM: (i32, i32) = (132, 104);
const TITLE: (i32, i32) = (60, 14);
const BODY: (i32, i32) = (60, 50);
const LANE: (i32, i32) = (400, 60);
const DRAGGED_PX: i32 = 20;
const NEXT: &str = "/vanished/next.json";
const KICK: &str = "/vanished/kick.wav";
const KICK_FRAMES: usize = 4_800;
const KICK_PEAK: i16 = 8_192;
const TYPED: char = 'x';

fn at(slot: usize, (x, y): (i32, i32)) -> (i32, i32) {
    (x, RAIL + slot as i32 * PITCH + y)
}

fn press(app: &mut App, (x, y): (i32, i32)) {
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
}

fn key(app: &mut App, key: Key, command: bool) {
    app.event(Event::Key { key, shift: false, command, repeat: false });
}

fn names(app: &mut App) -> Vec<String> {
    let reply = call(app, "read", r#"{"path":"tracks"}"#);
    let tracks = json::parse(&reply.text).unwrap_or_else(|| panic!("wanted the tracks read back as a list, got {}", reply.text));
    tracks.as_array().unwrap_or(&[]).iter().map(|track| String::from(track.get("name").and_then(Json::as_str).unwrap_or_default())).collect()
}

fn three_tracks() -> App {
    let mut app = App::new();
    painted(&mut app);
    press(&mut app, at(1, PLUS));
    painted(&mut app);
    press(&mut app, at(1, FIRST_ITEM));
    let tracks = names(&mut app).len();
    assert_eq!(tracks, 3, "wanted a third track added from the second track's add menu, got {tracks} tracks");
    app
}

fn first_track_removed(app: &mut App) {
    let reply = call(app, "write", r#"{"path":"tracks/0","value":null}"#);
    assert!(!reply.failed, "wanted an agent's removal of the first track taken, got {}", reply.text);
}

fn two_tracks_written(app: &mut App) {
    let tracks = Project::new().to_json().get("tracks").expect("wanted a new project to hold tracks").write();
    let reply = call(app, "write", &format!(r#"{{"path":"tracks","value":{tracks}}}"#));
    assert!(!reply.failed, "wanted an agent's write of two tracks taken, got {}", reply.text);
}

fn settled(app: &mut App) -> Vec<bp::draw::Fill> {
    app.event(Event::Leave);
    painted(app)
}

fn hover_the_face(app: &mut App) {
    let shown = face_rect(&painted(app));
    app.event(Event::Move { x: shown.x + face::CELL / 2, y: shown.y + face::READOUT_ROWS * face::PITCH + face::CELL / 2 });
}

fn renaming(app: &mut App, slot: usize) {
    press(app, at(slot, BODY));
    press(app, at(slot, TITLE));
    key(app, Key::Char(TYPED), false);
}

#[test]
fn an_undo_that_removes_a_track_closes_the_add_menu_open_on_it() {
    let mut app = three_tracks();
    press(&mut app, at(2, PLUS));
    painted(&mut app);
    key(&mut app, Key::Char('z'), true);
    let after = settled(&mut app);
    let mut plain = three_tracks();
    key(&mut plain, Key::Char('z'), true);
    assert_eq!(names(&mut app).len(), 2, "wanted the undo to take the third track back");
    assert!(after == settled(&mut plain), "wanted the screen an undo leaves with no menu open, the menu's track being gone, but it differs");
}

#[test]
fn a_song_opened_closes_the_add_menu_open_on_a_track_it_lacks() {
    let song = Project::new().to_json().write();
    let mut app = three_tracks();
    press(&mut app, at(2, PLUS));
    painted(&mut app);
    assert_eq!(app.opened(String::from(NEXT), Some(&song), None), Opened::Project, "wanted {NEXT} to open as a song");
    let after = settled(&mut app);
    let mut plain = three_tracks();
    assert_eq!(plain.opened(String::from(NEXT), Some(&song), None), Opened::Project, "wanted {NEXT} to open as a song");
    assert!(after == settled(&mut plain), "wanted the screen an opened song shows with no menu open, the menu's track being gone, but it differs");
}

#[test]
fn an_agent_removing_a_track_closes_the_add_menu_open_on_it() {
    let mut app = three_tracks();
    press(&mut app, at(2, PLUS));
    painted(&mut app);
    two_tracks_written(&mut app);
    let after = settled(&mut app);
    let mut plain = three_tracks();
    two_tracks_written(&mut plain);
    assert!(after == settled(&mut plain), "wanted the screen an agent's write leaves with no menu open, the menu's track being gone, but it differs");
}

#[test]
fn delete_pressed_while_a_track_is_dragged_removes_nothing() {
    let mut app = three_tracks();
    press(&mut app, at(2, BODY));
    let (x, y) = at(2, BODY);
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Move { x, y: y + DRAGGED_PX });
    key(&mut app, Key::Delete, false);
    painted(&mut app);
    app.event(Event::Up { x, y: y + DRAGGED_PX });
    painted(&mut app);
    let tracks = names(&mut app).len();
    assert_eq!(tracks, 3, "wanted Delete ignored while a track is held, as the other editing keys are, got {tracks} tracks");
}

#[test]
fn a_name_being_typed_lands_on_its_own_track_when_a_track_opens_above_it() {
    let mut app = App::new();
    painted(&mut app);
    let before = names(&mut app);
    renaming(&mut app, 1);
    let (x, y) = at(0, LANE);
    app.dropped(String::from(KICK), Kind::Wave, x, y);
    let asked = app.take_requests();
    assert!(asked.contains(&Request::Read { path: String::from(KICK) }), "wanted the dropped wave's read asked of the shell, got {asked:?}");
    app.read(String::from(KICK), Some(wav(&[KICK_PEAK; KICK_FRAMES])));
    key(&mut app, Key::Return, false);
    let after = names(&mut app);
    assert_eq!(after.len(), 3, "wanted the wave dropped on the first track, a MIDI track, to open an audio track under it, got {after:?}");
    assert_eq!((after[0].as_str(), after[2].as_str()), (before[0].as_str(), TYPED.to_string().as_str()), "wanted the typed name on the track being renamed, now third, and the first track's name left, got {after:?}");
}

#[test]
fn a_name_being_typed_is_dropped_when_an_agent_removes_its_track() {
    let mut app = three_tracks();
    renaming(&mut app, 2);
    two_tracks_written(&mut app);
    let written = names(&mut app);
    key(&mut app, Key::Return, false);
    painted(&mut app);
    let after = names(&mut app);
    assert_eq!(after, written, "wanted the two tracks the agent wrote left as they were once Return was pressed on a rename whose track was gone");
}

#[test]
fn a_name_being_typed_follows_its_track_when_an_agent_removes_a_track_above_it() {
    let mut app = three_tracks();
    renaming(&mut app, 1);
    first_track_removed(&mut app);
    let written = names(&mut app);
    assert_eq!(written.len(), 2, "wanted the agent's removal to leave two tracks, got {written:?}");
    key(&mut app, Key::Return, false);
    painted(&mut app);
    let after = names(&mut app);
    assert_eq!(after, vec![TYPED.to_string(), written[1].clone()], "wanted the typed name on the track being renamed, now first, once an agent had removed the track above it, and the next track's name left");
}

#[test]
fn grouping_the_selected_track_with_the_pointer_on_its_face_draws_the_group() {
    let mut app = App::new();
    painted(&mut app);
    hover_the_face(&mut app);
    key(&mut app, Key::Char('g'), true);
    painted(&mut app);
    let tracks = names(&mut app).len();
    assert_eq!(tracks, 3, "wanted the selected track grouped under a new group track and the screen drawn, got {tracks} tracks");
}

#[test]
fn a_wave_dropped_above_the_selected_track_with_the_pointer_on_the_face_draws() {
    let mut app = App::new();
    painted(&mut app);
    press(&mut app, at(1, BODY));
    hover_the_face(&mut app);
    let (x, y) = at(0, LANE);
    app.dropped(String::from(KICK), Kind::Wave, x, y);
    app.take_requests();
    app.read(String::from(KICK), Some(wav(&[KICK_PEAK; KICK_FRAMES])));
    painted(&mut app);
    let tracks = names(&mut app).len();
    assert_eq!(tracks, 3, "wanted the wave's new audio track opened under the first track, taking the selected track's slot, and the screen drawn, got {tracks} tracks");
}
