use bp::app::{App, Event};
use bp::arp::Arp;
use bp::fx::Kind;
use bp::keyboard::Key;
use bp::project::{Aim, LaneLfo, Node, Project, TICKS_PER_BEAT};
use support::{LEVEL, core, effect, midi, remembered, unseeded};

const TRACK: usize = 0;
const DELAY: usize = 2;
const DELAY_UNDER_THE_PASTED_MIDI_ROW: usize = 3;
const PASTED_DELAY: usize = 5;
const BAND: &str = "LOOP";
const PARAM: &str = "FEEDBACK";
const NODE: Node = Node { tick: 2 * TICKS_PER_BEAT, travel: 0.25, bend: 0.0 };
const AMOUNT: f64 = 0.5;

fn racked(lock: bool) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.lock = lock;
    project.tracks[TRACK].rack = vec![midi(Arp::DEFAULT), core(LEVEL), effect(Kind::Delay)];
    project.tracks[TRACK].automation.set_node(&Aim::knob(DELAY, BAND, PARAM), None, NODE);
    let mut lfo = LaneLfo::blank(Aim::knob(DELAY, BAND, PARAM));
    lfo.amount = AMOUNT;
    project.tracks[TRACK].automation.lfos.push(lfo);
    project
}

fn open(project: &Project) -> App {
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false}}", project.to_json().write());
    let mut app = App::new();
    app.resume(&session);
    app.take_requests();
    app
}

fn command(app: &mut App, letter: char) {
    app.event(Event::Key { key: Key::Char(letter), shift: false, command: true, repeat: false });
}

fn copied_and_pasted(lock: bool) -> Project {
    let mut app = open(&racked(lock));
    command(&mut app, 'a');
    command(&mut app, 'c');
    command(&mut app, 'v');
    remembered(&mut app)
}

fn curve_rows(project: &Project) -> Vec<usize> {
    let mut rows: Vec<usize> = project.tracks[TRACK].automation.curves.iter().filter(|curve| matches!(&curve.aim, Aim::Knob { band, name, .. } if band == BAND && name == PARAM)).map(|curve| curve.aim.row_or_top()).collect();
    rows.sort_unstable();
    rows
}

fn lfo_rows(project: &Project) -> Vec<(usize, f64)> {
    let mut rows: Vec<(usize, f64)> = project.tracks[TRACK].automation.lfos.iter().filter(|lfo| matches!(&lfo.aim, Aim::Knob { band, name, .. } if band == BAND && name == PARAM)).map(|lfo| (lfo.aim.row_or_top(), lfo.amount)).collect();
    rows.sort_by_key(|row| row.0);
    rows
}

#[test]
fn a_pasted_effect_brings_its_automation_and_lfo_along() {
    let project = copied_and_pasted(false);
    assert_eq!(unseeded(&project.tracks[TRACK].rack), vec![midi(Arp::DEFAULT), midi(Arp::DEFAULT), core(LEVEL), effect(Kind::Delay), core(LEVEL), effect(Kind::Delay)], "wanted the copied MIDI row pasted first in the bus, under the track's own, and the Core and delay pasted at the foot");
    assert_eq!(curve_rows(&project), vec![DELAY_UNDER_THE_PASTED_MIDI_ROW, PASTED_DELAY], "wanted the delay's feedback curve on both the original delay, a row down, and the pasted delay");
    let node = project.tracks[TRACK].automation.curves.iter().find(|curve| matches!(&curve.aim, Aim::Knob { row, band, .. } if *row == PASTED_DELAY && band == BAND)).map(|curve| curve.nodes.clone());
    assert_eq!(node, Some(vec![NODE]), "wanted the pasted curve to hold the same node");
    assert_eq!(lfo_rows(&project), vec![(DELAY_UNDER_THE_PASTED_MIDI_ROW, AMOUNT), (PASTED_DELAY, AMOUNT)], "wanted the delay's LFO lane on both delays at its amount");
}

#[test]
fn lock_keeps_the_automation_off_a_pasted_effect() {
    let project = copied_and_pasted(true);
    assert_eq!(project.tracks[TRACK].rack.len(), 6, "wanted the rows still pasted under LOCK, got {} rows", project.tracks[TRACK].rack.len());
    assert_eq!(curve_rows(&project), vec![DELAY_UNDER_THE_PASTED_MIDI_ROW], "wanted only the original delay's curve under LOCK, a row down with its delay");
    assert_eq!(lfo_rows(&project), vec![(DELAY_UNDER_THE_PASTED_MIDI_ROW, AMOUNT)], "wanted only the original delay's LFO lane under LOCK, a row down with its delay");
}
