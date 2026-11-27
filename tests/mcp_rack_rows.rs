use bp::app::App;
use bp::arp::Arp;
use bp::fx::Kind;
use bp::json::{self, Json};
use bp::project::{Project, Row};
use support::{CORE_ROW, HEIGHT, LEVEL, WIDTH, call, core, effect, midi, remembered, session};

const QUIET: f64 = 0.125;
const DELAY: usize = 3;
const SECOND_CORE: usize = 2;

fn racked(rack: Vec<Row>) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = rack;
    project
}

fn open(project: &Project, state: &str) -> App {
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false,\"selected\":0{state}}}", project.to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.draw(WIDTH, HEIGHT);
    app.take_requests();
    app
}

fn written(app: &mut App, args: &str) -> String {
    let reply = call(app, "write", args);
    assert!(!reply.failed, "wanted the write taken: {args}, got {}", reply.text);
    reply.text
}

fn read(app: &mut App, args: &str) -> Json {
    let reply = call(app, "read", args);
    assert!(!reply.failed, "wanted read {args} answered, got {}", reply.text);
    json::parse(&reply.text).unwrap_or_else(|| panic!("wanted read {args} to answer JSON, got {}", reply.text))
}

fn open_row(app: &mut App) -> Option<String> {
    read(app, "{}").get("openRow").and_then(Json::as_str).map(String::from)
}

fn kinds(app: &mut App) -> Vec<String> {
    read(app, r#"{"path":"tracks/0/rack"}"#).as_array().unwrap_or(&[]).iter().map(|row| String::from(row.get("kind").and_then(Json::as_str).unwrap_or("?"))).collect()
}

fn soloed(app: &mut App) -> Option<(f64, bool)> {
    let session = session(app);
    let solo = session.get("generators").and_then(Json::as_array).and_then(<[Json]>::first).and_then(|generator| generator.get("core")).and_then(Json::as_array)?;
    Some((solo.first()?.as_f64()?, solo.get(1)?.as_bool()?))
}

fn seeds(project: &Project) -> Vec<u64> {
    project.tracks[0].rack.iter().filter_map(|row| if let Row::Core(layer) = row { Some(layer.seed) } else { None }).collect()
}

#[test]
fn a_read_gives_the_row_whose_face_the_app_shows() {
    let project = racked(vec![midi(Arp::DEFAULT), core(LEVEL), effect(Kind::Reverb), effect(Kind::Delay)]);
    let mut app = open(&project, &format!(",\"open\":{DELAY}"));
    assert_eq!(open_row(&mut app).as_deref(), Some("tracks/0/rack/3"), "wanted the delay's row, whose face is on screen, given as openRow");
    let alone = read(&mut app, r#"{"path":"openRow"}"#);
    assert_eq!(alone.as_str(), Some("tracks/0/rack/3"), "wanted openRow read at its own path");
    let mut app = open(&project, &format!(",\"open\":{CORE_ROW}"));
    assert_eq!(open_row(&mut app).as_deref(), Some("tracks/0/rack/1"), "wanted the Core's row given when its face shows");
    let refused = call(&mut app, "write", r#"{"path":"openRow","value":"tracks/0/rack/2"}"#);
    assert_eq!(open_row(&mut app).as_deref(), Some("tracks/0/rack/1"), "wanted openRow left as the app shows it, a write there changing nothing, got {}", refused.text);
}

#[test]
fn the_open_face_stays_on_its_own_row_as_an_agent_inserts_moves_and_removes_rows() {
    let project = racked(vec![midi(Arp::DEFAULT), core(LEVEL), effect(Kind::Reverb), effect(Kind::Delay)]);
    let mut app = open(&project, &format!(",\"open\":{DELAY}"));
    written(&mut app, r#"{"path":"tracks/0/rack/1","insert":{"kind":"eq"}}"#);
    assert_eq!((kinds(&mut app), open_row(&mut app)), (vec!["midi", "eq", "core", "reverb", "delay"].into_iter().map(String::from).collect(), Some(String::from("tracks/0/rack/4"))), "wanted the delay's face followed a row down under the inserted EQ");
    written(&mut app, r#"{"path":"tracks/0/rack/4","move":2}"#);
    assert_eq!((kinds(&mut app), open_row(&mut app)), (vec!["midi", "eq", "delay", "core", "reverb"].into_iter().map(String::from).collect(), Some(String::from("tracks/0/rack/2"))), "wanted the delay's face followed to the row it was moved to");
    written(&mut app, r#"{"path":"tracks/0/rack/4","move":3}"#);
    assert_eq!(open_row(&mut app).as_deref(), Some("tracks/0/rack/2"), "wanted the delay's face left where it is when rows below it change places");
    written(&mut app, r#"{"path":"tracks/0/rack/1","value":null}"#);
    assert_eq!((kinds(&mut app), open_row(&mut app)), (vec!["midi", "delay", "reverb", "core"].into_iter().map(String::from).collect(), Some(String::from("tracks/0/rack/1"))), "wanted the delay's face followed a row up when the EQ above it is removed");
}

#[test]
fn a_soloed_core_stays_soloed_on_its_own_row_as_an_agent_edits_the_rack_and_the_solo_ends_with_its_row() {
    let project = racked(vec![midi(Arp::DEFAULT), core(LEVEL), core(QUIET), effect(Kind::Delay)]);
    let mut app = open(&project, &format!(",\"generators\":[{{\"core\":[{SECOND_CORE},true]}}]"));
    written(&mut app, r#"{"path":"tracks/0/rack/1","insert":{"kind":"eq"}}"#);
    assert_eq!(soloed(&mut app), Some((3.0, true)), "wanted the second Core still soloed, effects kept, a row down under the inserted EQ");
    written(&mut app, r#"{"path":"tracks/0/rack/3","move":1}"#);
    assert_eq!(soloed(&mut app), Some((1.0, true)), "wanted the solo to follow its Core to the row it was moved to");
    written(&mut app, r#"{"path":"tracks/0/rack/1","value":null}"#);
    assert_eq!(soloed(&mut app), None, "wanted the solo ended when its Core's row is removed");
}

#[test]
fn a_row_an_agent_inserts_above_a_core_leaves_that_cores_random_draws_as_they_were() {
    let project = racked(vec![midi(Arp::DEFAULT), core(LEVEL), effect(Kind::Delay)]);
    let mut app = open(&project, "");
    written(&mut app, r#"{"path":"tracks/0/gain","value":0.5}"#);
    let before = seeds(&remembered(&mut app));
    assert!(matches!(before.as_slice(), [seed] if *seed != 0), "wanted the song's one Core seeded, got {before:?}");
    written(&mut app, r#"{"path":"tracks/0/rack/1","insert":{"kind":"eq"}}"#);
    assert_eq!(seeds(&remembered(&mut app)), before, "wanted the Core a row down to keep the run of random draws it had");
    written(&mut app, r#"{"path":"tracks/0/rack/1","insert":{"kind":"core"}}"#);
    let after = seeds(&remembered(&mut app));
    assert_eq!(after.len(), 2, "wanted the inserted Core beside the first, got {after:?}");
    assert!(after[1] == before[0] && after[0] != before[0] && after[0] != 0, "wanted the first Core's draws kept and the inserted Core given a run of its own, got {after:?} from {before:?}");
}
