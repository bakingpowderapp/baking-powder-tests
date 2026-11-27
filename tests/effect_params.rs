use bp::arp::Arp;
use bp::face::Faced;
use bp::fx::{Kind, Params, Slot};
use bp::json::{self, Json};
use bp::project::{Layer, Preset, Row};
use support::midi;

const FIRST_SPAN: usize = 64;
const WET: f64 = 0.37;
const TRAVELS: [f64; 2] = [0.37, 0.61];
const SPLIT_AT: f64 = 0.3;
const LIFT: f64 = 0.2;
const BEND: f64 = 0.4;
const WILD: f64 = 1e9;

fn moved(slot: &Slot, cell: usize, before: f64) -> bool {
    slot.setting_of(cell).is_some_and(|(_, _, after)| after != before)
}

fn span(kind: Kind) -> usize {
    let slot = Slot::new(kind);
    let mut span = FIRST_SPAN;
    while (span..2 * span).any(|cell| slot.setting_of(cell).is_some()) {
        span *= 2;
    }
    span
}

fn packed(kind: Kind) -> (Slot, Vec<String>) {
    let mut slot = Slot { wet: WET, bypass: true, ..Slot::new(kind) };
    let mut stuck = Vec::new();
    for cell in 0..span(kind) {
        let Some((band, param, before)) = slot.setting_of(cell) else {
            continue;
        };
        let by_travel = TRAVELS.iter().any(|&travel| {
            slot.params.set_travel(band, param, travel);
            moved(&slot, cell, before)
        });
        let by_value = by_travel
            || [before + 1.0, 0.0, before * 0.5].iter().any(|&value| {
                slot.assign(band, param, value);
                moved(&slot, cell, before)
            });
        if !by_value {
            stuck.push(format!("{band} {param}"));
        }
    }
    if let Params::VolumeLfo(lfo) = &mut slot.params {
        lfo.drawn.split(SPLIT_AT);
        lfo.drawn.points[1].y = LIFT;
        lfo.drawn.points[1].curve = BEND;
    }
    if let Params::Eq(eq) = &mut slot.params {
        eq.solo = None;
    }
    (slot, stuck)
}

fn reopened(slot: &Slot) -> Slot {
    let rack = vec![midi(Arp::DEFAULT), Row::Core(Layer::DEFAULT), Row::Core(Layer::DEFAULT), Row::Effect(*slot)];
    let text = Preset::Rack(bp::project::Sound { rack, lfos: Vec::new() }).to_json().write();
    let json = json::parse(&text).expect("a saved rack is valid JSON");
    let Some(Preset::Rack(sound)) = Preset::from_json(&json) else {
        panic!("a saved rack opens");
    };
    let Row::Effect(opened) = sound.rack[3] else {
        panic!("the effect row opens as an effect");
    };
    opened
}

fn wilded(json: Json) -> Json {
    match json {
        Json::Number(_) => Json::Number(WILD),
        Json::Array(items) => Json::Array(items.into_iter().map(wilded).collect()),
        Json::Object(fields) => Json::Object(fields.into_iter().map(|(key, value)| (key, wilded(value))).collect()),
        other => other,
    }
}

fn without_signal(slot: &Slot) -> Slot {
    let mut left = *slot;
    if let Some((_, pre)) = slot.wiring() {
        left.wire(None, pre);
    }
    left
}

#[test]
fn every_knob_of_every_effect_survives_a_save_but_the_signal_a_preset_leaves_behind() {
    for kind in Kind::ALL {
        let (packed, stuck) = packed(kind);
        assert!(stuck.is_empty(), "{}: knobs that would not turn: {stuck:?}", kind.label());
        assert!(packed != Slot::new(kind), "{}: packing every knob left the row at its defaults", kind.label());
        let (opened, wanted) = (reopened(&packed), without_signal(&packed));
        assert!(opened == wanted, "{}: a knob changed in the save, the preset leaving only the signal's source behind and keeping its PRE or POST\n  wanted {wanted:?}\n  opened {opened:?}", kind.label());
    }
}

#[test]
fn a_bare_kind_opens_at_every_default() {
    for kind in Kind::ALL {
        let bare = Json::Object(vec![(String::from("kind"), Json::String(String::from(kind.name())))]);
        let opened = Slot::from_json(&bare).expect("a bare kind opens");
        assert!(opened == Slot::new(kind), "{}: a row with nothing but its kind is not the default row\n  opened {opened:?}", kind.label());
    }
}

#[test]
fn unknown_keys_are_ignored() {
    for kind in Kind::ALL {
        let (packed, _) = packed(kind);
        let Json::Object(mut fields) = packed.to_json() else {
            panic!("{}: a saved row is not an object", kind.label());
        };
        fields.insert(0, (String::from("mystery"), Json::String(String::from("?"))));
        fields.push((String::from("nonsense"), Json::Number(1.0)));
        let opened = Slot::from_json(&Json::Object(fields)).expect("a row with extra keys opens");
        assert!(opened == packed, "{}: unknown keys changed the row, its signal now spelled with it\n  packed {packed:?}\n  opened {opened:?}", kind.label());
    }
}

#[test]
fn an_eq_solo_is_heard_not_saved() {
    let mut slot = Slot::new(Kind::Eq);
    slot.assign("BAND 8", "SOLO", 1.0);
    let Params::Eq(eq) = &slot.params else {
        panic!("an EQ row does not hold EQ params");
    };
    assert_eq!(eq.solo, Some(7), "pressing SOL on band 8 did not solo it");
    let opened = reopened(&slot);
    let Params::Eq(eq) = &opened.params else {
        panic!("a reopened EQ row does not hold EQ params");
    };
    assert_eq!(eq.solo, None, "a saved EQ opened with a band still soloed");
    assert!(opened == Slot::new(Kind::Eq), "a solo alone changed the row in the save\n  opened {opened:?}");
}

#[test]
fn a_wild_file_settles_on_its_first_save() {
    for kind in Kind::ALL {
        let (packed, _) = packed(kind);
        let once = Slot::from_json(&wilded(packed.to_json())).expect("a wild row opens");
        let (twice, settled) = (reopened(&once), without_signal(&once));
        assert!(twice == settled, "{}: a wild row keeps changing as it is saved and reopened, the preset leaving only the signal's source behind\n  once {settled:?}\n  twice {twice:?}", kind.label());
    }
}
