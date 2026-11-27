use bp::app::App;
use bp::factory::PRESETS;
use bp::json::{self, Json};
use bp::project::Project;
use std::path::Path;
use support::call;

const GROWL: &str = "Instruments/Basses/Growl.json";
const CROSS: &str = "Effects/Delay/Cross.json";
const POLISH: &str = "Effects/Racks/Polish.json";
const KEPT_DIFFERENTLY: &str = "Kept differently";
const TRACK: &str = "tracks/0";
const AUDIO_TRACK: &str = "tracks/1";
const TRACK_GAIN: &str = "tracks/0/gain";
const DELAY: &str = "delay";
const KNOB: &str = "feedback";
const TYPED_BARE_AT: usize = 1;
const SOUND_LANE: &str = "osc1.fmAmount";
const ROW_LANE: &str = "wetPan";
const CURVED: &str = "wet";
const GAIN: &str = "gain";
const PAST_RANGE: f64 = 5.0;
const PAST_RANGE_AGAIN: f64 = 9.0;
const AT_ITS_LIMIT: f64 = 1.0;
const IN_RANGE: f64 = 0.5;
const OTHER_LANES_AMOUNT: f64 = 0.25;
const GAIN_WRITTEN: f64 = 1.0;
const CENTRE_PAST_RANGE: f64 = 7.0;
const RATE_PAST_RANGE: f64 = 900.0;
const NESTED: &str = "filter2/cutoffHz";
const NESTED_PAST_RANGE: f64 = 99_999.0;
const UNKNOWN: &str = "bogus";
const DOES_NOTHING: &str = "not a key here, so it does nothing";
const BESIDE: [(&str, &str, &str, Call); 9] = [
    ("rack", "a knob in a rack written beside a sound", TRACK, rack_beside_sound),
    ("bare_rack", "a knob in a rack written beside a sound without its MIDI row", TRACK, bare_rack_beside_sound),
    ("audio_rack", "a knob in a rack written beside an effects sound on an audio track", AUDIO_TRACK, rack_beside_effects),
    ("lane", "an LFO lane written beside a sound", TRACK, lane_beside_sound),
    ("gain_lane", "a gain lane typed ahead of a row's lane beside a sound", TRACK, gain_lane_ahead),
    ("curve", "a curve node and a gain lane beside a sound and a rack", TRACK, curve_and_gain_lane),
    ("row_end", "an LFO lane beside a row preset added at the end", TRACK, lane_beside_row_at_the_end),
    ("row_swapped", "an LFO lane beside a row preset swapped over a delay written first", TRACK, lane_beside_row_swapped_in),
    ("row_nested", "an LFO lane beside a row preset nested in a written rack", TRACK, lane_beside_row_in_a_rack),
];

type Call = fn(f64) -> String;

