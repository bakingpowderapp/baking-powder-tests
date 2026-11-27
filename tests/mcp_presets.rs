use bp::app::App;
use bp::factory::PRESETS;
use bp::json::{self, Json};
use bp::project::Project;
use std::path::Path;
use support::call;

const GROWL: &str = "Instruments/Basses/Growl.json";
const SEAR: &str = "Effects/Freak/Sear.json";

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

fn preset(path: &str) -> Json {
    let (_, text) = PRESETS.iter().find(|(known, _)| *known == path).unwrap_or_else(|| panic!("wanted {path} in the factory"));
    json::parse(text).unwrap_or_else(|| panic!("wanted {path} to parse"))
}

fn kinds(rack: Option<&Json>) -> Vec<String> {
    rack.and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(|row| row.get("kind")).filter_map(Json::as_str).map(String::from).collect()
}

fn under_the_midi_row(rack: Option<&Json>) -> Vec<String> {
    std::iter::once(String::from("midi")).chain(kinds(rack)).collect()
}

fn count(list: Option<&Json>) -> usize {
    list.and_then(Json::as_array).map_or(0, <[Json]>::len)
}

fn entries(listing: &Json) -> &[Json] {
    listing.get("entries").and_then(Json::as_array).unwrap_or_else(|| panic!("wanted entries in {}", listing.write()))
}

