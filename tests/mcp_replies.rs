use bp::app::App;
use bp::factory::PRESETS;
use bp::json::{self, Json};
use bp::project::Project;
use std::path::Path;
use support::call;

const MOST_REPLY_BYTES: usize = 40_000;
const GROWL: &str = "Instruments/Basses/Growl.json";
const GAS: &str = "Instruments/Basses/Gas.json";
const SEAR: &str = "Effects/Freak/Sear.json";
const LONG_OPENING: &str = "A bass that says too much";
const TRACKS: usize = 30;
const KEPT_DIFFERENTLY: &str = "Kept differently";
const TRACK: &str = "tracks/0";
const TRACK_GAIN: &str = "tracks/0/gain";
const GAIN_PAST_RANGE: f64 = 5.0;
const INSERTED_AT: usize = 1;
const FOLLOWED: usize = 1;
const GROWL_FREAK_ROW: usize = 3;
const FREAK_UNDER_MIDI_ROW: usize = 4;
const DRIVE: &str = "driveDb";
const DELAY_ROW: usize = 2;
const DELAY_KNOB: &str = "dry";
const CURVE: &str = "tracks/0/curves/0";
const LOW_NODE: f64 = 0.25;
const HIGH_NODE: f64 = 0.75;
const HIGH_NODE_AT: usize = 1;
const TWICE: f64 = 2.0;

