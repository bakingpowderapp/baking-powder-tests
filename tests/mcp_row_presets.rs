use bp::app::App;
use bp::factory::PRESETS;
use bp::fx::Kind;
use bp::json::{self, Json};
use bp::project::Project;
use std::path::Path;
use support::{call, effect};

const CROSS: &str = "Effects/Delay/Cross.json";
const SEAR: &str = "Effects/Freak/Sear.json";
const TRACK: &str = "tracks/0";
const MASTER: &str = "master";
const DELAY_ROW: usize = 2;
const MASTER_ROW: usize = 0;
const DELAY: &str = "delay";
const FREAK: &str = "freak";
const BOTH_HAVE: &str = "dry";
const BOTH_NAME_ALIKE: &str = "filter1.q";
const BOTH_NAME_ALIKE_TOO: &str = "filter2.q";
const DELAY_ONLY_CURVE: &str = "feedback";
const DELAY_ONLY_LANE: &str = "wetPan";
const NODES: [(f64, f64); 2] = [(0.0, 0.25), (4.0, 0.75)];
const LANE_AMOUNT: f64 = 0.3;
const CURVE: &str = "curve";
const LANE: &str = "LFO lane";
const LEFT_ALIKE: [&str; 2] = ["rack", "curves"];
const A_CURVE_BOTH_HAVE: Held = Held { curves: &[BOTH_HAVE], lanes: &[] };
const ONLY_THE_DELAYS: Held = Held { curves: &[DELAY_ONLY_CURVE], lanes: &[] };
const ONLY_THE_DELAYS_WITH_A_LANE: Held = Held { lanes: &[DELAY_ONLY_LANE], ..ONLY_THE_DELAYS };
const A_CURVE_AMONG_OTHERS: Held = Held { curves: &[BOTH_HAVE, DELAY_ONLY_CURVE], lanes: &[DELAY_ONLY_LANE, BOTH_NAME_ALIKE_TOO] };
const A_CURVE_NAMED_ALIKE_AMONG_OTHERS: Held = Held { curves: &[DELAY_ONLY_CURVE, BOTH_NAME_ALIKE], lanes: &[DELAY_ONLY_LANE, BOTH_NAME_ALIKE_TOO] };

type Aim = (Option<usize>, Option<String>);

struct Held {
    curves: &'static [&'static str],
    lanes: &'static [&'static str],
}

fn open(name: &str) -> App {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("Factory");
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in PRESETS.into_iter().filter(|(path, _)| [CROSS, SEAR].contains(path)) {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().expect("wanted a preset's folder")).unwrap_or_else(|error| panic!("wanted {}: {error}", file.display()));
        std::fs::write(&file, text).unwrap_or_else(|error| panic!("wanted {} written: {error}", file.display()));
    }
    let mut project = Project::new();
    project.tracks[0].rack.push(effect(Kind::Delay));
    project.master.rack = vec![effect(Kind::Delay)];
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false}}", project.to_json().write());
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

fn preset(path: &str) -> Json {
    let (_, text) = PRESETS.iter().find(|(known, _)| *known == path).unwrap_or_else(|| panic!("wanted {path} in the factory"));
    json::parse(text).unwrap_or_else(|| panic!("wanted {path} to parse"))
}

fn list<'a>(owner: &'a Json, key: &str) -> &'a [Json] {
    owner.get(key).and_then(Json::as_array).unwrap_or(&[])
}

fn shown(part: Option<&Json>) -> String {
    part.map_or_else(|| String::from("nothing"), Json::write)
}

fn aim(item: &Json) -> Aim {
    (item.get("row").and_then(Json::as_f64).map(|row| row as usize), item.get("param").and_then(Json::as_str).map(String::from))
}

fn aimed(row: usize, param: &str) -> Aim {
    (Some(row), Some(String::from(param)))
}

fn aims(row: usize, params: &[&str]) -> Vec<Aim> {
    params.iter().map(|param| aimed(row, param)).collect()
}

fn nodes(curve: &Json) -> Vec<(Option<f64>, Option<f64>)> {
    list(curve, "nodes").iter().map(|node| (node.get("beat").and_then(Json::as_f64), node.get("value").and_then(Json::as_f64))).collect()
}

fn as_written() -> Vec<(Option<f64>, Option<f64>)> {
    NODES.iter().map(|(beat, value)| (Some(*beat), Some(*value))).collect()
}

fn curves(owner: &Json) -> Vec<Aim> {
    list(owner, "curves").iter().map(aim).collect()
}

fn lanes(owner: &Json) -> Vec<Aim> {
    list(owner, "lfos").iter().map(aim).collect()
}

