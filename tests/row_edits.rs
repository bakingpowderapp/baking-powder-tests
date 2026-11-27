use bp::arp::{Arp, MidiRow};
use bp::fx::Kind;
use bp::project::{Aim, Host, LaneLfo, Node, Project, Row, TICKS_PER_BEAT};
use support::{CORE_ROW, LEVEL, core, effect, midi};

const TRACK: usize = 0;
const HOST: Host = Host::Track(TRACK);
const CORE: usize = CORE_ROW;
const REVERB: usize = CORE_ROW + 1;
const DELAY: usize = CORE_ROW + 2;
const MIDI: &str = MidiRow::LABEL;
const DELAY_KNOB: (&str, &str) = ("LOOP", "FEEDBACK");
const REVERB_KNOB: (&str, &str) = ("SPACE", "DECAY");
const NODE: Node = Node { tick: 2 * TICKS_PER_BEAT, travel: 0.25, bend: 0.0 };
const AMOUNT: f64 = 0.5;

fn racked() -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[TRACK].rack = vec![midi(Arp::DEFAULT), core(LEVEL), effect(Kind::Reverb), effect(Kind::Delay)];
    project.tracks[TRACK].automation.set_node(&Aim::knob(DELAY, DELAY_KNOB.0, DELAY_KNOB.1), None, NODE);
    let mut lfo = LaneLfo::blank(Aim::knob(REVERB, REVERB_KNOB.0, REVERB_KNOB.1));
    lfo.amount = AMOUNT;
    project.tracks[TRACK].automation.lfos.push(lfo);
    project
}

fn kinds(project: &Project) -> Vec<&'static str> {
    project.rack(HOST).iter().map(Row::label).collect()
}

fn curve_rows(project: &Project) -> Vec<usize> {
    project.tracks[TRACK].automation.curves.iter().filter(|curve| matches!(&curve.aim, Aim::Knob { band, name, .. } if (band.as_str(), name.as_str()) == DELAY_KNOB)).map(|curve| curve.aim.row_or_top()).collect()
}

fn lfo_rows(project: &Project) -> Vec<usize> {
    project.tracks[TRACK].automation.lfos.iter().filter(|lfo| matches!(&lfo.aim, Aim::Knob { band, name, .. } if (band.as_str(), name.as_str()) == REVERB_KNOB)).map(|lfo| lfo.aim.row_or_top()).collect()
}

fn reopened(project: &Project) -> Project {
    Project::from_json(&project.to_json()).expect("wanted the project to reopen from its own JSON")
}

#[test]
fn a_moved_row_takes_its_automation_and_lfo_with_it() {
    let mut project = racked();
    project.move_row(HOST, DELAY, REVERB);
    assert_eq!(kinds(&project), vec![MIDI, "Core", Kind::Delay.label(), Kind::Reverb.label()], "wanted the delay moved above the reverb");
    assert_eq!(curve_rows(&project), vec![REVERB], "wanted the delay's curve to follow it to row {REVERB}");
    assert_eq!(lfo_rows(&project), vec![DELAY], "wanted the reverb's LFO to follow it to row {DELAY}");
    let reopened = reopened(&project);
    assert_eq!((curve_rows(&reopened), lfo_rows(&reopened)), (vec![REVERB], vec![DELAY]), "wanted both lanes on the same rows after a save and reload");
}

#[test]
fn a_removed_row_takes_its_automation_and_lfo_away_and_the_rows_below_close_up() {
    let mut project = racked();
    project.remove_row(HOST, REVERB);
    assert_eq!(kinds(&project), vec![MIDI, "Core", Kind::Delay.label()], "wanted the reverb gone");
    assert_eq!(lfo_rows(&project), Vec::<usize>::new(), "wanted the reverb's LFO gone with it");
    assert_eq!(curve_rows(&project), vec![REVERB], "wanted the delay's curve to close up to row {REVERB}");
    assert_eq!(kinds(&racked()), vec![MIDI, "Core", Kind::Reverb.label(), Kind::Delay.label()], "wanted a fresh rack unchanged");
    let mut project = racked();
    project.remove_row(HOST, CORE);
    assert_eq!(kinds(&project), vec![MIDI, Kind::Reverb.label(), Kind::Delay.label()], "wanted the core gone and the MIDI row kept");
    assert_eq!((curve_rows(&project), lfo_rows(&project)), (vec![REVERB], vec![CORE]), "wanted both lanes to close up by one row");
    let reopened = reopened(&project);
    assert_eq!((curve_rows(&reopened), lfo_rows(&reopened)), (vec![REVERB + 1], vec![CORE + 1]), "wanted both lanes a row down after a reload put a Core back under the MIDI row");
}