fn open(name: &str) -> App {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("Factory");
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in PRESETS.into_iter().filter(|(path, _)| [GROWL, CROSS, POLISH].contains(path)) {
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

fn written(app: &mut App, args: &str) -> String {
    let reply = call(app, "write", args);
    assert!(!reply.failed, "wanted the write taken: {args}, got {}", reply.text);
    reply.text
}

fn read(app: &mut App, path: &str) -> Json {
    let reply = call(app, "read", &format!(r#"{{"path":"{path}"}}"#));
    assert!(!reply.failed, "wanted {path} read back, got {}", reply.text);
    json::parse(&reply.text).unwrap_or_else(|| panic!("wanted {path} read back as JSON, got {}", reply.text))
}

fn list<'a>(owner: &'a Json, key: &str) -> &'a [Json] {
    owner.get(key).and_then(Json::as_array).unwrap_or(&[])
}

fn rack_beside_sound(feedback: f64) -> String {
    format!(r#"{{"path":"{TRACK}","value":{{"preset":"{GROWL}","rack":[{{"kind":"midi"}},{{"kind":"core"}},{{"kind":"{DELAY}","{KNOB}":{feedback}}}]}}}}"#)
}

fn bare_rack_beside_sound(feedback: f64) -> String {
    format!(r#"{{"path":"{TRACK}","value":{{"preset":"{GROWL}","rack":[{{"kind":"core"}},{{"kind":"{DELAY}","{KNOB}":{feedback}}}]}}}}"#)
}

fn rack_beside_effects(feedback: f64) -> String {
    format!(r#"{{"path":"{AUDIO_TRACK}","value":{{"kind":"audio","preset":"{POLISH}","rack":[{{"kind":"{DELAY}","{KNOB}":{feedback}}}]}}}}"#)
}

fn lane_beside_sound(amount: f64) -> String {
    format!(r#"{{"path":"{TRACK}","value":{{"preset":"{GROWL}","lfos":[{{"row":0,"param":"{SOUND_LANE}","amount":{amount}}}]}}}}"#)
}

fn gain_lane_ahead(amount: f64) -> String {
    format!(r#"{{"path":"{TRACK}","value":{{"preset":"{GROWL}","lfos":[{{"param":"{GAIN}","amount":{amount}}},{{"row":0,"param":"{SOUND_LANE}","amount":{OTHER_LANES_AMOUNT}}}]}}}}"#)
}

fn curve_and_gain_lane(value: f64) -> String {
    let (lanes, curves) = (format!(r#"[{{"param":"{GAIN}","amount":{value}}}]"#), format!(r#"[{{"row":2,"param":"{CURVED}","nodes":[{{"beat":0,"value":{value}}}]}}]"#));
    format!(r#"{{"path":"{TRACK}","value":{{"preset":"{GROWL}","rack":[{{"kind":"midi"}},{{"kind":"core"}},{{"kind":"{DELAY}"}}],"lfos":{lanes},"curves":{curves}}}}}"#)
}

fn row_with_a_lane(amount: f64) -> String {
    format!(r#"{{"preset":"{CROSS}","lfos":[{{"row":0,"param":"{ROW_LANE}","amount":{amount}}}]}}"#)
}

fn lane_beside_row_at_the_end(amount: f64) -> String {
    format!(r#"{{"path":"{TRACK}/rack/2","value":{}}}"#, row_with_a_lane(amount))
}

fn lane_beside_row_swapped_in(amount: f64) -> String {
    format!(r#"{{"edits":[{{"path":"{TRACK}/rack/end","value":{{"kind":"{DELAY}"}}}},{{"path":"{TRACK}/rack/2","value":{}}}]}}"#, row_with_a_lane(amount))
}

fn lane_beside_row_in_a_rack(amount: f64) -> String {
    format!(r#"{{"path":"{TRACK}","value":{{"rack":[{{"kind":"midi"}},{{"kind":"core"}},{}]}}}}"#, row_with_a_lane(amount))
}

fn lines(reply: &str) -> Vec<&str> {
    reply.lines().skip_while(|line| !line.starts_with(KEPT_DIFFERENTLY)).skip(1).take_while(|line| line.starts_with(' ')).map(str::trim).collect()
}

fn told<'a>(reply: &'a str, path: &str) -> Vec<&'a str> {
    lines(reply).into_iter().filter_map(|line| line.strip_prefix(path)).filter_map(|rest| rest.strip_prefix(": ")).collect()
}

fn row_of(app: &mut App, host: &str, kind: &str) -> usize {
    let rack = read(app, &format!("{host}/rack"));
    let rows = rack.as_array().unwrap_or(&[]);
    rows.iter().position(|row| row.get("kind").and_then(Json::as_str) == Some(kind)).unwrap_or_else(|| panic!("wanted a {kind} row on {host}, got {}", rack.write()))
}

fn lane_of(app: &mut App, host: &str, row: bool, param: &str) -> String {
    let owner = read(app, host);
    let found = list(&owner, "lfos").iter().position(|lane| lane.get("row").is_some() == row && lane.get("param").and_then(Json::as_str) == Some(param));
    format!("{host}/lfos/{}", found.unwrap_or_else(|| panic!("wanted a lane on {param} on {host}, got {}", owner.write())))
}

fn assert_told(app: &mut App, reply: &str, path: &str, sent: f64, what: &str) {
    let kept = read(app, path).as_f64().unwrap_or_else(|| panic!("wanted a number kept at {path}, {what}"));
    assert!(kept != sent, "wanted {path} kept within its range, {what}, got it kept as sent, {kept}");
    let (told, wanted) = (told(reply, path), format!("sent {sent}, kept {kept}"));
    assert_eq!(told, vec![wanted.as_str()], "wanted the reply to say {wanted} of {path} once, {what}, got {told:?} in: {reply}");
}

#[test]
fn a_knob_past_its_range_in_a_rack_written_beside_a_sound_is_told_as_sent_and_as_kept() {
    let mut app = open("kept_lines_rack");
    let reply = written(&mut app, &rack_beside_sound(PAST_RANGE));
    let row = row_of(&mut app, TRACK, DELAY);
    assert_told(&mut app, &reply, &format!("{TRACK}/rack/{row}/{KNOB}"), PAST_RANGE, "the delay's feedback typed past its range in a rack beside the sound");
}

#[test]
fn a_knob_past_its_range_is_told_at_the_row_it_landed_on_when_the_rack_beside_a_sound_leaves_out_the_midi_row() {
    let mut app = open("kept_lines_bare_rack");
    let reply = written(&mut app, &bare_rack_beside_sound(PAST_RANGE));
    let row = row_of(&mut app, TRACK, DELAY);
    assert!(row != TYPED_BARE_AT, "wanted the delay typed at row {TYPED_BARE_AT} moved down under the MIDI row the track keeps, got it at row {row}");
    assert_told(&mut app, &reply, &format!("{TRACK}/rack/{row}/{KNOB}"), PAST_RANGE, "the delay's feedback typed past its range on a row the app moved down");
}

#[test]
fn a_knob_past_its_range_in_a_rack_written_beside_an_effects_sound_on_an_audio_track_is_told() {
    let mut app = open("kept_lines_audio_rack");
    let reply = written(&mut app, &rack_beside_effects(PAST_RANGE));
    let row = row_of(&mut app, AUDIO_TRACK, DELAY);
    assert_told(&mut app, &reply, &format!("{AUDIO_TRACK}/rack/{row}/{KNOB}"), PAST_RANGE, "the delay's feedback typed past its range in a rack beside an effects sound");
}

#[test]
fn a_nested_key_past_its_range_and_a_key_the_row_lacks_in_a_rack_beside_a_sound_are_each_told() {
    let mut app = open("kept_lines_nested_key");
    let delay = format!(r#"{{"kind":"{DELAY}","filter2":{{"cutoffHz":{NESTED_PAST_RANGE}}},"{UNKNOWN}":1}}"#);
    let reply = written(&mut app, &format!(r#"{{"path":"{TRACK}","value":{{"preset":"{GROWL}","rack":[{{"kind":"midi"}},{{"kind":"core"}},{delay}]}}}}"#));
    let row = format!("{TRACK}/rack/{}", row_of(&mut app, TRACK, DELAY));
    assert_told(&mut app, &reply, &format!("{row}/{NESTED}"), NESTED_PAST_RANGE, "a filter's cutoff typed past its range on a row of a rack beside the sound");
    let unknown = told(&reply, &format!("{row}/{UNKNOWN}"));
    assert_eq!(unknown, vec![DOES_NOTHING], "wanted the reply to say the key the delay does not have does nothing, got {unknown:?} in: {reply}");
}

#[test]
fn an_lfo_amount_past_its_range_written_beside_a_sound_is_told() {
    let mut app = open("kept_lines_lane");
    let reply = written(&mut app, &lane_beside_sound(PAST_RANGE));
    let lane = lane_of(&mut app, TRACK, true, SOUND_LANE);
    assert_told(&mut app, &reply, &format!("{lane}/amount"), PAST_RANGE, "the LFO amount typed past its range beside the sound");
}

#[test]
fn an_lfo_centre_and_rate_past_range_written_beside_a_sound_are_each_told() {
    let mut app = open("kept_lines_centre_rate");
    let typed = format!(r#"{{"row":0,"param":"{SOUND_LANE}","amount":{IN_RANGE},"center":{CENTRE_PAST_RANGE},"tempoSync":false,"rateHz":{RATE_PAST_RANGE}}}"#);
    let reply = written(&mut app, &format!(r#"{{"path":"{TRACK}","value":{{"preset":"{GROWL}","lfos":[{typed}]}}}}"#));
    let lane = lane_of(&mut app, TRACK, true, SOUND_LANE);
    assert_told(&mut app, &reply, &format!("{lane}/center"), CENTRE_PAST_RANGE, "the LFO centre typed past its knob's range beside the sound");
    assert_told(&mut app, &reply, &format!("{lane}/rateHz"), RATE_PAST_RANGE, "the LFO rate typed past its range beside the sound");
}

#[test]
fn an_lfo_amount_past_its_range_written_beside_a_row_preset_is_told_at_the_tracks_lane() {
    let landings: [(&str, &str, Call); 3] = [
        ("kept_lines_row_end", "added at the end of the rack", lane_beside_row_at_the_end),
        ("kept_lines_row_swapped", "swapped over a delay written first in the same call", lane_beside_row_swapped_in),
        ("kept_lines_row_nested", "nested in a written rack", lane_beside_row_in_a_rack),
    ];
    for (name, how, write) in landings {
        let mut app = open(name);
        let reply = written(&mut app, &write(PAST_RANGE));
        let lane = lane_of(&mut app, TRACK, true, ROW_LANE);
        assert_told(&mut app, &reply, &format!("{lane}/amount"), PAST_RANGE, &format!("the LFO amount typed past its range beside a row preset {how}"));
    }
}

#[test]
fn a_gain_lane_typed_ahead_of_a_rows_lane_beside_a_sound_is_told_with_its_own_amount() {
    let mut app = open("kept_lines_gain_lane");
    let reply = written(&mut app, &gain_lane_ahead(PAST_RANGE));
    let (gain, other) = (lane_of(&mut app, TRACK, false, GAIN), lane_of(&mut app, TRACK, true, SOUND_LANE));
    assert_told(&mut app, &reply, &format!("{gain}/amount"), PAST_RANGE, &format!("the gain lane's own amount, not the {OTHER_LANES_AMOUNT} typed on the lane after it"));
    let unsaid = told(&reply, &format!("{other}/amount"));
    assert!(unsaid.is_empty(), "wanted no line of the row's lane, its amount typed in range, got {unsaid:?} in: {reply}");
}

#[test]
fn a_curve_node_and_a_gain_lane_past_range_beside_a_sound_and_a_rack_are_each_told() {
    let mut app = open("kept_lines_curve");
    let reply = written(&mut app, &curve_and_gain_lane(PAST_RANGE));
    let gain = lane_of(&mut app, TRACK, false, GAIN);
    assert_told(&mut app, &reply, &format!("{TRACK}/curves/0/nodes/0/value"), PAST_RANGE, "the curve node typed past its knob's range beside the sound and a rack");
    assert_told(&mut app, &reply, &format!("{gain}/amount"), PAST_RANGE, "the gain lane's amount typed past its range beside the sound and a rack");
}

#[test]
fn a_line_two_edits_of_one_call_both_give_is_printed_once() {
    let track_then_gain = |beside: &str| format!(r#"{{"edits":[{{"path":"{TRACK}","value":{{{beside}"{GAIN}":{GAIN_WRITTEN}}}}},{{"path":"{TRACK_GAIN}","value":{PAST_RANGE}}}]}}"#);
    let rack_then_knob = format!(r#"{{"edits":[{},{{"path":"{TRACK}/rack/2/{KNOB}","value":{PAST_RANGE_AGAIN}}}]}}"#, rack_beside_sound(PAST_RANGE));
    let given_twice = [
        ("kept_lines_once_plain", "a gain typed past its range after its track was written with no preset", track_then_gain(""), String::from(TRACK_GAIN), PAST_RANGE),
        ("kept_lines_once_sound", "a gain typed past its range after its track was written with a preset", track_then_gain(&format!(r#""preset":"{GROWL}","#)), String::from(TRACK_GAIN), PAST_RANGE),
        ("kept_lines_once_knob", "a knob typed past its range again after its rack was written beside a sound", rack_then_knob, format!("{TRACK}/rack/2/{KNOB}"), PAST_RANGE_AGAIN),
    ];
    for (name, what, edits, path, sent) in given_twice {
        let mut app = open(name);
        let reply = written(&mut app, &edits);
        assert_told(&mut app, &reply, &path, sent, what);
    }
}

#[test]
fn a_value_in_range_written_beside_a_preset_shows_no_kept_differently_line() {
    for (name, what, _, write) in BESIDE {
        let mut app = open(&format!("kept_lines_in_range_{name}"));
        let reply = written(&mut app, &write(IN_RANGE));
        let lines = lines(&reply);
        assert!(lines.is_empty(), "wanted no line of {what}, its value typed in range, got {lines:?} in: {reply}");
    }
    let mut app = open("kept_lines_in_range_later");
    let reply = written(&mut app, &format!(r#"{{"edits":[{},{{"path":"{TRACK}/rack/2/{KNOB}","value":{IN_RANGE}}}]}}"#, rack_beside_sound(PAST_RANGE)));
    let lines = lines(&reply);
    assert!(lines.is_empty(), "wanted no line of a knob typed past its range and then in range by a later edit of the same call, got {lines:?} in: {reply}");
}

#[test]
fn a_value_past_its_range_written_beside_a_preset_leaves_the_song_its_limit_leaves() {
    for (name, what, host, write) in BESIDE {
        let (mut past, mut limit) = (open(&format!("kept_lines_song_past_{name}")), open(&format!("kept_lines_song_limit_{name}")));
        written(&mut past, &write(PAST_RANGE));
        written(&mut limit, &write(AT_ITS_LIMIT));
        let (kept, wanted) = (read(&mut past, host), read(&mut limit, host));
        assert!(kept == wanted, "wanted {what} typed at {PAST_RANGE} to leave the song {AT_ITS_LIMIT} leaves, got {} against {}", kept.write(), wanted.write());
    }
}
