use bp::app::{App, Event, Opened, Request};
use bp::json::{self, Json};
use bp::keyboard::Key;
use bp::project::Project;
use support::{HEIGHT, WIDTH};

const A: &str = "/tmp/save-ticket-a.json";
const B: &str = "/tmp/save-ticket-b.json";

fn fresh() -> App {
    let mut app = App::new();
    app.draw(WIDTH, HEIGHT);
    app.take_requests();
    app
}

fn press(app: &mut App, letter: char, shift: bool) {
    app.event(Event::Key { key: Key::Char(letter), shift, command: true, repeat: false });
}

fn save_request(app: &mut App) -> (Option<String>, String, u64) {
    app.take_requests().into_iter().find_map(|request| match request {
        Request::Write { path, json, save } => Some((Some(path), json, save)),
        Request::SaveAs { json, save } => Some((None, json, save)),
        _ => None,
    }).expect("wanted a save request")
}

fn press_save(app: &mut App) -> (Option<String>, String, u64) {
    press(app, 's', false);
    save_request(app)
}

fn press_save_as(app: &mut App) -> (Option<String>, String, u64) {
    press(app, 's', true);
    save_request(app)
}

fn open_other(app: &mut App) {
    let mut project = Project::new();
    project.bpm = 99.0;
    assert_eq!(app.opened(String::from(B), Some(&project.to_json().write()), None), Opened::Project, "wanted the other project to open");
    app.take_requests();
}

fn vary(app: &mut App, wet: f64) {
    let preset = format!("{{\"kind\": \"rack\", \"rack\": [{{\"kind\": \"core\"}}, {{\"kind\": \"freak\", \"wet\": {wet}}}]}}");
    assert_eq!(app.opened(String::from("marker.json"), Some(&preset), None), Opened::Preset, "wanted the marker rack to load as a preset");
    app.take_requests();
}

fn versions_in(json: &str) -> usize {
    json::parse(json).expect("a save is valid JSON").get("versions").and_then(Json::as_array).map_or(0, <[Json]>::len)
}

#[test]
fn a_save_that_lands_after_another_project_opened_leaves_it_alone() {
    let mut app = fresh();
    let (_, _, save) = press_save(&mut app);
    open_other(&mut app);
    app.saved(String::from(A), save);
    let (path, _, _) = press_save(&mut app);
    assert_eq!(path.as_deref(), Some(B), "wanted B to keep saving to its own path after A's save landed late, got {path:?}");
}

#[test]
fn a_cancelled_save_as_does_not_disown_the_save_before_it() {
    let mut app = fresh();
    let (_, _, save) = press_save(&mut app);
    let (panel, _, cancelled) = press_save_as(&mut app);
    assert_eq!(panel, None, "wanted Save As to ask the shell for a path");
    app.unsaved(cancelled);
    app.saved(String::from(A), save);
    let (path, _, _) = press_save(&mut app);
    assert_eq!(path.as_deref(), Some(A), "wanted the first save to land although the Save As after it was cancelled, got {path:?}");
}

#[test]
fn a_save_after_a_cancelled_save_as_keeps_every_version_the_file_holds() {
    let mut app = fresh();
    let (_, first, save) = press_save(&mut app);
    assert_eq!(versions_in(&first), 1, "wanted the first save to hold one version");
    app.saved(String::from(A), save);
    vary(&mut app, 0.25);
    let (_, cancelled, ticket) = press_save_as(&mut app);
    assert_eq!(versions_in(&cancelled), 1, "wanted the Save As to start a fresh lineage holding only the changed project");
    app.unsaved(ticket);
    let (_, again, save) = press_save(&mut app);
    assert_eq!(versions_in(&again), 2, "wanted the save after the cancelled Save As to hold the file's two versions and no phantom");
    app.saved(String::from(A), save);
    vary(&mut app, 0.75);
    let (_, next, _) = press_save(&mut app);
    assert_eq!(versions_in(&next), 3, "wanted three versions across three landed states, got {}", versions_in(&next));
}

#[test]
fn two_saves_of_one_change_landing_together_keep_one_version() {
    let mut app = fresh();
    let (_, _, first) = press_save(&mut app);
    app.saved(String::from(A), first);
    vary(&mut app, 0.25);
    let (_, _, second) = press_save(&mut app);
    let (_, _, third) = press_save(&mut app);
    app.saved(String::from(A), second);
    app.saved(String::from(A), third);
    vary(&mut app, 0.75);
    let (_, next, _) = press_save(&mut app);
    assert_eq!(versions_in(&next), 3, "wanted one version per changed state although two saves of one change landed, got {}", versions_in(&next));
}

#[test]
fn a_save_asked_while_an_earlier_one_is_still_writing_carries_its_version() {
    let mut app = fresh();
    let (_, _, first) = press_save(&mut app);
    app.saved(String::from(A), first);
    vary(&mut app, 0.25);
    let (path, _, pending) = press_save(&mut app);
    assert_eq!(path.as_deref(), Some(A), "wanted the second save to write straight to the path");
    vary(&mut app, 0.75);
    let (_, later, _) = press_save(&mut app);
    assert_eq!(versions_in(&later), 3, "wanted the file to carry the version of the save still writing, got {}", versions_in(&later));
    app.saved(String::from(A), pending);
}

#[test]
fn an_unsaved_ticket_cannot_land_later() {
    let mut app = fresh();
    let (_, _, save) = press_save(&mut app);
    app.unsaved(save);
    app.saved(String::from(A), save);
    let (path, _, _) = press_save(&mut app);
    assert_eq!(path, None, "wanted a cancelled Save As to leave the project without a path, got {path:?}");
}

#[test]
fn an_accepted_save_as_carries_its_version_into_the_saves_after_it() {
    let mut app = fresh();
    let (_, _, first) = press_save(&mut app);
    app.saved(String::from(A), first);
    vary(&mut app, 0.25);
    let (_, _, accepted) = press_save_as(&mut app);
    app.assured(accepted);
    vary(&mut app, 0.75);
    let (_, later, _) = press_save(&mut app);
    assert_eq!(versions_in(&later), 3, "wanted the accepted Save As's version carried, got {}", versions_in(&later));
}

#[test]
fn a_failed_save_leaves_its_version_to_the_save_that_carried_it() {
    let mut app = fresh();
    let (_, _, first) = press_save(&mut app);
    app.saved(String::from(A), first);
    vary(&mut app, 0.25);
    let (_, _, failed) = press_save(&mut app);
    let (_, carrying, carrier) = press_save(&mut app);
    assert_eq!(versions_in(&carrying), 2, "wanted the second save to carry the first's version");
    app.unsaved(failed);
    app.saved(String::from(A), carrier);
    vary(&mut app, 0.75);
    let (_, next, _) = press_save(&mut app);
    assert_eq!(versions_in(&next), 3, "wanted the version the failed save minted kept by the save that carried it, got {}", versions_in(&next));
}
