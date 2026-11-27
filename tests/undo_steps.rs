use bp::app::{App, Event, Opened, Request};
use bp::arp::Articulation;
use bp::json::{self, Json};
use bp::keyboard::Key;
use bp::project::{Clip, Note, Project, TICKS_PER_BAR, VELOCITY};
use support::{HEIGHT, WIDTH};

const STEPS: u32 = 250;
const EDITS: u32 = 260;
const SMALL_EDITS: u32 = 50;
const NOTES: usize = 400;
const WHOLE_COPIES: usize = 4;
const RAIL: i32 = 45;
const TILE: i32 = 120;
const BORDER: i32 = 8;
const TITLE: (i32, i32) = (TILE / 2, RAIL + 2 * BORDER);
const NAMES: [&str; 4] = ["é", "è", "🎹", "🎸"];
const DAMAGED: usize = 3;

fn fresh() -> App {
    let mut app = App::new();
    app.draw(WIDTH, HEIGHT);
    app.take_requests();
    app
}

fn relaunched(session: &str) -> App {
    let mut app = fresh();
    assert!(app.resume(session), "wanted the session to resume");
    app.draw(WIDTH, HEIGHT);
    app.take_requests();
    app
}

fn parsed(text: &str) -> Json {
    json::parse(text).expect("wanted the session to parse as JSON")
}

fn remembered(app: &mut App) -> Option<String> {
    app.take_requests().into_iter().rev().find_map(|request| if let Request::Remember { json } = request { Some(json) } else { None })
}

fn mark(app: &mut App, marker: u32) -> String {
    let preset = format!("{{\"kind\": \"rack\", \"rack\": [{{\"kind\": \"core\"}}, {{\"kind\": \"freak\", \"wet\": {}}}]}}", f64::from(marker) / 1000.0);
    assert_eq!(app.opened(String::from("marker.json"), Some(&preset), None), Opened::Preset, "wanted the marker rack to load as a preset");
    remembered(app).expect("wanted the session remembered after the marker landed")
}

fn marker_of(session: &str) -> u32 {
    let session = parsed(session);
    let rack = session.get("project").and_then(|project| project.get("tracks")).and_then(Json::as_array).and_then(|tracks| tracks.first()).and_then(|track| track.get("rack")).and_then(Json::as_array);
    let wet = rack.and_then(|rack| rack.get(2)).and_then(|row| row.get("wet")).and_then(Json::as_f64);
    wet.map_or(0, |wet| (wet * 1000.0).round() as u32)
}

fn name_of(session: &str) -> String {
    let project = Project::from_json(parsed(session).get("project").expect("wanted the session to hold the project")).expect("wanted the remembered project to read");
    project.tracks[0].name.clone()
}

fn command_z(app: &mut App, shift: bool) -> Option<String> {
    app.event(Event::Key { key: Key::Char('z'), shift, command: true, repeat: false });
    remembered(app)
}

fn undo(app: &mut App) -> Option<u32> {
    command_z(app, false).map(|session| marker_of(&session))
}

fn redo(app: &mut App) -> Option<u32> {
    command_z(app, true).map(|session| marker_of(&session))
}

fn walked_back(app: &mut App) -> Vec<u32> {
    std::iter::from_fn(|| undo(app)).collect()
}

fn steps(session: &str) -> usize {
    parsed(session).get("edits").and_then(Json::as_array).map_or(0, <[Json]>::len)
}

fn rename(app: &mut App, name: &str) -> String {
    let (x, y) = TITLE;
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    for letter in name.chars() {
        app.event(Event::Key { key: Key::Char(letter), shift: false, command: false, repeat: false });
    }
    app.event(Event::Key { key: Key::Return, shift: false, command: false, repeat: false });
    remembered(app).expect("wanted the session remembered after the rename")
}

#[test]
fn a_relaunch_walks_two_hundred_and_fifty_steps_back() {
    let mut app = fresh();
    let mut session = String::new();
    for marker in 1..=EDITS {
        session = mark(&mut app, marker);
    }
    assert_eq!(steps(&session), STEPS as usize, "wanted the session to carry {STEPS} steps after {EDITS} edits, got {}", steps(&session));
    let walked = walked_back(&mut relaunched(&session));
    let wanted: Vec<u32> = (EDITS - STEPS + 1..EDITS).rev().collect();
    assert_eq!(walked, wanted, "wanted undo after a relaunch to step back through every kept edit, newest first, and stop at the oldest");
}

