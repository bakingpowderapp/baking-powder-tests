use bp::arp::{Arp, MidiRow};
use bp::fx::Kind;
use bp::json;
use bp::project::{Aim, LaneLfo, Node, Preset, Project, Row, Sound, TICKS_PER_BEAT};
use support::{LEVEL, core, effect, midi};

const TRACK: usize = 0;
const DELAY: usize = 2;
const FIRST_REVERB: usize = 3;
const SECOND_REVERB: usize = 4;
const DELAY_KNOB: (&str, &str) = ("LOOP", "FEEDBACK");
const DECAY: (&str, &str) = ("SPACE", "DECAY");
const SIZE: (&str, &str) = ("SPACE", "SIZE");
const FIRST_TRAVEL: f32 = 0.25;
const SECOND_TRAVEL: f32 = 0.75;
const AMOUNT: f64 = 0.5;
const DELAY_KIND: &str = "\"delay\"";
const GONE_KIND: &str = "\"tape\"";

fn node(travel: f32) -> Node {
    Node { tick: 2 * TICKS_PER_BEAT, travel, bend: 0.0 }
}

fn lfo(row: usize) -> LaneLfo {
    let mut lfo = LaneLfo::blank(Aim::knob(row, SIZE.0, SIZE.1));
    lfo.amount = AMOUNT;
    lfo
}

fn racked() -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[TRACK];
    track.rack = vec![midi(Arp::DEFAULT), core(LEVEL), effect(Kind::Delay), effect(Kind::Reverb), effect(Kind::Reverb)];
    track.automation.set_node(&Aim::knob(DELAY, DELAY_KNOB.0, DELAY_KNOB.1), None, node(0.5));
    track.automation.set_node(&Aim::knob(FIRST_REVERB, DECAY.0, DECAY.1), None, node(FIRST_TRAVEL));
    track.automation.set_node(&Aim::knob(SECOND_REVERB, DECAY.0, DECAY.1), None, node(SECOND_TRAVEL));
    track.automation.lfos.push(lfo(SECOND_REVERB));
    project
}

fn without_the_delay(text: &str) -> String {
    assert_eq!(text.matches(DELAY_KIND).count(), 1, "wanted the delay row spelled once in the saved JSON");
    text.replace(DELAY_KIND, GONE_KIND)
}

fn reopened(text: &str) -> Project {
    Project::from_json(&json::parse(text).expect("wanted the project's JSON to parse")).expect("wanted the project to open")
}

fn kinds(project: &Project) -> Vec<&'static str> {
    project.tracks[TRACK].rack.iter().map(Row::label).collect()
}

fn decays(project: &Project) -> Vec<(usize, f32)> {
    project.tracks[TRACK].automation.curves.iter().filter(|curve| matches!(&curve.aim, Aim::Knob { band, name, .. } if (band.as_str(), name.as_str()) == DECAY)).map(|curve| (curve.aim.row_or_top(), curve.nodes[0].travel)).collect()
}

fn delay_curves(project: &Project) -> usize {
    project.tracks[TRACK].automation.curves.iter().filter(|curve| matches!(&curve.aim, Aim::Knob { band, name, .. } if (band.as_str(), name.as_str()) == DELAY_KNOB)).count()
}

fn lfo_rows(project: &Project) -> Vec<usize> {
    project.tracks[TRACK].automation.lfos.iter().map(|lfo| lfo.aim.row_or_top()).collect()
}

#[test]
fn lanes_past_a_row_whose_kind_is_gone_land_on_the_rows_they_named() {
    let project = reopened(&without_the_delay(&racked().to_json().write()));
    assert_eq!(kinds(&project), vec![MidiRow::LABEL, "Core", Kind::Reverb.label(), Kind::Reverb.label()], "wanted the unknown row dropped and the reverbs closed up");
    assert_eq!(decays(&project), vec![(FIRST_REVERB - 1, FIRST_TRAVEL), (SECOND_REVERB - 1, SECOND_TRAVEL)], "wanted each reverb's decay curve on its own reverb, a row up");
    assert_eq!(delay_curves(&project), 0, "wanted the dropped row's curve gone rather than landed on a reverb");
    assert_eq!(lfo_rows(&project), vec![SECOND_REVERB - 1], "wanted the LFO on the second reverb, a row up");
    let saved_again = reopened(&project.to_json().write());
    assert_eq!((decays(&saved_again), lfo_rows(&saved_again)), (decays(&project), lfo_rows(&project)), "wanted a second save and reload to change nothing");
}

#[test]
fn a_rack_preset_missing_a_row_lands_its_lfo_on_the_row_it_named() {
    let preset = Preset::Rack(Sound { rack: vec![midi(Arp::DEFAULT), core(LEVEL), effect(Kind::Delay), effect(Kind::Reverb)], lfos: vec![lfo(FIRST_REVERB)] });
    let text = without_the_delay(&preset.to_json().write());
    let Some(Preset::Rack(sound)) = Preset::from_json(&json::parse(&text).expect("wanted the preset's JSON to parse")) else {
        panic!("wanted a rack preset to open");
    };
    assert_eq!(sound.rack.len(), 3, "wanted the unknown row dropped from the preset's rack, got {} rows", sound.rack.len());
    let rows: Vec<usize> = sound.lfos.iter().map(|lfo| lfo.aim.row_or_top()).collect();
    assert_eq!(rows, vec![FIRST_REVERB - 1], "wanted the preset's LFO on its reverb, a row up");
}

#[test]
fn a_track_written_without_a_core_keeps_its_lane_on_the_effect_the_core_goes_above() {
    let text = r#"{"kind":"project","tracks":[{"name":"written","rack":[{"kind":"reverb"}],"curves":[{"row":0,"param":"wet","nodes":[{"beat":0,"value":0.25}]}]}]}"#;
    let project = reopened(text);
    assert_eq!(kinds(&project), vec![MidiRow::LABEL, "Core", Kind::Reverb.label()], "wanted a MIDI row and a Core put above the reverb");
    let lanes: Vec<Aim> = project.tracks[TRACK].automation.curves.iter().map(|curve| curve.aim.clone()).collect();
    assert_eq!(lanes, vec![Aim::Wet(2)], "wanted the wet curve on the reverb, now row 2");
}
