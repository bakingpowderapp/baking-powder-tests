use bp::app::{App, Event, Request};
use bp::draw::Tone;
use bp::json::{self, Json};
use bp::project::{Project, Track};
use support::WIDTH;

const RAIL: i32 = 45;
const TILE: i32 = 120;
const BORDER: i32 = 8;
const PITCH: i32 = TILE - BORDER;
const NUDGE: i32 = 50;
const TALL: i32 = 1200;

type Shape = Vec<(String, bool, bool)>;

fn plain(name: &str) -> Track {
    Track { name: String::from(name), ..Track::blank() }
}

fn member(name: &str) -> Track {
    Track { inside: true, ..plain(name) }
}

fn group(name: &str, folded: bool) -> Track {
    Track { name: String::from(name), folded, ..Track::group() }
}

fn project(tracks: Vec<Track>) -> Project {
    let mut project = Project::new();
    project.tracks = tracks;
    project
}

fn four() -> Project {
    project(vec![plain("A"), plain("B"), plain("C"), plain("D")])
}

fn open(project: &Project, selected: usize, picked: &[usize]) -> App {
    let picks: Vec<String> = picked.iter().map(usize::to_string).collect();
    let session = format!("{{\"project\":{},\"automation\":false,\"keyboard\":false,\"palette\":false,\"selected\":{selected},\"picked_tracks\":[{}]}}", project.to_json().write(), picks.join(","));
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.draw(WIDTH, TALL);
    app.take_requests();
    app
}

fn grip(slot: usize) -> (i32, i32) {
    (TILE / 2, RAIL + slot as i32 * PITCH + 2 * BORDER)
}

fn lift(app: &mut App, slot: usize, by: i32) -> (i32, i32) {
    let (x, y) = grip(slot);
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Move { x, y: y + by });
    (x, y + by)
}

fn drag(app: &mut App, slot: usize, slots: i32) -> Json {
    let (x, y) = lift(app, slot, slots * PITCH);
    app.event(Event::Up { x, y });
    let text = app.take_requests().into_iter().rev().find_map(|request| if let Request::Remember { json } = request { Some(json) } else { None }).expect("wanted the session remembered after the drop");
    json::parse(&text).expect("wanted the remembered session to parse")
}

fn shape(session: &Json) -> Shape {
    let project = Project::from_json(session.get("project").expect("wanted the session to hold the project")).expect("wanted the remembered project to read");
    project.tracks.iter().map(|track| (track.name.clone(), track.group, track.inside)).collect()
}

fn names(session: &Json) -> Vec<String> {
    shape(session).into_iter().map(|(name, _, _)| name).collect()
}

fn outside(name: &str) -> (String, bool, bool) {
    (String::from(name), false, false)
}

fn inside(name: &str) -> (String, bool, bool) {
    (String::from(name), false, true)
}

fn header(name: &str) -> (String, bool, bool) {
    (String::from(name), true, false)
}

fn picks(session: &Json) -> (i64, Vec<i64>) {
    let selected = session.get("selected").and_then(Json::as_i64).unwrap_or(0);
    let picked = session.get("picked_tracks").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_i64).collect();
    (selected, picked)
}

fn title_at(app: &mut App, top: i32, tone: Tone) -> bool {
    app.draw(WIDTH, TALL).iter().any(|fill| fill.tone == tone && fill.rect.x == BORDER && fill.rect.y == top + BORDER && fill.rect.w == TILE - 2 * BORDER)
}

#[test]
fn dragging_one_of_two_selected_tracks_carries_both() {
    let mut app = open(&four(), 0, &[2]);
    let dropped = drag(&mut app, 0, 2);
    assert_eq!(names(&dropped), ["B", "D", "A", "C"], "wanted A and C carried past D together and in their order, got {:?}", names(&dropped));
    assert_eq!(picks(&dropped), (2, vec![3]), "wanted A still selected and C still picked where they landed, got {:?}", picks(&dropped));
}

#[test]
fn the_lower_tile_carries_the_upper_one_too() {
    let mut app = open(&four(), 3, &[1]);
    let dropped = drag(&mut app, 3, -2);
    assert_eq!(names(&dropped), ["B", "D", "A", "C"], "wanted B and D gathered at the top by a grab on D, got {:?}", names(&dropped));
}

#[test]
fn selected_tracks_gather_beside_the_one_that_stays() {
    let mut app = open(&four(), 0, &[3]);
    let dropped = drag(&mut app, 3, -2);
    assert_eq!(names(&dropped), ["A", "D", "B", "C"], "wanted D brought up beside A, which was already there, got {:?}", names(&dropped));
}

