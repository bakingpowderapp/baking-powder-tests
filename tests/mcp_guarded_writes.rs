use bp::app::App;
use bp::factory::PRESETS;
use bp::fx::{Kind, Slot};
use bp::json::{self, Json};
use bp::project::{Project, Row};
use std::path::Path;
use support::call;

const CROSS: &str = "Effects/Delay/Cross.json";
const SEAR: &str = "Effects/Freak/Sear.json";
const TRACK: &str = "tracks/0";
const MASTER: &str = "master";
const TRACK_GAIN: &str = "tracks/0/gain";
const OTHER_GAIN: &str = "tracks/1/gain";
const DELAY_ROW: usize = 2;
const FREAK_ROW: usize = 3;
const MASTER_ROW: usize = 0;
const DELAY_LANE: &str = "feedback";
const FREAK_LANE: &str = "bands[0].driveDb";
const MASTER_LANE: &str = "thresholdDb";
const DELAY_CURVE: &str = "width";
const CROSS_PAN: &str = "wetPan";
const CHANGED_AMOUNT: f64 = 0.8765;
const CHANGED_GAIN: f64 = 0.7;
const WRITTEN_GAIN: f64 = 0.5;
const ONE_ELSEWHERE: &str = "1 change elsewhere";
const A_FREAK: &str = r#"{"kind":"freak"}"#;
const FREAK: &str = "freak";
const AUDIO: &str = "audio";
const CORE_ROW: usize = 1;

struct Song {
    app: App,
    read_at: u64,
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
    project.tracks[0].rack.push(Row::Effect(Slot::new(Kind::Delay)));
    project.tracks[0].rack.push(Row::Effect(Slot::new(Kind::Freak)));
    project.master.rack = vec![Row::Effect(Slot::new(Kind::Compressor))];
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false}}", project.to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.factory(root.to_str().map(String::from).expect("wanted the factory's path as text"));
    app.take_requests();
    app
}

fn served(app: &mut App, tool: &str, args: &str) -> (bool, String) {
    let reply = call(app, tool, args);
    (reply.failed, reply.text)
}

fn parsed(reply: &str, what: &str) -> Json {
    json::parse(reply).unwrap_or_else(|| panic!("wanted {what} read back as JSON, got {reply}"))
}

