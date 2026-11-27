use bp::app::App;
use bp::json::{self, Json};
use bp::mcp::Call;
use bp::project::Project;
use std::time::Duration;
use support::call;

const PATIENCE: Duration = Duration::from_secs(120);
const TICK: f32 = 0.01;
const PAUSE: Duration = Duration::from_millis(2);

fn open() -> App {
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false}}", Project::new().to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.take_requests();
    app
}

fn listened(app: &mut App, args: &str) -> Json {
    let (listen, waiting) = Call::new("listen", json::parse(args).expect("wanted the arguments to parse"));
    app.deliver(listen);
    let answer = std::thread::spawn(move || waiting.reply(PATIENCE));
    while !answer.is_finished() {
        app.tick(TICK);
        std::thread::sleep(PAUSE);
    }
    let reply = answer.join().expect("wanted the listen answered");
    assert!(!reply.failed, "wanted listen {args} answered, got {}", reply.text);
    json::parse(&reply.text).unwrap_or_else(|| panic!("wanted listen to answer JSON, got {}", reply.text))
}

fn flags(heard: &Json) -> Vec<String> {
    heard.get("flags").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).map(String::from).collect()
}

#[test]
fn a_handover_between_spans_within_two_lu_is_flagged_naming_its_tracks_and_a_plateau_or_a_lift_is_not() {
    let mut app = open();
    let edits = r#"{"edits":[
        {"path":"tracks/0/clips/0","value":{"startBeat":0,"lengthBeats":16,"name":"first","notes":{"steps":"x...","pitch":60}}},
        {"path":"tracks/1","value":{"name":"Twin","kind":"midi","clips":[{"startBeat":16,"lengthBeats":16,"name":"second","notes":{"steps":"x...","pitch":60}}]}},
        {"path":"tracks/2","value":{"name":"Third","kind":"midi","clips":[{"startBeat":24,"lengthBeats":8,"name":"last","notes":{"steps":"x...","pitch":60}}]}}
    ]}"#;
    let reply = call(&mut app, "write", edits);
    assert!(!reply.failed, "wanted the three tracks written, got {}", reply.text);
    let heard = listened(&mut app, r#"{"spans":[[0,8],[8,16],[16,24],[24,32]]}"#);
    let flags = flags(&heard);
    let handover = flags.iter().find(|flag| flag.starts_with("spans 1 and 2:"));
    assert!(handover.is_some_and(|flag| flag.contains("Twin enters") && flag.contains("MIDI 1 leaves")), "wanted the handover from MIDI 1 to Twin at one loudness flagged and both named, got {flags:?} from {}", heard.write());
    assert!(!flags.iter().any(|flag| flag.starts_with("spans 0 and 1:")), "wanted two spans with the same track sounding left unflagged as a plateau meant, got {flags:?} from {}", heard.write());
    assert!(!flags.iter().any(|flag| flag.starts_with("spans 2 and 3:")), "wanted the lift from one track to two left unflagged, got {flags:?} from {}", heard.write());
}