#[test]
fn a_track_outside_the_selection_moves_alone() {
    let mut app = open(&four(), 0, &[2]);
    let dropped = drag(&mut app, 1, 2);
    assert_eq!(names(&dropped), ["A", "C", "D", "B"], "wanted B alone carried to the end, got {:?}", names(&dropped));
    assert_eq!(picks(&dropped), (0, vec![1]), "wanted A selected and C picked as before, C now second, got {:?}", picks(&dropped));
}

#[test]
fn one_track_still_drags_on_its_own() {
    let mut app = open(&four(), 0, &[]);
    let dropped = drag(&mut app, 1, 1);
    assert_eq!(names(&dropped), ["A", "C", "B", "D"], "wanted B dropped one slot down, got {:?}", names(&dropped));
}

#[test]
fn a_press_without_a_drag_selects_that_track_alone() {
    let mut app = open(&four(), 0, &[2]);
    let pressed = drag(&mut app, 2, 0);
    assert_eq!((names(&pressed), picks(&pressed)), (vec![String::from("A"), String::from("B"), String::from("C"), String::from("D")], (2, Vec::new())), "wanted a press on C to select it alone and move nothing");
}

#[test]
fn both_tiles_float_together_while_dragged() {
    let mut app = open(&four(), 0, &[2]);
    assert!(title_at(&mut app, RAIL, Tone::Ink100) && title_at(&mut app, RAIL + 2 * PITCH, Tone::Ink75), "wanted A lit as selected and C as picked on their slots before the drag");
    lift(&mut app, 0, NUDGE);
    let floating = (title_at(&mut app, RAIL + NUDGE, Tone::Ink100), title_at(&mut app, RAIL + NUDGE + PITCH, Tone::Ink75));
    assert_eq!(floating, (true, true), "wanted A's tile under the pointer and C's stacked a slot below it");
    assert!(!title_at(&mut app, RAIL + 2 * PITCH, Tone::Ink75), "wanted C's tile lifted off its slot");
}

#[test]
fn a_group_dragged_with_a_track_below_keeps_its_members_and_leaves_the_track_outside() {
    let tracks = vec![group("DRUMS", false), member("KICK"), member("SNARE"), plain("BASS"), plain("LEAD")];
    let mut app = open(&project(tracks), 0, &[3]);
    let dropped = shape(&drag(&mut app, 0, 1));
    assert_eq!(dropped, [outside("LEAD"), header("DRUMS"), inside("KICK"), inside("SNARE"), outside("BASS")], "wanted the group and BASS below LEAD, the members still inside and BASS still out");
}

#[test]
fn a_folded_group_takes_its_hidden_members_along() {
    let tracks = vec![group("DRUMS", true), member("KICK"), member("SNARE"), plain("LEAD"), plain("BASS")];
    let mut app = open(&project(tracks), 0, &[4]);
    let dropped = shape(&drag(&mut app, 0, 1));
    assert_eq!(dropped, [outside("LEAD"), header("DRUMS"), inside("KICK"), inside("SNARE"), outside("BASS")], "wanted the folded group below LEAD with its members and BASS after them");
}

#[test]
fn tracks_dropped_among_a_groups_members_join_it() {
    let tracks = vec![group("DRUMS", false), member("KICK"), member("SNARE"), plain("PAD"), plain("BASS"), plain("LEAD")];
    let mut app = open(&project(tracks), 4, &[5]);
    let dropped = shape(&drag(&mut app, 4, -2));
    assert_eq!(dropped, [header("DRUMS"), inside("KICK"), inside("BASS"), inside("LEAD"), inside("SNARE"), outside("PAD")], "wanted BASS and LEAD between the members and inside the group");
}

#[test]
fn members_dragged_out_together_leave_the_group() {
    let tracks = vec![group("DRUMS", false), member("KICK"), member("SNARE"), member("HAT"), plain("BASS")];
    let mut app = open(&project(tracks), 1, &[3]);
    let dropped = shape(&drag(&mut app, 1, 2));
    assert_eq!(dropped, [header("DRUMS"), inside("SNARE"), outside("BASS"), outside("KICK"), outside("HAT")], "wanted KICK and HAT after BASS and outside, SNARE left inside");
}

#[test]
fn a_track_resting_under_a_group_stays_outside_when_another_is_gathered_to_it() {
    let tracks = vec![group("DRUMS", false), member("KICK"), member("SNARE"), plain("BASS"), plain("PAD"), plain("LEAD")];
    let mut app = open(&project(tracks), 3, &[5]);
    let dropped = shape(&drag(&mut app, 5, -1));
    assert_eq!(dropped, [header("DRUMS"), inside("KICK"), inside("SNARE"), outside("BASS"), outside("LEAD"), outside("PAD")], "wanted LEAD brought up beside BASS and both still outside the group");
}