fn read(app: &mut App, path: &str) -> Json {
    let (failed, reply) = served(app, "read", &format!(r#"{{"path":"{path}"}}"#));
    assert!(!failed, "wanted {path} read back, got {reply}");
    parsed(&reply, path)
}

fn whole(app: &mut App) -> Json {
    let (failed, reply) = served(app, "read", "{}");
    assert!(!failed, "wanted the song read back, got {reply}");
    parsed(&reply, "the song")
}

fn lane(row: usize, param: &str) -> String {
    format!(r#"{{"row":{row},"param":"{param}","amount":0.5,"tempoSync":true,"division":"1/4"}}"#)
}

fn arranged() -> String {
    let lanes = format!(r#"{{"path":"{TRACK}/lfos","value":[{},{}]}}"#, lane(DELAY_ROW, DELAY_LANE), lane(FREAK_ROW, FREAK_LANE));
    let curves = format!(r#"{{"path":"{TRACK}/curves","value":[{{"row":{DELAY_ROW},"param":"{DELAY_CURVE}","nodes":[{{"beat":0,"value":0.2}},{{"beat":4,"value":0.9}}]}}]}}"#);
    let master = format!(r#"{{"path":"{MASTER}/lfos","value":[{}]}}"#, lane(MASTER_ROW, MASTER_LANE));
    format!(r#"{{"edits":[{lanes},{curves},{master}]}}"#)
}

fn row_preset(host: &str, row: usize, preset: &str) -> String {
    format!(r#""path":"{host}/rack/{row}/preset","value":"{preset}""#)
}

fn value_at(path: &str, value: &str) -> String {
    format!(r#""path":"{path}","value":{value}"#)
}

fn aim(item: &Json) -> (Option<usize>, Option<&str>) {
    (item.get("row").and_then(Json::as_f64).map(|row| row as usize), item.get("param").and_then(Json::as_str))
}

fn own_lanes(preset: &str) -> Vec<String> {
    let (_, text) = PRESETS.iter().find(|(known, _)| *known == preset).unwrap_or_else(|| panic!("wanted {preset} in the factory"));
    let mut params: Vec<String> = parsed(text, preset).get("lfos").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(|lane| lane.get("param").and_then(Json::as_str)).map(String::from).collect();
    params.sort();
    params
}

impl Song {
    fn read_after(name: &str, first: &[String]) -> Song {
        let mut song = Song { app: open(name), read_at: 0 };
        song.write(&arranged());
        for edit in first {
            song.write(&format!("{{{edit}}}"));
        }
        song.read_at = whole(&mut song.app).get("revision").and_then(Json::as_f64).expect("wanted the read to give its revision") as u64;
        song
    }

    fn write(&mut self, args: &str) {
        let (failed, reply) = served(&mut self.app, "write", args);
        assert!(!failed, "wanted the write taken: {args}, got {reply}");
    }

    fn changes(&mut self, path: &str, value: &str) {
        self.write(&format!("{{{}}}", value_at(path, value)));
    }

    fn guarded(&mut self, edit: &str) -> (bool, String) {
        served(&mut self.app, "write", &format!(r#"{{{edit},"unchangedSince":{}}}"#, self.read_at))
    }

    fn list(&mut self, path: &str) -> Vec<Json> {
        read(&mut self.app, path).as_array().map(<[Json]>::to_vec).unwrap_or_default()
    }

    fn index(&mut self, list: &str, row: usize, param: &str) -> usize {
        self.list(list).iter().position(|item| aim(item) == (Some(row), Some(param))).unwrap_or_else(|| panic!("wanted {param} on row {row} in {list}"))
    }

    fn amount(&mut self, list: &str, row: usize, param: &str) -> Option<f64> {
        self.list(list).iter().find(|item| aim(item) == (Some(row), Some(param))).and_then(|item| item.get("amount")).and_then(Json::as_f64)
    }

    fn lanes_on(&mut self, list: &str, row: usize) -> Vec<String> {
        let mut params: Vec<String> = self.list(list).iter().filter(|item| aim(item).0 == Some(row)).filter_map(|item| aim(item).1.map(String::from)).collect();
        params.sort();
        params
    }

    fn changes_lane(&mut self, host: &str, row: usize, param: &str) -> String {
        let lanes = format!("{host}/lfos");
        let lane = format!("{lanes}/{}", self.index(&lanes, row, param));
        self.changes(&format!("{lane}/amount"), &CHANGED_AMOUNT.to_string());
        lane
    }
}

fn assert_refused(song: &mut Song, edit: &str, at: &str, since: &str) {
    let before = whole(&mut song.app);
    let (failed, reply) = song.guarded(edit);
    assert!(failed, "wanted the guarded write refused, {since} since revision {}, got {reply}", song.read_at);
    assert!(reply.contains(at), "wanted the refusal to say the song changed at {at}, got {reply}");
    let after = whole(&mut song.app);
    assert!(after == before, "wanted the refused write to leave the song as it was, got {}", after.write());
}

#[test]
fn a_guarded_row_preset_is_refused_when_a_lane_on_its_row_changed_since_the_read() {
    let mut song = Song::read_after("guard_lane_changed", &[]);
    let lane = song.changes_lane(TRACK, DELAY_ROW, DELAY_LANE);
    assert_refused(&mut song, &row_preset(TRACK, DELAY_ROW, CROSS), &lane, "a lane on the preset's row having changed");
    let amount = song.amount(&format!("{TRACK}/lfos"), DELAY_ROW, DELAY_LANE);
    assert_eq!(amount, Some(CHANGED_AMOUNT), "wanted the lane's changed amount kept, got {amount:?}");
}

#[test]
fn a_guarded_row_preset_is_refused_when_it_would_bring_back_a_lane_deleted_since_the_read() {
    let mut song = Song::read_after("guard_lane_deleted", &[row_preset(TRACK, DELAY_ROW, CROSS)]);
    let lanes = format!("{TRACK}/lfos");
    let pan = song.index(&lanes, DELAY_ROW, CROSS_PAN);
    song.changes(&format!("{lanes}/{pan}"), "null");
    assert_refused(&mut song, &row_preset(TRACK, DELAY_ROW, CROSS), &lanes, "a lane the preset brings having been deleted");
    let kept = song.lanes_on(&lanes, DELAY_ROW);
    assert!(!kept.iter().any(|param| param == CROSS_PAN), "wanted the deleted pan lane to stay deleted, got {kept:?} on the row");
}

#[test]
fn a_guarded_row_preset_is_refused_when_the_lanes_it_would_bring_back_were_cleared_since_the_read() {
    let mut song = Song::read_after("guard_lanes_cleared", &[row_preset(TRACK, DELAY_ROW, CROSS)]);
    let lanes = format!("{TRACK}/lfos");
    song.changes(&lanes, "null");
    assert_refused(&mut song, &row_preset(TRACK, DELAY_ROW, CROSS), &lanes, "the track's lanes having been cleared");
    let track = read(&mut song.app, TRACK);
    assert!(track.get("lfos").is_none(), "wanted the track left without lanes, got {}", track.write());
}

#[test]
fn a_guarded_row_preset_is_refused_when_a_curve_it_would_drop_was_deleted_since_the_read() {
    let mut song = Song::read_after("guard_curve_deleted", &[]);
    let curves = format!("{TRACK}/curves");
    let curve = song.index(&curves, DELAY_ROW, DELAY_CURVE);
    song.changes(&format!("{curves}/{curve}"), "null");
    assert_refused(&mut song, &row_preset(TRACK, DELAY_ROW, SEAR), &curves, "a curve on the row it swaps having been deleted");
}

#[test]
fn a_guarded_row_preset_on_the_master_is_refused_when_its_rows_lane_changed_since_the_read() {
    let mut song = Song::read_after("guard_master_lane", &[]);
    let lane = song.changes_lane(MASTER, MASTER_ROW, MASTER_LANE);
    assert_refused(&mut song, &row_preset(MASTER, MASTER_ROW, SEAR), &lane, "a lane on the master row it swaps having changed");
}

#[test]
fn a_guarded_preview_of_a_row_preset_is_refused_as_its_write_would_be() {
    let mut song = Song::read_after("guard_preview", &[]);
    let lane = song.changes_lane(TRACK, DELAY_ROW, DELAY_LANE);
    assert_refused(&mut song, &format!(r#"{},"preview":true"#, row_preset(TRACK, DELAY_ROW, SEAR)), &lane, "a lane on the previewed preset's row having changed");
}

#[test]
fn a_row_preset_among_other_edits_is_guarded_the_same_and_none_of_them_lands() {
    let mut song = Song::read_after("guard_among_edits", &[]);
    let lane = song.changes_lane(TRACK, DELAY_ROW, DELAY_LANE);
    let edits = format!(r#""edits":[{{{}}},{{{}}}]"#, value_at(OTHER_GAIN, &WRITTEN_GAIN.to_string()), row_preset(TRACK, DELAY_ROW, SEAR));
    assert_refused(&mut song, &edits, &lane, "a lane on the preset's row having changed");
}

#[test]
fn a_row_preset_inside_a_whole_rack_is_guarded_the_same() {
    let mut song = Song::read_after("guard_whole_rack", &[]);
    let lane = song.changes_lane(TRACK, DELAY_ROW, DELAY_LANE);
    let mut rack = song.list(&format!("{TRACK}/rack"));
    rack[DELAY_ROW] = parsed(&format!(r#"{{"preset":"{SEAR}"}}"#), "the preset's row");
    assert_refused(&mut song, &value_at(&format!("{TRACK}/rack"), &Json::Array(rack).write()), &lane, "a lane on the nested preset's row having changed");
}

#[test]
fn a_guarded_row_preset_is_taken_when_only_another_rows_lane_changed_since_the_read() {
    let mut song = Song::read_after("guard_other_row", &[]);
    song.changes_lane(TRACK, FREAK_ROW, FREAK_LANE);
    let (failed, reply) = song.guarded(&row_preset(TRACK, DELAY_ROW, CROSS));
    assert!(!failed, "wanted the guarded load taken, only another row's lane having changed since revision {}, got {reply}", song.read_at);
    assert!(reply.contains(ONE_ELSEWHERE), "wanted the reply to count the one change elsewhere, got {reply}");
    let lanes = format!("{TRACK}/lfos");
    let amount = song.amount(&lanes, FREAK_ROW, FREAK_LANE);
    assert_eq!(amount, Some(CHANGED_AMOUNT), "wanted the other row's changed lane kept, got {amount:?}");
    let landed = song.lanes_on(&lanes, DELAY_ROW);
    assert_eq!(landed, own_lanes(CROSS), "wanted the preset's own lanes on its row, got {landed:?}");
    let mut plain = Song::read_after("guard_other_row_plain", &[]);
    plain.changes_lane(TRACK, FREAK_ROW, FREAK_LANE);
    plain.write(&format!("{{{}}}", row_preset(TRACK, DELAY_ROW, CROSS)));
    let (guarded, unguarded) = (read(&mut song.app, TRACK), read(&mut plain.app, TRACK));
    assert!(guarded == unguarded, "wanted the guarded load to leave the track an unguarded one leaves, got {} against {}", guarded.write(), unguarded.write());
}

#[test]
fn a_guarded_row_preset_lands_as_an_unguarded_one_does_when_nothing_changed() {
    let mut song = Song::read_after("guard_unchanged", &[]);
    let (failed, reply) = song.guarded(&row_preset(TRACK, DELAY_ROW, CROSS));
    assert!(!failed, "wanted the guarded load taken, nothing having changed since revision {}, got {reply}", song.read_at);
    let mut plain = Song::read_after("guard_unchanged_plain", &[]);
    plain.write(&format!("{{{}}}", row_preset(TRACK, DELAY_ROW, CROSS)));
    let (guarded, unguarded) = (whole(&mut song.app), whole(&mut plain.app));
    assert!(guarded == unguarded, "wanted the guarded load to leave the song an unguarded one leaves, got {} against {}", guarded.write(), unguarded.write());
}

#[test]
fn a_guarded_write_is_refused_when_its_own_path_changed_since_the_read() {
    let mut song = Song::read_after("guard_own_path", &[]);
    song.changes(OTHER_GAIN, &CHANGED_GAIN.to_string());
    assert_refused(&mut song, &value_at(OTHER_GAIN, &WRITTEN_GAIN.to_string()), OTHER_GAIN, "its own path having changed");
}

#[test]
fn a_guarded_write_is_taken_when_only_another_path_changed_and_counts_the_change() {
    let mut song = Song::read_after("guard_other_path", &[]);
    song.changes(OTHER_GAIN, &CHANGED_GAIN.to_string());
    let (failed, reply) = song.guarded(&value_at(TRACK_GAIN, &WRITTEN_GAIN.to_string()));
    assert!(!failed, "wanted the guarded write taken, only another path having changed since revision {}, got {reply}", song.read_at);
    assert!(reply.contains(ONE_ELSEWHERE), "wanted the reply to count the one change elsewhere, got {reply}");
    let gains = (read(&mut song.app, TRACK_GAIN).as_f64(), read(&mut song.app, OTHER_GAIN).as_f64());
    assert_eq!(gains, (Some(WRITTEN_GAIN), Some(CHANGED_GAIN)), "wanted the written gain landed and the other track's changed gain kept, got {gains:?}");
}

#[test]
fn a_guarded_write_of_a_row_is_refused_when_a_lane_it_would_drop_changed_since_the_read() {
    let mut song = Song::read_after("guard_plain_row", &[]);
    let lane = song.changes_lane(TRACK, DELAY_ROW, DELAY_LANE);
    assert_refused(&mut song, &value_at(&format!("{TRACK}/rack/{DELAY_ROW}"), A_FREAK), &lane, "a lane on the row it replaces having changed");
}

#[test]
fn a_guarded_write_of_a_rows_kind_is_refused_when_a_lane_it_would_drop_changed_since_the_read() {
    let mut song = Song::read_after("guard_plain_kind", &[]);
    let lane = song.changes_lane(TRACK, DELAY_ROW, DELAY_LANE);
    assert_refused(&mut song, &value_at(&format!("{TRACK}/rack/{DELAY_ROW}/kind"), &format!("\"{FREAK}\"")), &lane, "a lane on the row whose kind it changes having changed");
}

#[test]
fn a_guarded_write_of_a_whole_rack_is_refused_when_a_lane_on_a_row_it_leaves_out_changed_since_the_read() {
    let mut song = Song::read_after("guard_plain_rack", &[]);
    let lane = song.changes_lane(TRACK, DELAY_ROW, DELAY_LANE);
    let mut rack = song.list(&format!("{TRACK}/rack"));
    rack.remove(DELAY_ROW);
    assert_refused(&mut song, &value_at(&format!("{TRACK}/rack"), &Json::Array(rack).write()), &lane, "a lane on a row the written rack leaves out having changed");
}

#[test]
fn a_guarded_write_of_a_whole_rack_that_keeps_its_rows_is_taken_and_a_lane_changed_since_the_read_stays() {
    let mut song = Song::read_after("guard_rack_kept", &[]);
    song.changes_lane(TRACK, DELAY_ROW, DELAY_LANE);
    let rack = song.list(&format!("{TRACK}/rack"));
    let (failed, reply) = song.guarded(&value_at(&format!("{TRACK}/rack"), &Json::Array(rack).write()));
    assert!(!failed, "wanted the guarded write of the rack as it stood taken, no row of it dropped, got {reply}");
    let amount = song.amount(&format!("{TRACK}/lfos"), DELAY_ROW, DELAY_LANE);
    assert_eq!(amount, Some(CHANGED_AMOUNT), "wanted the lane's changed amount kept under the rack written as it stood, got {amount:?}");
}

#[test]
fn a_guarded_write_of_a_row_is_taken_when_only_another_tracks_gain_changed_since_the_read() {
    let mut song = Song::read_after("guard_row_elsewhere", &[]);
    song.changes(OTHER_GAIN, &CHANGED_GAIN.to_string());
    let row = format!("{TRACK}/rack/{DELAY_ROW}");
    let (failed, reply) = song.guarded(&value_at(&row, A_FREAK));
    assert!(!failed, "wanted the guarded write of the row taken, only another track's gain having changed since revision {}, got {reply}", song.read_at);
    assert!(reply.contains(ONE_ELSEWHERE), "wanted the reply to count the one change elsewhere, got {reply}");
    let kind = read(&mut song.app, &format!("{row}/kind"));
    assert_eq!(kind.as_str(), Some(FREAK), "wanted the row written as a freak, got {}", kind.write());
}

#[test]
fn a_guarded_write_of_a_tracks_kind_is_refused_when_a_row_it_would_drop_changed_since_the_read() {
    let mut song = Song::read_after("guard_track_kind", &[]);
    let level = format!("{TRACK}/rack/{CORE_ROW}/level");
    song.changes(&level, &CHANGED_GAIN.to_string());
    assert_refused(&mut song, &value_at(&format!("{TRACK}/kind"), &format!("\"{AUDIO}\"")), &level, "the Core an audio track drops having changed");
}

#[test]
fn a_guarded_write_of_a_tracks_kind_is_taken_when_only_another_tracks_gain_changed_since_the_read() {
    let mut song = Song::read_after("guard_track_kind_elsewhere", &[]);
    song.changes(OTHER_GAIN, &CHANGED_GAIN.to_string());
    let kind = format!("{TRACK}/kind");
    let (failed, reply) = song.guarded(&value_at(&kind, &format!("\"{AUDIO}\"")));
    assert!(!failed, "wanted the guarded write of the track's kind taken, only another track's gain having changed since revision {}, got {reply}", song.read_at);
    assert!(reply.contains(ONE_ELSEWHERE), "wanted the reply to count the one change elsewhere, got {reply}");
    let kept = read(&mut song.app, &kind);
    assert_eq!(kept.as_str(), Some(AUDIO), "wanted the track turned to audio, got {}", kept.write());
}
