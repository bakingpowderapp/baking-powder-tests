use bp::app::{App, Event, Opened};
use bp::arp::Arp;
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::face::{Face, Faced};
use bp::fx::{Kind, Slot};
use bp::json::{self, Json};
use bp::keyboard::Key;
use bp::project::{DEFAULT_BPM, Host, Plan, Preset, Project, Row, Sound, VELOCITY};
use support::{AUDIBLE, BLOCK, BLOCKS, CORE_ROW, LEVEL, SAMPLE_RATE, SILENT, call, core, midi, peak, remembered, struck};

const SOURCE: usize = 0;
const GATED: usize = 1;
const SPARE: usize = 2;
const GATE_ROW: usize = CORE_ROW + 1;
const SETTLE: usize = 24;
const SOURCE_NOTE: (usize, u8) = (SOURCE, 36);
const GATED_NOTE: (usize, u8) = (GATED, 60);
const SIGNAL: (&str, &str) = ("SIGNAL", "SOURCE");
const NAMES: [&str; 4] = ["off", "MIDI 1", "MIDI 2", "MIDI 3"];
const KEPT_DIFFERENTLY: &str = "Kept differently";
const ABOVE: usize = 0;
const BELOW: usize = 1;
const OTHER_SONG: &str = "/x/songs/other.json";

fn gate(source: Option<usize>, pre: bool) -> Slot {
    let mut slot = Slot::new(Kind::Gate);
    slot.wire(source, pre);
    slot
}

fn song(source: Option<usize>, pre: bool) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(2);
    project.tracks[SOURCE].rack = vec![struck(Arp::DEFAULT), core(1.0)];
    project.tracks[GATED].rack = vec![struck(Arp::DEFAULT), core(LEVEL), Row::Effect(gate(source, pre))];
    project
}

fn open(project: &Project, notes: &[(usize, u8)]) -> (Audio, Renderer) {
    let (mut audio, renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, None, &[], &[]);
    for &(track, midi) in notes {
        audio.note_on(track, midi, VELOCITY);
    }
    audio.play(0.0);
    (audio, renderer)
}

fn run(project: &Project, notes: &[(usize, u8)]) -> Vec<Vec<f32>> {
    let (_audio, mut renderer) = open(project, notes);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    (0..BLOCKS)
        .map(|_| {
            renderer.render(&mut l, &mut r);
            l.clone()
        })
        .collect()
}

fn difference(a: &[Vec<f32>], b: &[Vec<f32>]) -> Vec<Vec<f32>> {
    a.iter().zip(b).map(|(x, y)| x.iter().zip(y).map(|(p, q)| p - q).collect()).collect()
}

fn reread(project: &Project) -> Project {
    let text = project.to_json().write();
    Project::from_json(&json::parse(&text).expect("wanted the written project to parse")).expect("wanted the written project to read")
}

fn key_of(project: &Project, track: usize, row: usize) -> Option<(usize, bool)> {
    match project.tracks[track].rack.get(row) {
        Some(Row::Effect(slot)) if slot.kind() == Kind::Gate => slot.source(),
        other => panic!("wanted a gate at row {row} of track {track}, got {other:?}"),
    }
}

fn app(project: &Project) -> App {
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false,\"selected\":{GATED}}}", project.to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.take_requests();
    app
}

fn command(app: &mut App, letter: char) {
    app.event(Event::Key { key: Key::Char(letter), shift: false, command: true, repeat: false });
}

fn written(app: &mut App, args: &str) -> String {
    let reply = call(app, "write", args);
    assert!(!reply.failed, "wanted the write taken: {args}, got {}", reply.text);
    reply.text
}

