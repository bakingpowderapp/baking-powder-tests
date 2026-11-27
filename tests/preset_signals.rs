use bp::arp::Arp;
use bp::fx::{Kind, Slot};
use bp::json::Json;
use bp::project::{Host, Preset, Project, Row, Sound};
use support::{LEVEL, core, midi, struck};

const SOURCE: usize = 0;
const HOST: usize = 1;
const PRESSED_ROW: usize = 2;
const SIGNALLED: [Kind; 3] = [Kind::Gate, Kind::Sidechain, Kind::Space];
const UNSIGNALLED_PRE: (Option<usize>, bool) = (None, true);
const HALF: f64 = 0.5;

fn wired(kind: Kind, source: Option<usize>, pre: bool) -> Row {
    let mut slot = Slot::new(kind);
    slot.wire(source, pre);
    Row::Effect(slot)
}

fn wirings(rack: &[Row]) -> Vec<(Option<usize>, bool)> {
    rack.iter().filter_map(|row| if let Row::Effect(slot) = row { slot.wiring() } else { None }).collect()
}

fn song() -> Project {
    let mut project = Project::new();
    project.tracks.truncate(2);
    project.tracks[SOURCE].rack = vec![struck(Arp::DEFAULT), core(1.0)];
    project.tracks[HOST].rack = vec![struck(Arp::DEFAULT), core(LEVEL)];
    project
}

fn landed(mut project: Project, preset: Preset, pressed: Option<usize>) -> Project {
    let host = Host::Track(HOST);
    let plan = project.plan(host, &preset, pressed, false).unwrap_or_else(|denial| panic!("wanted the {} preset to land, got {denial:?}", preset.name()));
    project.land(host, preset, plan, pressed, ("Signals", ""));
    project
}

fn naming(json: &mut Json, source: usize) {
    let Json::Object(fields) = json else {
        return;
    };
    for (_, rows) in fields.iter_mut().filter(|(key, _)| key == "rack") {
        let Json::Array(rows) = rows else {
            continue;
        };
        for row in rows {
            if let Json::Object(row) = row
                && row.iter().any(|(key, kind)| key == "kind" && SIGNALLED.iter().any(|signalled| kind.as_str() == Some(signalled.name())))
            {
                row.push((String::from("sourceTrack"), Json::Number(source as f64)));
            }
        }
    }
}

#[test]
fn a_preset_brings_no_signal_into_a_song_and_one_row_pressed_onto_its_kind_keeps_that_rows() {
    let rack: Vec<Row> = [midi(Arp::DEFAULT), core(LEVEL)].into_iter().chain(SIGNALLED.map(|kind| wired(kind, Some(SOURCE), true))).collect();
    let mut saved = Preset::Rack(Sound { rack, lfos: Vec::new() }).to_json();
    let text = saved.write();
    assert!(!text.contains("sourceTrack") && text.matches("preEffects").count() == SIGNALLED.len(), "wanted a rack preset saved with each row's PRE and no sourceTrack, got {text}");
    naming(&mut saved, SOURCE);
    assert_eq!(saved.write().matches("sourceTrack").count(), SIGNALLED.len(), "wanted the preset's file to name a track on each of its three rows");
    let preset = Preset::from_json(&saved).expect("wanted a rack preset naming a track to read");
    let sound = landed(song(), preset, None);
    assert_eq!(wirings(&sound.tracks[HOST].rack), [UNSIGNALLED_PRE; 3], "wanted the gate, sidechain and space of a rack preset whose file names a track to arrive taking no signal, each keeping PRE");
    for kind in SIGNALLED {
        let joined = landed(song(), Preset::Row(wired(kind, Some(SOURCE), true), Vec::new()), None);
        assert_eq!(wirings(&joined.tracks[HOST].rack), [UNSIGNALLED_PRE], "wanted a {} row preset to join the rack taking no signal, keeping PRE", kind.label());
        let mut keyed = song();
        keyed.tracks[HOST].rack.push(wired(kind, Some(SOURCE), false));
        let mut pressed = Slot::new(kind);
        pressed.wet = HALF;
        pressed.wire(None, true);
        let swapped = landed(keyed, Preset::Rack(Sound { rack: vec![Row::Effect(pressed)], lfos: Vec::new() }), Some(PRESSED_ROW));
        let row = swapped.tracks[HOST].rack[PRESSED_ROW];
        assert_eq!((row.mix(), wirings(&[row])), (HALF, vec![(Some(SOURCE), false)]), "wanted a one-row {} rack preset pressed onto a row of its kind to bring its own settings and keep that row's signal and tap", kind.label());
    }
}
