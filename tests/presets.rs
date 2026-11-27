use bp::arp::Arp;
use bp::core::Params;
use bp::fx::Kind;
use bp::json;
use bp::project::{Cores, Preset, RACK_ROWS, Row, settled_rack};
use support::{core, core_layer, effect, midi};

fn reread(preset: &Preset) -> Preset {
    let text = preset.to_json().write();
    let json = json::parse(&text).expect("a preset writes valid JSON");
    Preset::from_json(&json).expect("a written preset reads back")
}

#[test]
fn a_core_row_reads_back_as_written() {
    let mut patch = Params::DEFAULT;
    patch.filter.on = true;
    patch.filter.cutoff_hz = 1200.0;
    patch.filter.resonance = 0.25;
    patch.voices = 1;
    let preset = Preset::Row(Row::Core(core_layer(0.8, patch)), Vec::new());
    assert_eq!(reread(&preset), preset, "a Core row did not survive a save and load");
}

#[test]
fn a_rack_settles_to_the_ceiling_with_a_core() {
    let too_many = settled_rack(Cores::Required, (0..RACK_ROWS + 2).map(|_| effect(Kind::Delay)).collect());
    assert_eq!(too_many.len(), RACK_ROWS, "a rack over the ceiling keeps more than {RACK_ROWS} rows");
    assert!(too_many[0].is_midi() && too_many[1].is_core(), "a track rack loaded without a MIDI row or a Core did not get the MIDI row at row 0 and the Core at row 1, got {:?} then {:?}", too_many[0], too_many[1]);
    let many_cores = settled_rack(Cores::Required, (0..RACK_ROWS + 2).map(|_| core(1.0)).chain([effect(Kind::Eq)]).collect());
    let kept = many_cores.iter().filter(|row| row.is_core()).count();
    assert!(many_cores[0].is_midi() && kept == RACK_ROWS - 1, "a rack of Cores did not open with a MIDI row over {} Cores, got {:?} over {kept} Cores", RACK_ROWS - 1, many_cores[0]);
    assert_eq!(settled_rack(Cores::Refused, vec![midi(Arp::DEFAULT), core(1.0), effect(Kind::Motion)]), vec![effect(Kind::Motion)], "a send rack kept a MIDI row or a Core");
    assert_eq!(settled_rack(Cores::Required, Vec::new()), vec![midi(Arp::DEFAULT), core(1.0)], "an empty track rack did not get a MIDI row and a Core");
}