fn kind_at(owner: &Json, row: usize) -> Option<&str> {
    list(owner, "rack").get(row).and_then(|row| row.get("kind")).and_then(Json::as_str)
}

fn own_lanes(path: &str, row: usize) -> Vec<Aim> {
    let mut own: Vec<Aim> = list(&preset(path), "lfos").iter().filter_map(|lane| lane.get("param").and_then(Json::as_str)).map(|param| aimed(row, param)).collect();
    own.sort();
    own
}

fn arranged(name: &str, host: &str, row: usize, held: &Held) -> App {
    let mut app = open(name);
    let [(first_beat, first_value), (last_beat, last_value)] = NODES;
    let curve = |param: &&str| format!(r#"{{"row":{row},"param":"{param}","nodes":[{{"beat":{first_beat},"value":{first_value}}},{{"beat":{last_beat},"value":{last_value}}}]}}"#);
    let lane = |param: &&str| format!(r#"{{"row":{row},"param":"{param}","amount":{LANE_AMOUNT},"tempoSync":true,"division":"1/4"}}"#);
    let joined = |items: Vec<String>| items.join(",");
    let lists = [("curves", joined(held.curves.iter().map(curve).collect())), ("lfos", joined(held.lanes.iter().map(lane).collect()))];
    let edits: Vec<String> = lists.iter().filter(|(_, items)| !items.is_empty()).map(|(key, items)| format!(r#"{{"path":"{host}/{key}","value":[{items}]}}"#)).collect();
    written(&mut app, &format!(r#"{{"edits":[{}]}}"#, edits.join(",")));
    let owner = read(&mut app, host);
    let landed = (curves(&owner), lanes(&owner));
    assert_eq!(landed, (aims(row, held.curves), aims(row, held.lanes)), "wanted the delay row's curves and LFO lanes arranged before the load, got {}", owner.write());
    app
}

fn loads(host: &str, row: usize, preset: &str) -> [(&'static str, String); 2] {
    let at_its_path = format!(r#"{{"path":"{host}/rack/{row}/preset","value":"{preset}","details":true}}"#);
    let as_a_value = format!(r#"{{"path":"{host}/rack/{row}","value":{{"preset":"{preset}"}},"details":true}}"#);
    [("at the row's preset path", at_its_path), ("as the row's value", as_a_value)]
}

fn in_its_rack(app: &mut App, host: &str, row: usize, preset: &str) -> String {
    let mut rack = read(app, &format!("{host}/rack")).as_array().map(<[Json]>::to_vec).unwrap_or_default();
    rack[row] = json::parse(&format!(r#"{{"preset":"{preset}"}}"#)).expect("wanted the preset's row to parse");
    format!(r#"{{"path":"{host}/rack","value":{},"details":true}}"#, Json::Array(rack).write())
}

fn dropped(reply: &str, at: &str, noun: &str) -> Option<usize> {
    let mut loaded = reply.lines().skip_while(|line| !line.starts_with("Presets:")).skip(1).take_while(|line| line.starts_with(' ')).map(str::trim);
    let line = loaded.find(|line| line.starts_with(&format!("{at}: ")))?;
    line.split("; ").find_map(|part| part.strip_suffix(&format!(" {noun} dropped, no such knob")))?.parse().ok()
}

fn assert_curve_kept(name: &str, host: &str, row: usize) {
    for (index, (how, load)) in loads(host, row, SEAR).into_iter().enumerate() {
        let mut app = arranged(&format!("{name}_{index}"), host, row, &A_CURVE_BOTH_HAVE);
        written(&mut app, &load);
        let owner = read(&mut app, host);
        assert_eq!(kind_at(&owner, row), Some(FREAK), "wanted the freak preset in the delay's row of {host}, loaded {how}, got {}", owner.write());
        let kept: Vec<_> = list(&owner, "curves").iter().map(|curve| (aim(curve), nodes(curve))).collect();
        assert_eq!(kept, vec![(aimed(row, BOTH_HAVE), as_written())], "wanted the delay's curve on {BOTH_HAVE} kept on the freak row of {host} with its nodes, loaded {how}, got {kept:?}");
    }
}

#[test]
fn a_freak_preset_loaded_onto_a_delay_row_keeps_the_rows_curve_on_dry_with_its_nodes() {
    assert_curve_kept("row_presets_curve", TRACK, DELAY_ROW);
}

#[test]
fn a_freak_preset_loaded_onto_a_delay_row_of_the_master_keeps_the_rows_curve_on_dry() {
    assert_curve_kept("row_presets_master_curve", MASTER, MASTER_ROW);
}

#[test]
fn a_freak_preset_loaded_onto_a_delay_row_drops_what_the_freak_has_no_knob_for_and_counts_it() {
    let row = format!("{TRACK}/rack/{DELAY_ROW}");
    for (index, (how, load)) in loads(TRACK, DELAY_ROW, SEAR).into_iter().enumerate() {
        let mut app = arranged(&format!("row_presets_dropped_{index}"), TRACK, DELAY_ROW, &ONLY_THE_DELAYS);
        let reply = written(&mut app, &load);
        let track = read(&mut app, TRACK);
        let left = curves(&track);
        assert_eq!(left, Vec::new(), "wanted the curve on {DELAY_ONLY_CURVE} dropped, the freak having no such knob, loaded {how}, got {}", track.write());
        let count = dropped(&reply, &row, CURVE);
        assert_eq!(count, Some(1), "wanted the preset's line at {row} to count the 1 {CURVE} dropped, no such knob, loaded {how}, got {count:?} in: {reply}");
    }
}

#[test]
fn a_freak_preset_loaded_onto_a_delay_row_leaves_what_the_same_preset_in_a_written_rack_leaves() {
    for (held, what, knob) in [(&A_CURVE_AMONG_OTHERS, "both", BOTH_HAVE), (&A_CURVE_NAMED_ALIKE_AMONG_OTHERS, "alike", BOTH_NAME_ALIKE)] {
        let mut racked = arranged(&format!("row_presets_alike_{what}_rack"), TRACK, DELAY_ROW, held);
        let rack = in_its_rack(&mut racked, TRACK, DELAY_ROW, SEAR);
        written(&mut racked, &rack);
        let wanted = read(&mut racked, TRACK);
        assert_eq!(kind_at(&wanted, DELAY_ROW), Some(FREAK), "wanted the freak preset in the delay's row of the written rack, got {}", wanted.write());
        for (index, (how, load)) in loads(TRACK, DELAY_ROW, SEAR).into_iter().enumerate() {
            let mut app = arranged(&format!("row_presets_alike_{what}_{index}"), TRACK, DELAY_ROW, held);
            written(&mut app, &load);
            let track = read(&mut app, TRACK);
            for key in LEFT_ALIKE {
                assert!(track.get(key) == wanted.get(key), "wanted the {key} a whole-rack write of the same preset leaves, a curve being on {knob}, loaded {how}, got {} against {}", shown(track.get(key)), shown(wanted.get(key)));
            }
        }
    }
}

#[test]
fn a_freak_preset_in_a_written_rack_counts_what_it_drops_from_the_delay_row() {
    let row = format!("{TRACK}/rack/{DELAY_ROW}");
    let mut app = arranged("row_presets_rack_counts", TRACK, DELAY_ROW, &ONLY_THE_DELAYS_WITH_A_LANE);
    let rack = in_its_rack(&mut app, TRACK, DELAY_ROW, SEAR);
    let reply = written(&mut app, &rack);
    for noun in [LANE, CURVE] {
        let count = dropped(&reply, &row, noun);
        assert_eq!(count, Some(1), "wanted the preset's line at {row} to count the 1 {noun} dropped, no such knob, by a whole-rack write, got {count:?} in: {reply}");
    }
}

#[test]
fn a_delay_preset_loaded_onto_a_delay_row_keeps_its_curves_and_brings_its_own_lanes() {
    for (index, (how, load)) in loads(TRACK, DELAY_ROW, CROSS).into_iter().enumerate() {
        let mut app = arranged(&format!("row_presets_own_kind_{index}"), TRACK, DELAY_ROW, &A_CURVE_AMONG_OTHERS);
        let before = read(&mut app, TRACK);
        let reply = written(&mut app, &load);
        let track = read(&mut app, TRACK);
        assert_eq!(kind_at(&track, DELAY_ROW), Some(DELAY), "wanted the delay preset in the delay's row, loaded {how}, got {}", track.write());
        assert!(track.get("curves") == before.get("curves"), "wanted the row's curves left as they were under a preset of its own kind, loaded {how}, got {} against {}", shown(track.get("curves")), shown(before.get("curves")));
        let mut brought = lanes(&track);
        brought.sort();
        assert_eq!(brought, own_lanes(CROSS, DELAY_ROW), "wanted the preset's own LFO lanes in place of the row's, loaded {how}, got {brought:?}");
        assert!(!reply.contains("dropped"), "wanted nothing told as dropped under a preset of the row's own kind, loaded {how}, got {reply}");
    }
}