#[test]
fn a_track_written_with_a_preset_takes_its_rows_and_lfo_lanes() {
    let mut app = open("preset_whole_sound", |path| path == GROWL);
    let reply = written(&mut app, &format!(r#"{{"path":"tracks/0","value":{{"name":"Bass","kind":"midi","preset":"{GROWL}"}},"details":true}}"#));
    assert!(reply.contains("Presets:") && reply.contains(&format!("tracks/0: {GROWL}, 5 rows")), "wanted the detailed reply to name the preset that landed and its rows, got {reply}");
    let track = answered(&mut app, "read", r#"{"path":"tracks/0"}"#);
    let sound = preset(GROWL);
    assert_eq!(kinds(track.get("rack")), under_the_midi_row(sound.get("rack")), "wanted the track's rack to hold the preset's rows under its own MIDI row, got {}", track.write());
    assert_eq!(count(track.get("lfos")), count(sound.get("lfos")), "wanted the preset's LFO lanes on the track, got {}", track.write());
    assert_eq!(track.get("name").and_then(Json::as_str), Some("Bass"), "wanted the name written beside preset kept");
}

#[test]
fn a_preset_written_at_a_tracks_preset_path_swaps_its_sound_and_keeps_its_clips() {
    let mut app = open("preset_swapped_sound", |path| path == GROWL);
    written(&mut app, r#"{"path":"tracks/0","value":{"name":"Lead","kind":"midi","clips":[{"startBeat":0,"lengthBeats":4,"name":"one","notes":[{"pitch":60,"startBeat":0,"lengthBeats":1}]}]}}"#);
    let reply = written(&mut app, &format!(r#"{{"path":"tracks/0/preset","value":"{GROWL}"}}"#));
    assert!(reply.starts_with("Written at tracks/0 as revision"), "wanted the sound landed on the track at tracks/0, got {reply}");
    let track = answered(&mut app, "read", r#"{"path":"tracks/0","notes":true}"#);
    assert_eq!(track.get("name").and_then(Json::as_str), Some("Lead"), "wanted the track's own name kept, got {}", track.write());
    assert_eq!(count(track.get("clips")), 1, "wanted the track's clip kept, got {}", track.write());
    assert_eq!(kinds(track.get("rack")), under_the_midi_row(preset(GROWL).get("rack")), "wanted the preset's rows in place of the old sound under the track's MIDI row, got {}", track.write());
}

#[test]
fn an_effect_preset_lands_at_a_rack_row_under_the_keys_written_beside_it() {
    let mut app = open("preset_effect_row", |path| path == SEAR);
    let reply = written(&mut app, &format!(r#"{{"path":"tracks/0/rack/2","value":{{"preset":"{SEAR}","wet":0.5}},"details":true}}"#));
    assert!(reply.contains(&format!("tracks/0/rack/2: {SEAR}, a freak row, its wet as written beside preset")), "wanted the reply to name the row that landed and the key set over it, got {reply}");
    let row = answered(&mut app, "read", r#"{"path":"tracks/0/rack/2"}"#);
    assert_eq!(row.get("kind").and_then(Json::as_str), Some("freak"), "wanted a freak row, got {}", row.write());
    assert_eq!(row.get("wet").and_then(Json::as_f64), Some(0.5), "wanted the wet written beside preset, got {}", row.write());
    assert_eq!(row.get("driveDb").and_then(Json::as_f64), preset(SEAR).get("driveDb").and_then(Json::as_f64), "wanted the preset's own drive under it, got {}", row.write());
}

#[test]
fn a_whole_sound_is_refused_at_a_rack_row_naming_where_it_lands() {
    let mut app = open("preset_refused_sound", |path| path == GROWL);
    let reply = call(&mut app, "write", &format!(r#"{{"path":"tracks/0/rack/2","value":{{"preset":"{GROWL}"}}}}"#));
    assert!(reply.failed && reply.text.contains("lands on a track"), "wanted a whole sound refused at a row with where it lands, got {}", reply.text);
}

#[test]
fn a_listing_names_each_presets_rows_and_lanes_and_takes_a_path_from_a_folders_name() {
    let mut app = open("preset_listing", |_| true);
    let listing = answered(&mut app, "library", r#"{"path":"Instruments/Basses"}"#);
    let growl = entries(&listing).iter().find(|entry| entry.get("path").and_then(Json::as_str) == Some("Growl.json")).unwrap_or_else(|| panic!("wanted Growl listed, got {}", listing.write()));
    assert_eq!(growl.get("rows").and_then(Json::as_str), Some(kinds(preset(GROWL).get("rack")).join(", ").as_str()), "wanted Growl's rows named, got {}", growl.write());
    assert_eq!(growl.get("lfos").and_then(Json::as_f64), Some(count(preset(GROWL).get("lfos")) as f64), "wanted Growl's LFO lanes counted, got {}", growl.write());
    assert!(growl.get("rack").is_none(), "wanted no folded rack line in a listing, got {}", growl.write());
    let found = answered(&mut app, "library", r#"{"find":"growl"}"#);
    assert!(entries(&found).iter().any(|entry| entry.get("path").and_then(Json::as_str) == Some(GROWL)), "wanted find to give Growl's path from its folder's name, got {}", found.write());
}

#[test]
fn a_row_preset_lands_once_on_a_rack_written_earlier_in_the_same_call() {
    let asked = [("preset_once_at_its_path", format!(r#"{{"path":"tracks/0/rack/0/preset","value":"{SEAR}"}}"#)), ("preset_once_as_a_value", format!(r#"{{"path":"tracks/0/rack/0","value":{{"preset":"{SEAR}"}}}}"#))];
    for (name, load) in asked {
        let mut app = open(name, |path| path == SEAR);
        written(&mut app, &format!(r#"{{"edits":[{{"path":"tracks/0/rack","value":[{{"kind":"delay"}}]}},{load}]}}"#));
        let rack = answered(&mut app, "read", r#"{"path":"tracks/0/rack"}"#);
        assert_eq!(kinds(Some(&rack)), ["midi", "core", "freak"], "wanted the preset's row once, in place of the delay written before it in the same call, by {load}, got {}", rack.write());
    }
}

fn pitch_lines(reply: &str) -> Vec<&str> {
    reply.lines().skip_while(|line| *line != "Preset pitch:").skip(1).take_while(|line| line.starts_with(' ')).map(str::trim).collect()
}

#[test]
fn a_presets_pitch_offsets_are_told_on_the_rows_it_brought_and_never_on_a_core_that_was_there() {
    let mut app = open("preset_pitch_rows", |path| path == GROWL);
    let effects = Path::new(env!("CARGO_TARGET_TMPDIR")).join("preset_pitch_rows").join("Factory").join("Effects");
    std::fs::create_dir_all(&effects).unwrap_or_else(|error| panic!("wanted {}: {error}", effects.display()));
    std::fs::write(effects.join("Pair.json"), r#"{"kind": "rack", "description": "Two effects", "rack": [{"kind": "delay"}, {"kind": "reverb"}]}"#).expect("wanted the effects preset written");
    let reply = written(&mut app, &format!(r#"{{"path":"tracks/0/preset","value":"{GROWL}"}}"#));
    let told = pitch_lines(&reply);
    assert!(matches!(told.as_slice(), [line] if line.starts_with(&format!("tracks/0/rack/1: {GROWL}: "))), "wanted the sound's tuned Core told once, at the row it landed on, got {reply}");
    let reply = written(&mut app, r#"{"path":"tracks/0/preset","value":"Effects/Pair.json"}"#);
    let rack = answered(&mut app, "read", r#"{"path":"tracks/0/rack"}"#);
    assert!(kinds(Some(&rack)).ends_with(&[String::from("delay"), String::from("reverb")]) && kinds(Some(&rack)).contains(&String::from("core")), "wanted the two effects joined under the sound that was there, got {}", rack.write());
    assert!(pitch_lines(&reply).is_empty() && !reply.contains("Preset pitch"), "wanted the effects preset credited with no tuning, the tuned Core having been there before it, got {reply}");
}
