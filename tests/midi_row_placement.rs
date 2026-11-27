use bp::app::{App, Event};
use bp::arp::{Arp, MidiRow, Ties};
use bp::division::Division;
use bp::draw::Rect;
use bp::face::{CELL, H, PITCH};
use bp::fx::Kind;
use bp::json::{self, Json};
use bp::keyboard::Key;
use bp::project::{Cores, Project, Row, Track, settled_rack};
use support::{CORE_ROW, HEIGHT, LEVEL, WIDTH, call, core, effect, face_rect, midi, painted};

const MIDI_ROW: usize = 0;
const DELAY: usize = 3;
const RACK_X: i32 = 6;
const RACK_SHORT_OF_FACE: i32 = 10;
const RAIL: i32 = 45;
const TILE: i32 = 120;
const BORDER: i32 = 8;
const AUDIO_SLOT: usize = 1;
const MIDI_SLOT: i32 = 1;
const HEAD: i32 = 2;
const SOFT: u8 = 64;

type Pick = fn(Rect, usize) -> (i32, i32);

fn arped() -> Arp {
    Arp { on: true, rate: Division::Sixteenth, ..Arp::DEFAULT }
}

fn labels(rack: &[Row]) -> Vec<&'static str> {
    rack.iter().map(Row::label).collect()
}

fn opened(text: &str) -> Project {
    Project::from_json(&json::parse(text).unwrap_or_else(|| panic!("wanted valid JSON, got {text}"))).unwrap_or_else(|| panic!("wanted a project to open from {text}"))
}

fn open(project: &Project, open: usize) -> App {
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false,\"selected\":0,\"open\":{open}}}", project.to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.draw(WIDTH, HEIGHT);
    app.take_requests();
    app
}

