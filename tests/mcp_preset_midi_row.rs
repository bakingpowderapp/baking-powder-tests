use bp::app::App;
use bp::factory::PRESETS;
use bp::json::{self, Json};
use bp::project::Project;
use std::path::Path;
use support::call;

const GROWL: &str = "Instruments/Basses/Growl.json";
const GATE: f64 = 0.3;

fn open(name: &str) -> App {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("Factory");
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in PRESETS.into_iter().filter(|(path, _)| *path == GROWL) {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().expect("wanted a preset's folder")).unwrap_or_else(|error| panic!("wanted {}: {error}", file.display()));
        std::fs::write(&file, text).unwrap_or_else(|error| panic!("wanted {} written: {error}", file.display()));
    }
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false}}", Project::new().to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.factory(root.to_str().map(String::from).expect("wanted the factory's path as text"));
    app.take_requests();
    app
}

fn answered(app: &mut App, tool: &str, args: &str) -> Json {
    let reply = call(app, tool, args);
    assert!(!reply.failed, "wanted {tool} {args} answered, got {}", reply.text);
    json::parse(&reply.text).unwrap_or_else(|| panic!("wanted {tool} {args} to answer JSON, got {}", reply.text))
}

fn written(app: &mut App, args: &str) -> String {
    let reply = call(app, "write", args);
    assert!(!reply.failed, "wanted write {args} taken, got {}", reply.text);
    reply.text
}

fn preset() -> Json {
    let (_, text) = PRESETS.iter().find(|(known, _)| *known == GROWL).unwrap_or_else(|| panic!("wanted {GROWL} in the factory"));
    json::parse(text).unwrap_or_else(|| panic!("wanted {GROWL} to parse"))
}

fn kinds(rack: Option<&Json>) -> Vec<String> {
    rack.and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(|row| row.get("kind")).filter_map(Json::as_str).map(String::from).collect()
}

fn lane_rows(lanes: Option<&Json>) -> Vec<f64> {
    lanes.and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(|lane| lane.get("row")).filter_map(Json::as_f64).collect()
}

#[test]
fn a_preset_written_at_a_tracks_preset_path_keeps_its_midi_row_and_moves_the_lanes_under_it() {
    let mut app = open("preset_kept_midi_row");
    written(&mut app, &format!(r#"{{"path":"tracks/0","value":{{"name":"Bass","kind":"midi","rack":[{{"kind":"midi","arp":{{"on":true,"gate":{GATE}}}}},{{"kind":"core"}}]}}}}"#));
    let reply = written(&mut app, &format!(r#"{{"path":"tracks/0/preset","value":"{GROWL}","details":true}}"#));
    let sound = preset();
    let rows = kinds(sound.get("rack")).len() + 1;
    assert!(reply.contains(&format!("tracks/0: {GROWL}, {rows} rows, midi, ")), "wanted the detailed reply to count the kept MIDI row first among the rows that landed, got {reply}");
    let track = answered(&mut app, "read", r#"{"path":"tracks/0"}"#);
    let first = track.get("rack").and_then(Json::as_array).and_then(<[Json]>::first).unwrap_or_else(|| panic!("wanted a rack on the track, got {}", track.write()));
    assert_eq!(first.get("kind").and_then(Json::as_str), Some("midi"), "wanted the track's MIDI row kept at row 0, got {}", track.write());
    assert_eq!(first.get("arp").and_then(|arp| arp.get("on")).and_then(Json::as_bool), Some(true), "wanted the kept MIDI row's arp still on, got {}", first.write());
    assert_eq!(first.get("arp").and_then(|arp| arp.get("gate")).and_then(Json::as_f64), Some(GATE), "wanted the kept MIDI row's gate as it was set, got {}", first.write());
    let wanted: Vec<String> = std::iter::once(String::from("midi")).chain(kinds(sound.get("rack"))).collect();
    assert_eq!(kinds(track.get("rack")), wanted, "wanted the preset's rows under the kept MIDI row, got {}", track.write());
    let lowered: Vec<f64> = lane_rows(sound.get("lfos")).into_iter().map(|row| row + 1.0).collect();
    assert!(!lowered.is_empty(), "wanted {GROWL} to carry LFO lanes to move");
    assert_eq!(lane_rows(track.get("lfos")), lowered, "wanted the preset's LFO lanes one row down, on the rows they named under the kept MIDI row, got {}", track.write());
}
