use bp::app::App;
use bp::json::{self, Json};
use bp::project::Project;
use support::{CORE_ROW, call};

fn open() -> App {
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false}}", Project::new().to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.take_requests();
    app
}

fn read(app: &mut App, path: &str) -> Json {
    let reply = call(app, "read", &format!(r#"{{"path":"{path}","notes":true}}"#));
    assert!(!reply.failed, "wanted {path} read, got {}", reply.text);
    json::parse(&reply.text).unwrap_or_else(|| panic!("wanted {path} read as JSON, got {}", reply.text))
}

#[test]
fn a_clip_written_at_the_first_index_of_a_track_without_clips_lands() {
    let mut app = open();
    assert!(read(&mut app, "tracks/0").get("clips").is_none(), "wanted a fresh track to leave its empty clips out");
    let reply = call(&mut app, "write", r#"{"path":"tracks/0/clips/0","value":{"startBeat":0,"lengthBeats":4,"name":"one","notes":[{"pitch":60,"startBeat":0,"lengthBeats":1}]}}"#);
    assert!(!reply.failed, "wanted the first clip appended to a track with none, got {}", reply.text);
    let clips = read(&mut app, "tracks/0/clips");
    assert_eq!(clips.as_array().map(<[Json]>::len), Some(1), "wanted the one clip there, got {}", clips.write());
}

#[test]
fn a_knob_written_inside_a_section_left_at_its_defaults_lands() {
    let mut app = open();
    assert!(read(&mut app, &format!("tracks/0/rack/{CORE_ROW}")).get("mono").is_none(), "wanted a fresh Core to leave mono out at its defaults");
    let reply = call(&mut app, "write", &format!(r#"{{"path":"tracks/0/rack/{CORE_ROW}/mono/glideMs","value":120}}"#));
    assert!(!reply.failed, "wanted the glide written into a section left at its defaults, got {}", reply.text);
    let glide = read(&mut app, &format!("tracks/0/rack/{CORE_ROW}/mono/glideMs"));
    assert_eq!(glide.as_f64(), Some(120.0), "wanted the glide kept, got {}", glide.write());
}

#[test]
fn a_key_the_manual_does_not_know_is_still_refused_naming_the_keys_there() {
    let mut app = open();
    let reply = call(&mut app, "write", r#"{"path":"tracks/0/clipz/0","value":{"startBeat":0,"lengthBeats":4}}"#);
    assert!(reply.failed && reply.text.contains("has no clipz") && reply.text.contains("its keys are"), "wanted a mistyped key refused with the keys that are there, got {}", reply.text);
}

fn absent(app: &mut App, path: &str) -> Json {
    let found = read(app, path);
    assert_eq!(found.get("stored").and_then(Json::as_bool), Some(false), "wanted {path} told as not written, got {}", found.write());
    found
}

fn kept_differently(reply: &str) -> Vec<&str> {
    reply.lines().skip_while(|line| !line.starts_with("Kept differently")).skip(1).take_while(|line| line.starts_with(' ')).map(str::trim).collect()
}

#[test]
fn an_absent_key_reads_as_its_own_default_empty_text_as_empty_and_a_list_as_no_one_value() {
    let mut app = open();
    assert_eq!(absent(&mut app, "tracks/0/brief").get("default"), Some(&Json::String(String::new())), "wanted an unwritten brief to read as empty text");
    assert_eq!(absent(&mut app, "tracks/0/mute").get("default"), Some(&Json::Bool(false)), "wanted an unwritten mute to read as false");
    assert_eq!(absent(&mut app, "tracks/0/rack/0/velocity").get("default"), Some(&Json::Number(100.0)), "wanted the MIDI row's unwritten velocity to read as 100");
    for list in ["levels", "shapes"] {
        let found = absent(&mut app, &format!("tracks/0/rack/{CORE_ROW}/osc1/{list}"));
        assert!(found.get("default").is_none() && found.get("about").and_then(Json::as_str).is_some(), "wanted an unwritten list to give its line and no single default, got {}", found.write());
    }
    let reply = call(&mut app, "write", r#"{"path":"tracks/0/brief","value":""}"#);
    assert!(!reply.failed && kept_differently(&reply.text).is_empty(), "wanted empty text written where empty is the default answered as kept, got {}", reply.text);
}

#[test]
fn a_slope_is_text_and_a_number_sent_to_one_is_answered_with_the_text_to_send() {
    let mut app = open();
    let row = format!("tracks/0/rack/{}", CORE_ROW + 1);
    let added = call(&mut app, "write", &format!(r#"{{"path":"{row}","value":{{"kind":"freak"}}}}"#));
    assert!(!added.failed, "wanted a freak row added, got {}", added.text);
    let slope = format!("{row}/crossoverSlope");
    assert_eq!(absent(&mut app, &slope).get("default"), Some(&Json::String(String::from("24"))), "wanted the unwritten slope's default as text");
    let reply = call(&mut app, "write", &format!(r#"{{"path":"{slope}","value":12}}"#));
    assert_eq!(kept_differently(&reply.text), [format!(r#"{slope}: sent 12, kept the default 24; it is text, so send "12""#)], "wanted a number sent to a slope answered with the text to send, got {}", reply.text);
    let reply = call(&mut app, "write", &format!(r#"{{"path":"{slope}","value":"12"}}"#));
    assert!(!reply.failed && kept_differently(&reply.text).is_empty(), "wanted the slope sent as text kept, got {}", reply.text);
    assert_eq!(read(&mut app, &slope).as_str(), Some("12"), "wanted the slope read back as the text written");
}

#[test]
fn a_find_that_matches_nothing_points_at_row_for_a_rack_rows_keys() {
    let mut app = open();
    let whole = call(&mut app, "manual", r#"{"find":"zzqq"}"#);
    assert!(whole.failed && whole.text.contains("a rack row's keys are searched with row or a row's path"), "wanted a find with no match to say how a rack row's keys are searched, got {}", whole.text);
    let row = call(&mut app, "manual", r#"{"row":"delay","find":"zzqq"}"#);
    assert!(row.failed && !row.text.contains("searched with row"), "wanted a find within a row to leave that hint out, got {}", row.text);
    let found = call(&mut app, "manual", r#"{"row":"delay","find":"feedback"}"#);
    assert!(!found.failed && found.text.contains("rightFeedback"), "wanted a row's keys found with row, got {}", found.text);
}