fn open(name: &str, stocked: fn(&str) -> bool) -> App {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("Factory");
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in PRESETS.into_iter().filter(|(path, _)| stocked(path)) {
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

fn answered(app: &mut App, tool: &str, args: &str) -> (Json, usize) {
    let reply = call(app, tool, args);
    assert!(!reply.failed, "wanted {tool} {args} answered, got {}", reply.text);
    let json = json::parse(&reply.text).unwrap_or_else(|| panic!("wanted {tool} {args} to answer JSON, got {}", reply.text));
    (json, reply.text.len())
}

fn description(listing: &Json, path: &str) -> String {
    let entry = listing.get("entries").and_then(Json::as_array).unwrap_or(&[]).iter().find(|entry| entry.get("path").and_then(Json::as_str) == Some(path));
    entry.and_then(|entry| entry.get("description")).and_then(Json::as_str).map(String::from).unwrap_or_else(|| panic!("wanted {path} listed with a description in {}", listing.write()))
}

fn preset(path: &str) -> Json {
    let (_, text) = PRESETS.iter().find(|(known, _)| *known == path).unwrap_or_else(|| panic!("wanted {path} in the factory"));
    json::parse(text).unwrap_or_else(|| panic!("wanted {path} to parse"))
}

fn whole_description(path: &str) -> String {
    preset(path).get("description").and_then(Json::as_str).map(String::from).unwrap_or_else(|| panic!("wanted {path} to carry a description"))
}

fn row_number(path: &str, row: usize, key: &str) -> f64 {
    let sound = preset(path);
    let found = sound.get("rack").and_then(Json::as_array).and_then(|rows| rows.get(row)).and_then(|row| row.get(key)).and_then(Json::as_f64);
    found.unwrap_or_else(|| panic!("wanted {path} to carry {key} on row {row}"))
}

fn written(app: &mut App, args: &str) -> String {
    let reply = call(app, "write", args);
    assert!(!reply.failed, "wanted the write taken: {args}, got {}", reply.text);
    reply.text
}

fn told(reply: &str, path: &str) -> Option<String> {
    let lines = reply.lines().skip_while(|line| !line.starts_with(KEPT_DIFFERENTLY)).skip(1).take_while(|line| line.starts_with(' '));
    let rest = lines.filter_map(|line| line.trim().strip_prefix(path)).find_map(|rest| rest.strip_prefix(": "))?;
    rest.split(',').next().map(String::from)
}

fn assert_told(reply: &str, path: &str, wanted: &str, what: &str) {
    let told = told(reply, path);
    assert_eq!(told.as_deref(), Some(wanted), "wanted the reply to say {wanted} of {path}, {what}, got {told:?} in: {reply}");
}

#[test]
fn a_deep_listing_too_long_for_one_reply_cuts_each_description_to_its_opening_words() {
    let mut app = open("replies_listing", |_| true);
    let lengthy = Path::new(env!("CARGO_TARGET_TMPDIR")).join("replies_listing").join("Factory").join("Instruments").join("Lengthy");
    std::fs::create_dir_all(&lengthy).unwrap_or_else(|error| panic!("wanted {}: {error}", lengthy.display()));
    let padding = "and on ".repeat(MOST_REPLY_BYTES / 7);
    std::fs::write(lengthy.join("Long.json"), format!("{{\"kind\": \"rack\", \"description\": \"{LONG_OPENING}. It goes on {padding}\", \"rack\": [{{\"kind\": \"core\"}}]}}")).expect("wanted the lengthy preset written");
    let (deep, bytes) = answered(&mut app, "library", r#"{"path":"Instruments","deep":true}"#);
    assert!(bytes <= MOST_REPLY_BYTES, "wanted the deep listing within {MOST_REPLY_BYTES} bytes, got {bytes}");
    assert!(deep.get("cut").is_some(), "wanted the listing to say its descriptions were cut, got {}", deep.write());
    assert_eq!(description(&deep, "Lengthy/Long.json"), LONG_OPENING, "wanted the long description cut to the words before its first full stop");
    assert_eq!(description(&deep, "Basses/Gas.json"), whole_description(GAS), "wanted a description with no stop in it kept whole in the cut listing");
    let (narrow, _) = answered(&mut app, "library", r#"{"path":"Instruments/Basses"}"#);
    assert!(narrow.get("cut").is_none(), "wanted a listing of one folder whole, got {}", narrow.write());
    assert_eq!(description(&narrow, "Gas.json"), whole_description(GAS), "wanted the description whole in a listing that fits");
}

#[test]
fn the_cores_manual_fits_one_reply_whole_and_a_path_within_the_row_gives_one_part() {
    let mut app = open("replies_manual", |_| false);
    let (core, bytes) = answered(&mut app, "manual", r#"{"row":"core"}"#);
    assert!(bytes <= MOST_REPLY_BYTES, "wanted the core's manual within {MOST_REPLY_BYTES} bytes, got {bytes}");
    let Json::Object(fields) = &core else {
        panic!("wanted the core's manual as an object, got {}", core.write());
    };
    let folded: Vec<&str> = fields.iter().filter(|(_, value)| value.as_str().is_some_and(|line| line.contains("manual with row core and path"))).map(|(key, _)| key.as_str()).collect();
    assert!(folded.is_empty(), "wanted the simplified core's manual whole in one reply with nothing folded, got {}", core.write());
    let (filter, _) = answered(&mut app, "manual", r#"{"row":"core","path":"filter"}"#);
    assert!(filter.get("cutoffHz").is_some(), "wanted the filter's keys whole at row core and path filter, got {}", filter.write());
}

#[test]
fn a_document_too_long_to_send_whole_reads_as_its_outline() {
    let mut app = open("replies_outline", |path| path == GROWL);
    let tracks: Vec<String> = (0..TRACKS).map(|index| format!(r#"{{"name":"Bass {index}","kind":"midi","preset":"{GROWL}"}}"#)).collect();
    let reply = call(&mut app, "write", &format!(r#"{{"path":"tracks","value":[{}]}}"#, tracks.join(",")));
    assert!(!reply.failed, "wanted {TRACKS} tracks of one preset written, got {}", reply.text);
    let (outline, bytes) = answered(&mut app, "read", "{}");
    assert!(outline.get("outlined").and_then(Json::as_str).is_some(), "wanted the read to say it outlined a document too long to send whole, got {bytes} bytes");
    let rack = outline.get("tracks").and_then(Json::as_array).and_then(<[Json]>::first).and_then(|track| track.get("rack"));
    assert!(rack.and_then(Json::as_str).is_some_and(|line| line.starts_with("folded 6 rows, midi, ")), "wanted the first rack folded to a line, got {rack:?}");
    let (whole, _) = answered(&mut app, "read", r#"{"notes":true}"#);
    assert!(whole.get("outlined").is_none(), "wanted the document whole when notes are asked for");
}

#[test]
fn a_reply_says_sent_of_a_value_the_agent_typed_that_was_kept_differently() {
    let mut app = open("replies_sent_typed", |_| false);
    let reply = written(&mut app, &format!(r#"{{"path":"{TRACK_GAIN}","value":{GAIN_PAST_RANGE}}}"#));
    assert_told(&reply, TRACK_GAIN, &format!("sent {GAIN_PAST_RANGE}"), "the gain the agent typed past its range");
}

#[test]
fn a_reply_says_sent_with_the_number_the_agent_typed_before_the_insert_renumbered_it() {
    let mut app = open("replies_sent_renumbered", |_| false);
    let reply = written(&mut app, &format!(r#"{{"path":"tracks/{INSERTED_AT}","insert":{{"kind":"audio","follows":{FOLLOWED}}}}}"#));
    assert_told(&reply, &format!("tracks/{INSERTED_AT}/follows"), &format!("sent {FOLLOWED}"), "the track the agent typed, as numbered before the insert moved it down one");
}

#[test]
fn a_reply_says_sent_of_a_value_typed_beside_a_preset_and_nothing_of_the_values_the_preset_put_there() {
    let mut app = open("replies_sent_beside_preset", |path| path == GROWL);
    let reply = written(&mut app, &format!(r#"{{"path":"{TRACK}","value":{{"preset":"{GROWL}","gain":{GAIN_PAST_RANGE}}}}}"#));
    assert_told(&reply, TRACK_GAIN, &format!("sent {GAIN_PAST_RANGE}"), "the gain the agent typed beside the preset");
    let drive = format!("{TRACK}/rack/{FREAK_UNDER_MIDI_ROW}/{DRIVE}");
    let told = told(&reply, &drive);
    assert_eq!(told, None, "wanted nothing told of {drive}, kept as the preset wrote it, got {told:?} in: {reply}");
    let (kept, _) = answered(&mut app, "read", &format!(r#"{{"path":"{drive}"}}"#));
    assert_eq!(kept.as_f64(), Some(row_number(GROWL, GROWL_FREAK_ROW, DRIVE)), "wanted the preset's own drive on its freak row under the track's MIDI row, got {}", kept.write());
}

#[test]
fn a_reply_says_written_of_a_value_an_operation_put_there() {
    let mut app = open("replies_written_scaled", |_| false);
    written(&mut app, &format!(r#"{{"path":"{TRACK}/rack/end","value":{{"kind":"delay"}}}}"#));
    written(&mut app, &format!(r#"{{"path":"{CURVE}","value":{{"row":{DELAY_ROW},"param":"{DELAY_KNOB}","nodes":[{{"beat":0,"value":{LOW_NODE}}},{{"beat":4,"value":{HIGH_NODE}}}]}}}}"#));
    let reply = written(&mut app, &format!(r#"{{"path":"{CURVE}","scale":{TWICE}}}"#));
    assert_told(&reply, &format!("{CURVE}/nodes/{HIGH_NODE_AT}/value"), &format!("written {}", HIGH_NODE * TWICE), "the node the scale took past the knob's range, a number the agent never typed");
}

#[test]
fn a_reply_tells_of_a_gain_kept_differently_beside_a_preset_and_a_written_rack() {
    let mut app = open("replies_rack_beside", |path| path == GROWL);
    let reply = written(&mut app, &format!(r#"{{"path":"{TRACK}","value":{{"preset":"{GROWL}","gain":{GAIN_PAST_RANGE},"rack":[{{"kind":"midi"}},{{"kind":"core"}},{{"kind":"delay"}}]}}}}"#));
    assert_told(&reply, TRACK_GAIN, &format!("sent {GAIN_PAST_RANGE}"), "the gain the agent typed beside a preset and a rack of its own");
}

#[test]
fn a_reply_tells_of_a_gain_kept_differently_beside_a_rack_holding_a_row_preset() {
    let mut app = open("replies_nested_row", |path| path == SEAR);
    let reply = written(&mut app, &format!(r#"{{"path":"{TRACK}","value":{{"gain":{GAIN_PAST_RANGE},"rack":[{{"kind":"midi"}},{{"kind":"core"}},{{"preset":"{SEAR}"}}]}}}}"#));
    assert_told(&reply, TRACK_GAIN, &format!("sent {GAIN_PAST_RANGE}"), "the gain the agent typed on a track whose written rack holds a row preset");
}

#[test]
fn a_reply_tells_of_a_gain_kept_differently_when_a_row_preset_follows_it_in_the_same_call() {
    let mut app = open("replies_gain_then_row", |path| path == SEAR);
    let reply = written(&mut app, &format!(r#"{{"edits":[{{"path":"{TRACK_GAIN}","value":{GAIN_PAST_RANGE}}},{{"path":"{TRACK}/rack/end/preset","value":"{SEAR}"}}]}}"#));
    assert_told(&reply, TRACK_GAIN, &format!("sent {GAIN_PAST_RANGE}"), "the gain the agent typed before loading a row preset on the same track");
}
