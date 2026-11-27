use bp::app::{App, Event};
use bp::arp::{Arp, MidiRow};
use bp::division::Division;
use bp::face::{CELL, PITCH, READOUT_ROWS};
use bp::fx::Kind;
use bp::project::{Aim, Clip, LaneLfo, Node, Project, Row, TICKS_PER_BEAT};
use support::{CORE_ROW, LEVEL, core, effect, face_rect, midi, painted, remembered};

const FLAT_BUTTON: (i32, i32) = (5, 12);
const DECAY: (&str, &str) = ("SPACE", "DECAY");
const OSC_LEVEL: (&str, &str) = ("OSC 1", "LEVEL");
const TRAVEL: f32 = 0.25;
const UPPER_AMOUNT: f64 = 0.5;
const LOWER_AMOUNT: f64 = 0.75;
const SECOND_MIDI: usize = 2;
const SECOND_CORE: usize = 3;
const REVERB: usize = 4;
const SEGMENT: usize = 2;

fn project() -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp { on: true, rate: Division::Sixteenth, ..Arp::DEFAULT }), core(LEVEL), midi(Arp { on: true, rate: Division::Eighth, ..Arp::DEFAULT }), core(LEVEL), effect(Kind::Reverb)];
    track.clips.push(Clip::chord(0, String::from("chord"), &[60, 64, 67]));
    track.automation.set_node(&Aim::knob(REVERB, DECAY.0, DECAY.1), None, Node { tick: TICKS_PER_BEAT, travel: TRAVEL, bend: 0.0 });
    for (row, amount) in [(CORE_ROW, UPPER_AMOUNT), (SECOND_CORE, LOWER_AMOUNT)] {
        let mut lfo = LaneLfo::blank(Aim::knob(row, OSC_LEVEL.0, OSC_LEVEL.1));
        lfo.amount = amount;
        track.automation.lfos.push(lfo);
    }
    project
}

fn open() -> App {
    let session = format!("{{\"project\":{},\"open\":{SECOND_MIDI},\"keyboard\":false}}", project().to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    painted(&mut app);
    app.take_requests();
    app
}

fn press_cell(app: &mut App, (row, column): (i32, i32)) {
    let face = face_rect(&painted(app));
    let (x, y) = (face.x + column * PITCH + CELL / 2, face.y + (READOUT_ROWS + row) * PITCH + CELL / 2);
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
}

fn kinds(project: &Project, track: usize) -> Vec<&'static str> {
    project.tracks[track].rack.iter().map(Row::label).collect()
}

fn decay_rows(project: &Project, track: usize) -> Vec<(usize, f32)> {
    project.tracks[track].automation.curves.iter().filter(|curve| matches!(&curve.aim, Aim::Knob { band, name, .. } if (band.as_str(), name.as_str()) == DECAY)).map(|curve| (curve.aim.row_or_top(), curve.nodes[0].travel)).collect()
}

fn lfo_rows(project: &Project, track: usize) -> Vec<(usize, f64)> {
    project.tracks[track].automation.lfos.iter().map(|lfo| (lfo.aim.row_or_top(), lfo.amount)).collect()
}

#[test]
fn flattening_the_lower_midi_row_of_two_keeps_the_upper_rows_lanes_and_carries_its_own_and_the_reverbs_to_the_split() {
    let mut app = open();
    press_cell(&mut app, FLAT_BUTTON);
    let project = remembered(&mut app);
    let (midi_row, reverb) = (MidiRow::LABEL, Kind::Reverb.label());
    assert_eq!(project.tracks.len(), 2, "wanted the flattened MIDI row's segment split onto a track of its own");
    assert_eq!(kinds(&project, 0), vec![midi_row, "Core", reverb], "wanted the first MIDI row, its Core and the reverb left on the first track");
    assert_eq!(decay_rows(&project, 0), vec![(REVERB - SEGMENT, TRAVEL)], "wanted the reverb's decay curve to follow it up past the segment that left");
    assert_eq!(lfo_rows(&project, 0), vec![(CORE_ROW, UPPER_AMOUNT)], "wanted the first Core's LFO left where it was and the second Core's gone with its row");
    assert_eq!(kinds(&project, 1), vec![midi_row, "Core", reverb], "wanted the flattened MIDI row, its Core and a copy of the reverb on the split track");
    assert_eq!(decay_rows(&project, 1), vec![(SEGMENT, TRAVEL)], "wanted the reverb copy's decay curve carried to the split track");
    assert_eq!(lfo_rows(&project, 1), vec![(SECOND_CORE - SEGMENT, LOWER_AMOUNT)], "wanted the second Core's LFO carried to the split track with its row");
    assert!(matches!(project.tracks[1].rack[0], Row::Midi(row) if !row.arp.on), "wanted the flattened MIDI row's arp off, got {:?}", project.tracks[1].rack[0]);
}