#[test]
fn a_step_costs_what_changed_not_a_whole_project() {
    let mut project = Project::new();
    let notes = (0..NOTES).map(|at| Note { pitch: 36 + (at % 48) as u8, start: at as f32 * 0.125, length: 0.125, velocity: VELOCITY, articulation: Articulation::PLAIN }).collect();
    project.tracks[0].clips.push(Clip { start: 0, length: 16 * TICKS_PER_BAR, label: String::from("long"), notes, file: None });
    let document = project.to_json().write();
    let mut app = relaunched(&format!("{{\"project\":{document}}}"));
    let mut session = String::new();
    for marker in 1..=SMALL_EDITS {
        session = mark(&mut app, marker);
    }
    assert_eq!(steps(&session), SMALL_EDITS as usize + 1, "wanted the opening project and each of the {SMALL_EDITS} edits kept as a step, got {}", steps(&session));
    assert!(session.len() < WHOLE_COPIES * document.len(), "wanted {SMALL_EDITS} one-knob steps on a {}-byte project to fit in {WHOLE_COPIES} copies of it, got a {}-byte session", document.len(), session.len());
    let walked = walked_back(&mut relaunched(&session));
    let wanted: Vec<u32> = (0..SMALL_EDITS).rev().collect();
    assert_eq!(walked, wanted, "wanted every step on the long project back after a relaunch");
}

#[test]
fn undo_redo_and_a_fresh_edit_survive_two_relaunches() {
    let mut app = fresh();
    for marker in 1..=5 {
        mark(&mut app, marker);
    }
    let undone = [undo(&mut app), undo(&mut app), undo(&mut app)];
    assert_eq!(undone, [Some(4), Some(3), Some(2)], "wanted three undos to step back from the fifth edit to the second");
    assert_eq!(redo(&mut app), Some(3), "wanted redo to return the third edit");
    let session = mark(&mut app, 9);
    let mut second = relaunched(&session);
    let session = mark(&mut second, 10);
    let walked = walked_back(&mut relaunched(&session));
    assert_eq!(walked, vec![9, 3, 2, 1, 0], "wanted the branch taken after the redo, then the edits before it, down to the untouched project");
}

#[test]
fn redo_still_works_on_steps_a_relaunch_brought_back() {
    let mut app = fresh();
    let mut session = String::new();
    for marker in 1..=4 {
        session = mark(&mut app, marker);
    }
    let mut resumed = relaunched(&session);
    let back = [undo(&mut resumed), undo(&mut resumed)];
    let forth = [redo(&mut resumed), redo(&mut resumed), redo(&mut resumed)];
    assert_eq!((back, forth), ([Some(3), Some(2)], [Some(3), Some(4), None]), "wanted two undos and two redos to return to the newest edit and a third redo to do nothing");
    let session = mark(&mut resumed, 7);
    let walked = walked_back(&mut relaunched(&session));
    assert_eq!(walked, vec![4, 3, 2, 1, 0], "wanted the whole stack back after undoing and redoing across a relaunch");
}

#[test]
fn names_past_ascii_come_back_letter_for_letter() {
    let mut app = fresh();
    let original = Project::new().tracks[0].name.clone();
    let mut session = String::new();
    for name in NAMES {
        session = rename(&mut app, name);
        assert_eq!(name_of(&session), name, "wanted the first track renamed to {name}");
    }
    let mut resumed = relaunched(&session);
    let walked: Vec<String> = std::iter::from_fn(|| command_z(&mut resumed, false)).map(|session| name_of(&session)).collect();
    let wanted: Vec<String> = NAMES.iter().rev().skip(1).map(|name| String::from(*name)).chain(std::iter::once(original)).collect();
    assert_eq!(walked, wanted, "wanted each earlier name back whole after a relaunch, though neighbours share the first bytes of a letter");
}

#[test]
fn a_session_whose_steps_cannot_be_read_resumes_without_them() {
    let mut app = fresh();
    mark(&mut app, 1);
    let document = parsed(&mark(&mut app, 2)).get("project").expect("wanted the session to hold the project").write();
    let unreadable = format!("{{\"project\":{document},\"edits\":[{{\"at\":1,\"project\":{document}}},{{\"at\":2,\"project\":{document}}}]}}");
    let mut resumed = relaunched(&unreadable);
    let pressed = [undo(&mut resumed), undo(&mut resumed)];
    assert_eq!(pressed, [Some(2), None], "wanted the project resumed as it was left with nothing to undo");
}

#[test]
fn a_damaged_step_keeps_the_newer_ones() {
    let mut app = fresh();
    let mut session = String::new();
    for marker in 1..=5 {
        session = mark(&mut app, marker);
    }
    let mut damaged = parsed(&session);
    if let Json::Object(fields) = &mut damaged
        && let Some((_, Json::Array(edits))) = fields.iter_mut().find(|(key, _)| key == "edits")
        && let Some(Json::Object(step)) = edits.get_mut(DAMAGED)
    {
        step.retain(|(key, _)| key != "head");
        step.push((String::from("head"), Json::Number(1e12)));
    }
    let walked = walked_back(&mut relaunched(&damaged.write()));
    assert_eq!(walked, vec![4, 3], "wanted the steps newer than the damaged one kept and the rest let go");
}