fn read(app: &mut App, path: &str) -> Json {
    let reply = call(app, "read", &format!(r#"{{"path":"{path}"}}"#));
    assert!(!reply.failed, "wanted {path} read, got {}", reply.text);
    json::parse(&reply.text).unwrap_or_else(|| panic!("wanted {path} read as JSON, got {}", reply.text))
}

fn kinds(rack: &Json) -> Vec<String> {
    rack.as_array().unwrap_or(&[]).iter().map(|row| String::from(row.get("kind").and_then(Json::as_str).unwrap_or("?"))).collect()
}

fn rack_kinds(app: &mut App, track: usize) -> Vec<String> {
    kinds(&read(app, &format!("tracks/{track}/rack")))
}

fn track_names(app: &mut App) -> Vec<String> {
    read(app, "tracks").as_array().unwrap_or(&[]).iter().map(|track| String::from(track.get("name").and_then(Json::as_str).unwrap_or("?"))).collect()
}

fn rack_rect(app: &mut App) -> Rect {
    let face = face_rect(&painted(app));
    Rect::new(RACK_X, face.y, face.x - RACK_SHORT_OF_FACE, H)
}

fn slot(row: usize) -> i32 {
    if row == MIDI_ROW { MIDI_SLOT } else { HEAD + row as i32 - 1 }
}

fn grip(rack: Rect, row: usize) -> (i32, i32) {
    (rack.x + CELL / 2, rack.y + slot(row) * PITCH + CELL / 2)
}

fn name(rack: Rect, row: usize) -> (i32, i32) {
    (rack.x + rack.w / 2, rack.y + slot(row) * PITCH + CELL / 2)
}

fn drag(app: &mut App, (x, y): (i32, i32), (to_x, to_y): (i32, i32)) {
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Move { x, y: (y + to_y) / 2 });
    app.event(Event::Move { x: to_x, y: to_y });
    app.event(Event::Up { x: to_x, y: to_y });
    app.draw(WIDTH, HEIGHT);
}

fn click(app: &mut App, at: (i32, i32)) {
    drag(app, at, at);
}

fn press(app: &mut App, key: Key, command: bool) {
    app.event(Event::Key { key, shift: false, command, repeat: false });
    app.event(Event::KeyUp { key });
    app.draw(WIDTH, HEIGHT);
}

fn spelled(rows: &[&str]) -> Vec<String> {
    rows.iter().map(|row| String::from(*row)).collect()
}

#[test]
fn a_new_midi_track_opens_with_a_midi_row_over_a_core() {
    let wanted = [MidiRow::LABEL, "Core"];
    assert_eq!(labels(&Track::blank().rack), wanted, "wanted a blank track's rack a MIDI row over a Core");
    let mut project = Project::new();
    assert!(project.add_midi_track(project.tracks.len()), "wanted room for another MIDI track");
    let shapes: Vec<Vec<&str>> = project.tracks.iter().map(|track| labels(&track.rack)).collect();
    assert!(shapes.iter().all(|shape| *shape == wanted), "wanted every new MIDI track's rack a MIDI row over a Core, got {shapes:?}");
    let written = opened(r#"{"kind": "project", "tracks": [{"name": "bare"}, {"name": "core first", "rack": [{"kind": "core"}, {"kind": "reverb"}]}]}"#);
    assert_eq!(labels(&written.tracks[0].rack), wanted, "wanted a MIDI track written without a rack to open as a MIDI row over a Core");
    assert_eq!(labels(&written.tracks[1].rack), [MidiRow::LABEL, "Core", Kind::Reverb.label()], "wanted a MIDI row put above a rack written without one");
}

#[test]
fn a_saved_midi_track_writes_its_midi_row_first_and_reads_it_back() {
    let saved = Project::new().to_json();
    let rack = saved.get("tracks").and_then(Json::as_array).and_then(|tracks| tracks[0].get("rack")).unwrap_or_else(|| panic!("wanted the first track's rack written, got {}", saved.write()));
    assert_eq!(kinds(rack), spelled(&["midi", "core"]), "wanted a fresh track saved as a MIDI row then a Core, got {}", rack.write());
    let plain = &rack.as_array().expect("rows")[0];
    assert_eq!(plain.write_at(0), r#"{"kind": "midi"}"#, "wanted a MIDI row at its defaults written as its kind alone");
    let row = Row::Midi(MidiRow { bypass: true, velocity: SOFT, force_key: true, ties: Ties::Down, arp: arped(), ..MidiRow::DEFAULT });
    let json = row.to_json();
    let (bypass, velocity, arp) = (json.get("bypass").and_then(Json::as_bool), json.get("velocity").and_then(Json::as_f64), json.get("arp").and_then(|arp| arp.get("on")).and_then(Json::as_bool));
    assert_eq!((bypass, velocity, arp), (Some(true), Some(f64::from(SOFT)), Some(true)), "wanted bypass, VEL and the arp written under bypass, velocity and arp, got {}", json.write());
    let (forced, ties) = (json.get("forceKey").and_then(Json::as_bool), json.get("ties").and_then(Json::as_str));
    assert_eq!((forced, ties), (Some(true), Some("down")), "wanted FORCE and TIES written under forceKey and ties, got {}", json.write());
    assert_eq!(Row::from_json(&json), Some(row), "the MIDI row changed in the save");
}

#[test]
fn the_midi_row_holds_row_zero_against_every_move_and_removal() {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(arped()), core(LEVEL), effect(Kind::Reverb), effect(Kind::Delay)];
    let mut app = open(&project, CORE_ROW);
    let rack = rack_rect(&mut app);
    let whole = spelled(&["midi", "core", "reverb", "delay"]);
    assert_eq!(rack_kinds(&mut app, 0), whole, "wanted the rack opened as built");
    drag(&mut app, grip(rack, MIDI_ROW), grip(rack, DELAY));
    assert_eq!(rack_kinds(&mut app, 0), whole, "wanted the MIDI row's grip dragged down the rack to move nothing");
    drag(&mut app, grip(rack, MIDI_ROW), (rack.x + rack.w + 2 * CELL, rack.y + H / 2));
    assert_eq!(rack_kinds(&mut app, 0), whole, "wanted the MIDI row's grip dragged out of the rack to remove nothing");
    drag(&mut app, grip(rack, CORE_ROW), grip(rack, MIDI_ROW));
    assert_eq!(rack_kinds(&mut app, 0), whole, "wanted the Core dragged onto row 0 to stay at row {CORE_ROW}");
    drag(&mut app, grip(rack, DELAY), grip(rack, MIDI_ROW));
    assert_eq!(rack_kinds(&mut app, 0), spelled(&["midi", "delay", "core", "reverb"]), "wanted the delay dragged onto row 0 to land at row 1 under the MIDI row");
    click(&mut app, grip(rack, MIDI_ROW));
    press(&mut app, Key::Delete, false);
    assert_eq!((track_names(&mut app).len(), rack_kinds(&mut app, 0)), (1, spelled(&["midi", "delay", "core", "reverb"])), "wanted Delete on the picked MIDI row to remove nothing");
    let arp = read(&mut app, "tracks/0/rack/0").get("arp").and_then(|arp| arp.get("on")).and_then(Json::as_bool);
    assert_eq!(arp, Some(true), "wanted the MIDI row left at row 0 to keep its arp");
    click(&mut app, grip(rack, DELAY));
    press(&mut app, Key::Delete, false);
    assert_eq!(rack_kinds(&mut app, 0), spelled(&["midi", "delay", "core"]), "wanted Delete on the picked reverb to remove it");
}

#[test]
fn delete_on_the_picked_midi_row_keeps_both_tracks_and_every_row() {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].name = String::from("KEYS");
    project.tracks[0].rack = vec![midi(arped()), core(LEVEL), effect(Kind::Reverb)];
    project.tracks.push(Track { name: String::from("BASS"), ..Track::blank() });
    let whole = (spelled(&["KEYS", "BASS"]), spelled(&["midi", "core", "reverb"]), spelled(&["midi", "core"]));
    let picks: [(&str, Pick); 2] = [("its grip", grip), ("its name", name)];
    for (how, pick) in picks {
        let mut app = open(&project, CORE_ROW);
        let rack = rack_rect(&mut app);
        click(&mut app, pick(rack, MIDI_ROW));
        press(&mut app, Key::Delete, false);
        let kept = (track_names(&mut app), rack_kinds(&mut app, 0), rack_kinds(&mut app, 1));
        assert_eq!(kept, whole, "wanted Delete on the MIDI row picked by {how} to remove nothing, both tracks and every row kept");
    }
}

#[test]
fn audio_group_return_and_master_racks_refuse_a_midi_row() {
    let project = opened(
        r#"{"kind": "project", "tracks": [
            {"name": "take", "kind": "audio", "rack": [{"kind": "midi", "arp": {"on": true}}, {"kind": "eq"}]},
            {"name": "bus", "kind": "group", "rack": [{"kind": "midi"}, {"kind": "utility"}]},
            {"name": "keys", "rack": [{"kind": "midi", "arp": {"on": true}}, {"kind": "core"}]}
        ],
        "returns": [{"rack": [{"kind": "midi"}, {"kind": "reverb"}]}],
        "master": {"rack": [{"kind": "midi"}, {"kind": "compressor"}]}}"#,
    );
    let racks = [labels(&project.tracks[0].rack), labels(&project.tracks[1].rack), labels(&project.sends[0].rack), labels(&project.master.rack)];
    let wanted = [vec![Kind::Eq.label()], vec![Kind::Utility.label()], vec![Kind::Reverb.label()], vec![Kind::Compressor.label()]];
    assert_eq!(racks, wanted, "wanted the audio, group, return and master racks read without their MIDI rows");
    assert!(matches!(project.tracks[2].rack[MIDI_ROW], Row::Midi(row) if row.arp.on), "wanted the MIDI track to keep its MIDI row and its arp, got {:?}", project.tracks[2].rack);
    let refused = settled_rack(Cores::Refused, vec![midi(arped()), core(LEVEL), effect(Kind::Eq), midi(Arp::DEFAULT)]);
    assert_eq!(refused, vec![effect(Kind::Eq)], "wanted a rack that refuses Cores to settle without its MIDI rows");
}

#[test]
fn a_copied_midi_row_never_pastes_into_an_audio_rack() {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(arped()), core(LEVEL), effect(Kind::Delay)];
    let mut take = Track::audio();
    take.rack = vec![effect(Kind::Eq)];
    project.tracks.push(take);
    let mut app = open(&project, CORE_ROW);
    press(&mut app, Key::Char('a'), true);
    press(&mut app, Key::Char('c'), true);
    click(&mut app, (TILE / 2, RAIL + AUDIO_SLOT as i32 * (TILE - BORDER) + 2 * BORDER));
    press(&mut app, Key::Char('v'), true);
    assert_eq!(rack_kinds(&mut app, AUDIO_SLOT), spelled(&["eq", "delay"]), "wanted the copied delay pasted into the audio track's rack and the MIDI row and Core it refuses left out");
    assert_eq!(rack_kinds(&mut app, 0), spelled(&["midi", "core", "delay"]), "wanted the copied MIDI track's rack left as it was");
}