fn read(app: &mut App, path: &str) -> Json {
    let reply = call(app, "read", &format!(r#"{{"path":"{path}"}}"#));
    assert!(!reply.failed, "wanted {path} read, got {}", reply.text);
    json::parse(&reply.text).unwrap_or_else(|| panic!("wanted {path} read as JSON, got {}", reply.text))
}

fn kept_differently(reply: &str) -> Vec<&str> {
    reply.lines().skip_while(|line| !line.starts_with(KEPT_DIFFERENTLY)).skip(1).take_while(|line| line.starts_with(' ')).map(str::trim).collect()
}

fn key_cell(slot: &Slot) -> usize {
    (0..64).find(|&index| slot.setting_of(index).is_some_and(|(band, param, _)| (band, param) == SIGNAL)).expect("wanted the gate's SIGNAL SOURCE cell on its face")
}

fn shown_key(slot: &Slot) -> f64 {
    slot.setting_of(key_cell(slot)).map(|(_, _, source)| source).expect("wanted the gate's SIGNAL SOURCE cell")
}

#[test]
fn a_keyed_gate_opens_on_its_sources_sound_and_stays_shut_without_it() {
    let keyed = song(Some(SOURCE), false);
    let starved = run(&keyed, &[GATED_NOTE]);
    assert!(peak(&starved) < SILENT, "wanted the keyed gate shut while its source is silent, though its own track plays, got a peak of {}", peak(&starved));
    let own = run(&song(None, false), &[GATED_NOTE]);
    assert!(peak(&own[SETTLE..]) > AUDIBLE, "wanted a gate with no key to open on its own input, got a peak of {}", peak(&own[SETTLE..]));
    let opened = difference(&run(&keyed, &[SOURCE_NOTE, GATED_NOTE]), &run(&keyed, &[SOURCE_NOTE]));
    assert!(peak(&opened[SETTLE..]) > AUDIBLE, "wanted the gated track heard while its source sounds, got a peak of {}", peak(&opened[SETTLE..]));
}

#[test]
fn a_pre_tap_opens_the_gate_from_a_source_whose_fader_is_at_zero() {
    let faded = |pre: bool| {
        let mut project = song(Some(SOURCE), pre);
        project.tracks[SOURCE].gain = 0.0;
        project
    };
    let post = run(&faded(false), &[SOURCE_NOTE, GATED_NOTE]);
    assert!(peak(&post) < SILENT, "wanted POST to key the gate from the source's output, silent at a zero fader, so the gate stays shut, got a peak of {}", peak(&post));
    let pre = run(&faded(true), &[SOURCE_NOTE, GATED_NOTE]);
    assert!(peak(&pre[SETTLE..]) > AUDIBLE, "wanted PRE to key the gate from before the source's fader, so it opens, got a peak of {}", peak(&pre[SETTLE..]));
    let source_alone = run(&faded(true), &[SOURCE_NOTE]);
    assert!(peak(&source_alone) < SILENT, "wanted the source itself silent in the mix at a zero fader, got a peak of {}", peak(&source_alone));
}

#[test]
fn a_gates_key_and_tap_are_saved_and_read_back() {
    let project = reread(&song(Some(SOURCE), true));
    assert_eq!(key_of(&project, GATED, GATE_ROW), Some((SOURCE, true)), "wanted the gate's source track and its PRE tap read back as saved");
    let saved = song(Some(SOURCE), true).to_json();
    let row = saved.get("tracks").and_then(Json::as_array).and_then(|tracks| tracks[GATED].get("rack")).and_then(Json::as_array).map(|rack| rack[GATE_ROW].clone()).expect("wanted the gate row saved");
    assert_eq!((row.get("sourceTrack").and_then(Json::as_f64), row.get("preEffects").and_then(Json::as_bool)), (Some(SOURCE as f64), Some(true)), "wanted the key saved as sourceTrack and the tap as preEffects, got {}", row.write());
    let plain = Row::Effect(gate(None, false)).to_json();
    assert!(plain.get("sourceTrack").is_none() && plain.get("preEffects").is_none(), "wanted a gate with no key saved without either, got {}", plain.write());
}

#[test]
fn a_gate_keyed_by_its_own_track_opens_unkeyed() {
    let looped = reread(&song(Some(GATED), false));
    assert_eq!(key_of(&looped, GATED, GATE_ROW), None, "wanted a gate saved with its own track as its key to open following its own input");
    let heard = run(&looped, &[GATED_NOTE]);
    assert!(peak(&heard[SETTLE..]) > AUDIBLE, "wanted the gate that named its own track to open on its own input, got a peak of {}", peak(&heard[SETTLE..]));
}

#[test]
fn a_gates_key_follows_its_source_as_tracks_move_and_clears_when_the_source_goes() {
    let mut project = song(Some(SOURCE), false);
    assert!(project.add_midi_track(SOURCE), "wanted room for a track above the source");
    assert_eq!(key_of(&project, GATED + 1, GATE_ROW), Some((SOURCE + 1, false)), "wanted the key to follow its source down a place when a track is added above it");
    project.move_track(SOURCE + 1, SOURCE);
    assert_eq!(key_of(&project, GATED + 1, GATE_ROW), Some((SOURCE, false)), "wanted the key to follow its source back up when the source is moved");
    assert!(project.remove_track(SOURCE), "wanted the source track removed");
    assert_eq!(key_of(&project, GATED, GATE_ROW), None, "wanted the gate left following its own input when its source track goes");
}

#[test]
fn a_gate_in_a_rack_preset_lands_without_its_key() {
    let mut project = song(None, false);
    let preset = Preset::Rack(Sound { rack: vec![midi(Arp::DEFAULT), core(LEVEL), Row::Effect(gate(Some(SOURCE), true))], lfos: Vec::new() });
    let host = Host::Track(GATED);
    let plan = project.plan(host, &preset, None, false).expect("wanted the rack preset to land on a MIDI track");
    assert_eq!(plan, Plan::Replaces, "wanted a rack preset with a Core to replace the track's sound");
    project.land(host, preset, plan, None, ("Keyed", ""));
    assert_eq!(key_of(&project, GATED, GATE_ROW), None, "wanted the preset's gate to arrive following its own input, whatever track the preset was saved on");
}

#[test]
fn a_row_preset_pressed_onto_a_keyed_gate_keeps_that_rows_key() {
    let mut project = song(Some(SOURCE), true);
    let mut loaded = gate(None, false);
    loaded.wet = 0.5;
    let preset = Preset::Row(Row::Effect(loaded), Vec::new());
    let host = Host::Track(GATED);
    let plan = project.plan(host, &preset, Some(GATE_ROW), false).expect("wanted the gate preset to land on the gate row");
    project.land(host, preset, plan, Some(GATE_ROW), ("Half", ""));
    assert_eq!(project.tracks[GATED].rack[GATE_ROW].mix(), 0.5, "wanted the preset's own settings on the row");
    assert_eq!(key_of(&project, GATED, GATE_ROW), Some((SOURCE, true)), "wanted the row to keep the key and tap it had under the preset pressed onto it");
}

#[test]
fn a_pasted_gate_keeps_its_key_beside_the_copied_one() {
    let mut app = app(&song(Some(SOURCE), false));
    command(&mut app, 'a');
    command(&mut app, 'c');
    command(&mut app, 'v');
    assert_eq!(gates_on(&remembered(&mut app), GATED), [Some((SOURCE, false)), Some((SOURCE, false))], "wanted the copied gate left keyed from its source and its pasted copy keyed from it too, the source being in the song and not the gated track");
}

fn keyed_from_below() -> Project {
    let mut project = Project::new();
    project.tracks.truncate(2);
    project.tracks[ABOVE].rack = vec![struck(Arp::DEFAULT), core(LEVEL), Row::Effect(gate(Some(BELOW), false))];
    project.tracks[BELOW].rack = vec![struck(Arp::DEFAULT), core(1.0)];
    project
}

fn gates_on(project: &Project, track: usize) -> Vec<Option<(usize, bool)>> {
    project.tracks[track].rack.iter().filter_map(|row| match row {
        Row::Effect(slot) if slot.kind() == Kind::Gate => Some(slot.source()),
        _ => None,
    }).collect()
}

#[test]
fn a_gate_copied_in_one_song_and_pasted_in_another_arrives_without_its_key() {
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false,\"selected\":{ABOVE}}}", keyed_from_below().to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.take_requests();
    command(&mut app, 'a');
    command(&mut app, 'c');
    command(&mut app, 'v');
    assert_eq!(gates_on(&remembered(&mut app), ABOVE), [Some((BELOW, false)), Some((BELOW, false))], "wanted the gate pasted in its own song keyed from the track below, as the copied one is");
    let mut other = keyed_from_below();
    other.tracks[ABOVE].rack.truncate(GATE_ROW);
    assert_eq!(app.opened(String::from(OTHER_SONG), Some(&other.to_json().write()), None), Opened::Project, "wanted the other song to open");
    command(&mut app, 'v');
    assert_eq!(gates_on(&remembered(&mut app), ABOVE), [None], "wanted the gate pasted in another song to arrive following its own input, though that song has a track where its key was");
}

#[test]
fn the_key_list_passes_over_the_gates_own_track() {
    let mut face = Face::new();
    face.name_list(NAMES.iter().map(|name| String::from(*name)).collect(), Some(GATED + 1));
    let mut slot = gate(None, false);
    face.pick(key_cell(&slot));
    let mut stepped = Vec::new();
    for _ in 0..NAMES.len() {
        assert!(slot.nudge(&mut face, 1, false, DEFAULT_BPM), "wanted the key list to step on");
        stepped.push(shown_key(&slot));
    }
    assert_eq!(stepped, [(SOURCE + 1) as f64, (SPARE + 1) as f64, 0.0, (SOURCE + 1) as f64], "wanted the steps to take the track above and the track below the gate's own, then off, and never its own");
    assert!(slot.nudge(&mut face, -1, false, DEFAULT_BPM) && slot.nudge(&mut face, -1, false, DEFAULT_BPM), "wanted the key list to step back");
    assert_eq!(shown_key(&slot), (SPARE + 1) as f64, "wanted two steps back from the first track to pass off and land on the last, the gate's own left out");
}

#[test]
fn an_agent_keys_a_gate_and_one_naming_its_own_track_is_told_it_was_not_kept() {
    let mut app = app(&song(None, false));
    let row = format!("tracks/{GATED}/rack/{GATE_ROW}");
    written(&mut app, &format!(r#"{{"edits":[{{"path":"{row}/sourceTrack","value":{SOURCE}}},{{"path":"{row}/preEffects","value":true}}]}}"#));
    let kept = read(&mut app, &row);
    assert_eq!((kept.get("sourceTrack").and_then(Json::as_f64), kept.get("preEffects").and_then(Json::as_bool)), (Some(SOURCE as f64), Some(true)), "wanted the gate's key and tap kept as an agent wrote them, got {}", kept.write());
    let asked = [("at the key's own path", format!(r#"{{"path":"{row}/sourceTrack","value":{GATED}}}"#)), ("in a whole row", format!(r#"{{"path":"{row}","value":{{"kind":"gate","sourceTrack":{GATED}}}}}"#))];
    for (how, args) in asked {
        let reply = written(&mut app, &args);
        let told = kept_differently(&reply);
        assert!(told.iter().any(|line| line.starts_with(&format!("{row}/sourceTrack: sent {GATED}")) && line.contains("not kept")), "wanted the reply to say the gate's own track was not kept as its key, written {how}, got {reply}");
        let kept = read(&mut app, &row);
        assert!(kept.get("sourceTrack").is_none(), "wanted the gate read back without a sourceTrack after its own track was written {how}, got {}", kept.write());
    }
}

#[test]
fn a_gate_rows_manual_lists_its_key_and_tap() {
    let mut app = app(&song(None, false));
    let reply = call(&mut app, "manual", r#"{"row":"gate"}"#);
    assert!(!reply.failed, "wanted the gate row's manual, got {}", reply.text);
    let manual = json::parse(&reply.text).unwrap_or_else(|| panic!("wanted the gate row's manual as JSON, got {}", reply.text));
    let source = manual.get("sourceTrack").and_then(Json::as_str).unwrap_or_default();
    assert!(source.contains("source track") && source.contains("own"), "wanted the manual to list sourceTrack and say it cannot be the gate's own track, got {source:?}");
    assert!(manual.get("preEffects").and_then(Json::as_str).is_some(), "wanted the manual to list preEffects beside it, got {}", manual.write());
}
