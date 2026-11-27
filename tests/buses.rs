use bp::app::{App, Event};
use bp::arp::{Arp, MidiRow};
use bp::division::Division;
use bp::draw::{DrawList, Rect, Tone};
use bp::face::{CELL, FRAME, H, PITCH};
use bp::fx::Kind;
use bp::json;
use bp::keyboard::Key;
use bp::project::{Aim, Bus, Clip, Cores, Node, Project, Row, TICKS_PER_BEAT, settled_rack};
use bp::rack::{Offer, Rack};
use support::{AUDIBLE, CHORD, GRIP_LINE, GRIP_LINE_INSET, HEIGHT, LEVEL, WIDTH, core, effect, face_rect, frames, half_wet, midi, mixed, painted, peak_of, remembered, song_render, worst_delta, unseeded};

const W: i32 = 200;
const SECONDS: f64 = 2.0;
const QUIET: f64 = 0.125;
const CLOSE: f32 = 1e-5;
const LEVELS: [f64; 2] = [0.5, 0.25];
const FULL: [f64; 2] = [1.0, 1.0];
const DRAG: i32 = 50;
const DRAGGED: f64 = 0.5;
const RACK_X: i32 = 6;
const RACK_SHORT_OF_FACE: i32 = 10;
const MASTER: usize = 0;
const TABS: usize = 0;
const MASTER_SLOT: usize = 1;
const LIST: usize = 2;
const FIFTH: i8 = 7;
const ARP_GATE: (&str, &str) = ("ARP", "GATE");
const OWN: f32 = 0.75;
const COPIED: f32 = 0.25;

fn bus(levels: [f64; 2]) -> Row {
    Row::Bus(Bus { levels, ..Bus::DEFAULT })
}

fn song(rack: Vec<Row>) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = rack;
    project.tracks[0].clips.push(Clip::chord(0, String::new(), &CHORD));
    project
}

fn heard(rack: Vec<Row>) -> [Vec<f32>; 2] {
    song_render(&song(rack), frames(SECONDS))
}

fn scaled(render: &[Vec<f32>; 2], by: f64) -> [Vec<f32>; 2] {
    [0, 1].map(|channel| render[channel].iter().map(|sample| sample * by as f32).collect())
}

fn apart(got: &[Vec<f32>; 2], wanted: &[Vec<f32>; 2]) -> f32 {
    worst_delta(&got[0], &wanted[0]).max(worst_delta(&got[1], &wanted[1]))
}

fn reread(project: &Project) -> Project {
    let text = project.to_json().write();
    Project::from_json(&json::parse(&text).expect("wanted the written project to parse")).expect("wanted the written project to read")
}

fn labels(rack: &[Row]) -> Vec<&'static str> {
    rack.iter().map(Row::label).collect()
}

fn offer() -> Offer {
    Offer { cores: Cores::Required, paste: false }
}

fn name_at(rack: &mut Rack, slot: usize, rows: &[Row]) -> Option<usize> {
    rack.name_at(W / 2, slot as i32 * PITCH + CELL / 2, rows, offer())
}

fn tab(bus: usize) -> (i32, i32) {
    (W / 4 + bus as i32 * W / 2, TABS as i32 * PITCH + CELL / 2)
}

fn press_tab(rack: &mut Rack, bus: usize, rows: &mut Vec<Row>) {
    let (x, y) = tab(bus);
    rack.down(x, y, 1, false, offer(), rows);
    rack.up(x, y, rows, offer());
}

fn shift_press(rack: &mut Rack, (x, y): (i32, i32), rows: &mut Vec<Row>) {
    rack.down(x, y, 1, true, offer(), rows);
    rack.up(x, y, rows, offer());
}

fn open(project: &Project) -> App {
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false}}", project.to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.draw(WIDTH, HEIGHT);
    app.take_requests();
    app
}

fn command(app: &mut App, letter: char) {
    app.event(Event::Key { key: Key::Char(letter), shift: false, command: true, repeat: false });
}

fn click(app: &mut App, (x, y): (i32, i32)) {
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    app.draw(WIDTH, HEIGHT);
}

fn rack_rect(app: &mut App) -> Rect {
    let face = face_rect(&painted(app));
    Rect::new(RACK_X, face.y, face.x - RACK_SHORT_OF_FACE, H)
}

fn second_tab(app: &mut App) -> (i32, i32) {
    let rack = rack_rect(app);
    (rack.x + 3 * rack.w / 4, rack.y + TABS as i32 * PITCH + CELL / 2)
}

fn name_of(app: &mut App, slot: usize) -> (i32, i32) {
    let rack = rack_rect(app);
    (rack.x + rack.w / 2, rack.y + slot as i32 * PITCH + CELL / 2)
}

fn gate_curves(project: &Project) -> Vec<(usize, f32)> {
    let mut found: Vec<(usize, f32)> = project.tracks[0].automation.curves.iter().map(|curve| (curve.aim.row_or_top(), curve.nodes[0].travel)).collect();
    found.sort_by_key(|(row, _)| *row);
    found
}

fn add(app: &mut App, name: &str) {
    command(app, 'f');
    for letter in name.chars() {
        app.event(Event::Key { key: Key::Char(letter), shift: false, command: false, repeat: false });
    }
    app.event(Event::Key { key: Key::Return, shift: false, command: false, repeat: false });
}

fn arped() -> Arp {
    Arp { on: true, rate: Division::Sixteenth, ..Arp::DEFAULT }
}

fn drawn(rack: &mut Rack, rows: &[Row]) -> DrawList {
    let mut list = DrawList::new();
    rack.draw(&mut list, rows, None, offer(), None, &[]);
    list
}

fn inner_lines() -> (Rect, Rect) {
    let y = MASTER_SLOT as i32 * PITCH + 2 * FRAME;
    (Rect::new(2 * FRAME, y, CELL - 3 * FRAME, FRAME), Rect::new(PITCH + FRAME, y, W - 2 * PITCH - 2 * FRAME, FRAME))
}

#[test]
fn each_bus_runs_its_own_effects_and_the_two_are_summed() {
    let bused = heard(vec![midi(Arp::DEFAULT), core(LEVEL), bus(FULL), core(QUIET), half_wet(Kind::Delay)]);
    let first = heard(vec![midi(Arp::DEFAULT), core(LEVEL)]);
    let second = heard(vec![midi(Arp::DEFAULT), core(QUIET), half_wet(Kind::Delay)]);
    assert!(peak_of(&bused[0]) > AUDIBLE, "wanted the bused rack heard, got a peak of {}", peak_of(&bused[0]));
    let off = apart(&bused, &mixed(&first, &second));
    assert!(off < CLOSE, "wanted two buses to sound as the two racks summed, the delay on the second alone, got {off} apart");
    let serial = heard(vec![midi(Arp::DEFAULT), core(LEVEL), core(QUIET), half_wet(Kind::Delay)]);
    let shared = apart(&bused, &serial);
    assert!(shared > AUDIBLE, "wanted the bused rack to differ from one whose delay hears both Cores, got {shared} apart");
}

#[test]
fn a_bus_level_scales_its_own_bus_alone() {
    let bused = heard(vec![midi(Arp::DEFAULT), core(LEVEL), bus(LEVELS), core(QUIET), half_wet(Kind::Delay)]);
    let first = scaled(&heard(vec![midi(Arp::DEFAULT), core(LEVEL)]), LEVELS[0]);
    let second = scaled(&heard(vec![midi(Arp::DEFAULT), core(QUIET), half_wet(Kind::Delay)]), LEVELS[1]);
    let off = apart(&bused, &mixed(&first, &second));
    assert!(off < CLOSE, "wanted bus 1 at {} and bus 2 at {} of their racks' levels, got {off} apart", LEVELS[0], LEVELS[1]);
}

#[test]
fn a_rack_without_a_bus_row_plays_as_it_did_with_one_at_full_level_after_it() {
    let plain = heard(vec![midi(Arp::DEFAULT), core(LEVEL), half_wet(Kind::Delay)]);
    let trailing = heard(vec![midi(Arp::DEFAULT), core(LEVEL), half_wet(Kind::Delay), bus(FULL)]);
    let off = apart(&trailing, &plain);
    assert!(off < CLOSE, "wanted an empty second bus at full levels to change nothing, got {off} apart");
}

#[test]
fn a_bus_row_is_written_and_read_back() {
    let row = bus(LEVELS);
    let written = row.to_json();
    assert_eq!(written.get("kind").and_then(json::Json::as_str), Some(Bus::KIND), "wanted the row written under the kind bus, got {}", written.write());
    assert_eq!(Row::from_json(&written), Some(row), "the bus row changed in the save");
    let project = song(vec![midi(Arp::DEFAULT), core(LEVEL), bus(LEVELS), core(QUIET), effect(Kind::Reverb)]);
    assert_eq!(unseeded(&reread(&project).tracks[0].rack), project.tracks[0].rack, "wanted a bused rack read back as written");
}

#[test]
fn a_rack_keeps_one_bus_row_and_drops_a_spare_one_at_its_end() {
    let spare = song(vec![midi(Arp::DEFAULT), core(LEVEL), bus(FULL)]);
    assert_eq!(labels(&reread(&spare).tracks[0].rack), [MidiRow::LABEL, "Core"], "wanted a bus row at full levels with nothing under it dropped");
    let lowered = song(vec![midi(Arp::DEFAULT), core(LEVEL), bus(LEVELS)]);
    assert_eq!(unseeded(&reread(&lowered).tracks[0].rack), lowered.tracks[0].rack, "wanted a bus row that holds a level kept though nothing sits under it");
    let twice = song(vec![midi(Arp::DEFAULT), core(LEVEL), bus(LEVELS), core(QUIET), bus(FULL), core(LEVEL)]);
    assert_eq!(unseeded(&reread(&twice).tracks[0].rack), vec![midi(Arp::DEFAULT), core(LEVEL), bus(LEVELS), core(QUIET), core(LEVEL)], "wanted the second bus row dropped and its rows left on bus 2");
}

#[test]
fn audio_group_return_and_master_racks_refuse_a_bus_row() {
    let text = r#"{"kind": "project", "tracks": [
            {"name": "take", "kind": "audio", "rack": [{"kind": "eq"}, {"kind": "bus", "levels": [0.5, 0.5]}, {"kind": "delay"}]},
            {"name": "sum", "kind": "group", "rack": [{"kind": "bus"}, {"kind": "utility"}]},
            {"name": "keys", "rack": [{"kind": "midi"}, {"kind": "core"}, {"kind": "bus", "levels": [0.5, 0.5]}, {"kind": "core"}]}
        ],
        "returns": [{"rack": [{"kind": "reverb"}, {"kind": "bus", "levels": [0.5, 0.5]}]}],
        "master": {"rack": [{"kind": "bus", "levels": [0.5, 0.5]}, {"kind": "compressor"}]}}"#;
    let project = Project::from_json(&json::parse(text).expect("wanted the project text to parse")).expect("wanted the project to read");
    let racks = [labels(&project.tracks[0].rack), labels(&project.tracks[1].rack), labels(&project.sends[0].rack), labels(&project.master.rack)];
    let wanted = [vec![Kind::Eq.label(), Kind::Delay.label()], vec![Kind::Utility.label()], vec![Kind::Reverb.label()], vec![Kind::Compressor.label()]];
    assert_eq!(racks, wanted, "wanted the audio, group, return and master racks read without their bus rows");
    assert_eq!(labels(&project.tracks[2].rack), [MidiRow::LABEL, "Core", "Bus", "Core"], "wanted the MIDI track to keep its bus row");
    let refused = settled_rack(Cores::Refused, vec![effect(Kind::Eq), bus(LEVELS), effect(Kind::Delay)]);
    assert_eq!(refused, vec![effect(Kind::Eq), effect(Kind::Delay)], "wanted a rack that refuses Cores to settle without its bus row");
}

#[test]
fn a_midi_rack_lists_one_bus_at_a_time_under_two_tabs() {
    let mut rows = vec![midi(Arp::DEFAULT), core(LEVEL), effect(Kind::Reverb), bus(FULL), core(QUIET), effect(Kind::Delay)];
    let mut rack = Rack::new(W);
    assert_eq!(name_at(&mut rack, TABS, &rows), None, "wanted the first slot kept for the tabs, no row under it");
    assert_eq!(name_at(&mut rack, MASTER_SLOT, &rows), Some(0), "wanted the first MIDI row in the second slot, beneath the tabs");
    let first: Vec<Option<usize>> = (LIST..LIST + 3).map(|slot| name_at(&mut rack, slot, &rows)).collect();
    assert_eq!(first, [Some(1), Some(2), None], "wanted the Core and the reverb listed under tab 1, the first MIDI row and the bus row left out");
    press_tab(&mut rack, 1, &mut rows);
    assert_eq!(name_at(&mut rack, MASTER_SLOT, &rows), Some(0), "wanted the first MIDI row kept beneath the tabs with tab 2 picked");
    let second: Vec<Option<usize>> = (LIST..LIST + 3).map(|slot| name_at(&mut rack, slot, &rows)).collect();
    assert_eq!(second, [Some(4), Some(5), None], "wanted the second Core and the delay listed under tab 2");
    press_tab(&mut rack, 0, &mut rows);
    assert_eq!(name_at(&mut rack, LIST, &rows), Some(1), "wanted tab 1 pressed again to list bus 1");
    assert_eq!(rows.len(), 6, "wanted a pressed tab to change no row, got {} rows", rows.len());
    let plain = vec![effect(Kind::Eq), effect(Kind::Utility)];
    assert_eq!(name_at(&mut Rack::new(W), 0, &plain), Some(0), "wanted a rack with no MIDI row to list from its first slot, with no tabs");
}

#[test]
fn a_dragged_tab_sets_its_bus_level_and_a_second_press_restores_it() {
    let mut rows = vec![midi(Arp::DEFAULT), core(LEVEL)];
    let mut rack = Rack::new(W);
    let (x, y) = tab(0);
    rack.down(x, y, 1, false, offer(), &mut rows);
    assert!(rack.moved(x, y + DRAG, offer(), &mut rows, false), "a tab dragged down changed nothing");
    rack.up(x, y + DRAG, &rows, offer());
    assert_eq!(rows, vec![midi(Arp::DEFAULT), core(LEVEL), bus([DRAGGED, 1.0])], "wanted tab 1 dragged {DRAG} down to hold bus 1 at {DRAGGED} in a bus row");
    let (changed, action) = rack.down(x, y, 2, false, offer(), &mut rows);
    assert!(changed && action.is_none(), "a second press on a lowered tab changed nothing");
    assert_eq!(rows, vec![midi(Arp::DEFAULT), core(LEVEL)], "wanted a second press to restore full level and the spare bus row to go");
}

#[test]
fn rows_pasted_under_the_second_tab_land_on_bus_two() {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), core(LEVEL), effect(Kind::Reverb)];
    let mut app = open(&project);
    command(&mut app, 'a');
    command(&mut app, 'c');
    let at = second_tab(&mut app);
    click(&mut app, at);
    command(&mut app, 'v');
    let rack = remembered(&mut app).tracks[0].rack.clone();
    assert_eq!(labels(&rack), [MidiRow::LABEL, "Core", Kind::Reverb.label(), "Bus", MidiRow::LABEL, "Core", Kind::Reverb.label()], "wanted the three copied rows pasted under a new bus row, on bus 2");
}

#[test]
fn a_midi_row_inside_a_bus_plays_that_buss_cores_and_none_of_the_other_buss() {
    let (first, second) = (core(LEVEL), core(QUIET));
    let arped_first = heard(vec![midi(Arp::DEFAULT), midi(arped()), first]);
    let arped_second = heard(vec![midi(Arp::DEFAULT), midi(arped()), second]);
    let plain_first = heard(vec![midi(Arp::DEFAULT), first]);
    let plain_second = heard(vec![midi(Arp::DEFAULT), second]);
    let in_one = heard(vec![midi(Arp::DEFAULT), midi(arped()), first, bus(FULL), second]);
    assert!(peak_of(&in_one[0]) > AUDIBLE, "wanted the bused rack heard, got a peak of {}", peak_of(&in_one[0]));
    let off = apart(&in_one, &mixed(&arped_first, &plain_second));
    assert!(off < CLOSE, "wanted a MIDI row in bus 1 to arp bus 1's Core alone, bus 2's Core playing what the first MIDI row plays, got {off} apart");
    let carried = apart(&in_one, &mixed(&arped_first, &arped_second));
    assert!(carried > AUDIBLE, "wanted the rack to differ from one whose bus 1 arp plays on into bus 2, got {carried} apart");
    let in_two = heard(vec![midi(Arp::DEFAULT), first, bus(FULL), midi(arped()), second]);
    let off = apart(&in_two, &mixed(&plain_first, &arped_second));
    assert!(off < CLOSE, "wanted a MIDI row in bus 2 to arp bus 2's Core alone, bus 1's Core playing what the first MIDI row plays, got {off} apart");
}

#[test]
fn the_first_midi_row_feeds_both_buses_whatever_its_times() {
    let master = |times: usize| Row::Midi(MidiRow { times, transpose: FIFTH, ..MidiRow::DEFAULT });
    let bused = heard(vec![master(2), core(LEVEL), bus(FULL), core(QUIET)]);
    let each = mixed(&heard(vec![master(2), core(LEVEL)]), &heard(vec![master(2), core(QUIET)]));
    assert!(peak_of(&bused[0]) > AUDIBLE, "wanted the bused rack heard, got a peak of {}", peak_of(&bused[0]));
    let off = apart(&bused, &each);
    assert!(off < CLOSE, "wanted both buses to play what the first MIDI row at 2 plays, moved up twice, got {off} apart");
    let once = mixed(&heard(vec![master(2), core(LEVEL)]), &heard(vec![master(1), core(QUIET)]));
    let short = apart(&bused, &once);
    assert!(short > AUDIBLE, "wanted bus 2 to hear the first MIDI row's last copy and not its first alone, got {short} apart");
}

#[test]
fn a_midi_row_added_from_the_add_box_lands_first_in_the_bus_that_is_showing() {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), core(LEVEL), effect(Kind::Reverb)];
    let mut app = open(&project);
    add(&mut app, "midi");
    assert_eq!(labels(&remembered(&mut app).tracks[0].rack), [MidiRow::LABEL, MidiRow::LABEL, "Core", Kind::Reverb.label()], "wanted the new MIDI row first in bus 1, above its Core");
    add(&mut app, "delay");
    assert_eq!(labels(&remembered(&mut app).tracks[0].rack), [MidiRow::LABEL, MidiRow::LABEL, "Core", Kind::Reverb.label(), Kind::Delay.label()], "wanted a new effect at the foot of bus 1, where it always landed");
    let at = second_tab(&mut app);
    click(&mut app, at);
    add(&mut app, "core");
    add(&mut app, "midi");
    let rack = remembered(&mut app).tracks[0].rack.clone();
    assert_eq!(labels(&rack), [MidiRow::LABEL, MidiRow::LABEL, "Core", Kind::Reverb.label(), Kind::Delay.label(), "Bus", MidiRow::LABEL, "Core"], "wanted the Core added under tab 2 on bus 2 and the MIDI row added after it first in bus 2, above that Core");
}

#[test]
fn a_copied_midi_row_pasted_with_the_first_midi_row_picked_takes_its_place_settings_and_lanes() {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), midi(arped()), core(LEVEL)];
    let node = |travel: f32| Node { tick: TICKS_PER_BEAT, travel, bend: 0.0 };
    project.tracks[0].automation.set_node(&Aim::knob(MASTER, ARP_GATE.0, ARP_GATE.1), None, node(OWN));
    project.tracks[0].automation.set_node(&Aim::knob(1, ARP_GATE.0, ARP_GATE.1), None, node(COPIED));
    let mut app = open(&project);
    let lower = name_of(&mut app, LIST);
    click(&mut app, lower);
    command(&mut app, 'c');
    let first = name_of(&mut app, MASTER_SLOT);
    click(&mut app, first);
    app.take_requests();
    command(&mut app, 'v');
    let pasted = remembered(&mut app);
    assert_eq!(unseeded(&pasted.tracks[0].rack), vec![midi(arped()), midi(arped()), core(LEVEL)], "wanted the copied MIDI row's settings on the first MIDI row and no row added, the track keeping one first MIDI row");
    assert_eq!(gate_curves(&pasted), [(MASTER, COPIED), (1, COPIED)], "wanted the copied row's curve in place of the first MIDI row's own");
    command(&mut app, 'v');
    let again = remembered(&mut app);
    assert_eq!(unseeded(&again.tracks[0].rack), vec![midi(arped()), midi(arped()), midi(arped()), core(LEVEL)], "wanted the same row pasted again, the two now alike, to land first in the bus that is showing");
    assert_eq!(gate_curves(&again), [(MASTER, COPIED), (1, COPIED), (2, COPIED)], "wanted the row pasted into the bus to bring its curve, and the row it pushed down to keep its own");
}

#[test]
fn picking_a_row_of_the_other_bus_turns_the_rack_to_its_tab() {
    let mut rows = vec![midi(Arp::DEFAULT), core(LEVEL), bus(FULL), core(QUIET)];
    let mut rack = Rack::new(W);
    press_tab(&mut rack, 1, &mut rows);
    assert_eq!(rack.bus(), 1, "wanted tab 2 showing after its press");
    rack.pick(&rows, vec![MASTER, 1]);
    assert_eq!((rack.bus(), name_at(&mut rack, LIST, &rows)), (0, Some(1)), "wanted the rack turned to bus 1 when the first MIDI row and a row of bus 1 are picked, so the picked Core is in sight");
    rack.pick(&rows, vec![MASTER]);
    assert_eq!(rack.bus(), 0, "wanted the first MIDI row picked alone to leave the tab as it was");
}

#[test]
fn the_first_midi_row_is_drawn_beneath_the_tabs_framed_alone_as_every_other_row() {
    let mut rows = vec![midi(Arp::DEFAULT), core(LEVEL), effect(Kind::Reverb)];
    let mut rack = Rack::new(W);
    let (handle, name) = inner_lines();
    let lined = |list: &DrawList| list.fills().iter().any(|fill| fill.rect == handle || fill.rect == name);
    let resting = drawn(&mut rack, &rows);
    assert!(!lined(&resting), "wanted no second line inside the first MIDI row's outline, its boxes framed alone as every other row's are");
    let (x, y) = (CELL / 2, MASTER_SLOT as i32 * PITCH + CELL / 2);
    shift_press(&mut rack, (x, y), &mut rows);
    let picked = drawn(&mut rack, &rows);
    assert_ne!(picked.fills(), resting.fills(), "wanted the SHIFT-press to pick the first MIDI row and light its handle");
    assert!(!lined(&picked), "wanted no second line inside the picked first MIDI row's outline either");
}

#[test]
fn a_rows_handle_shows_its_lines_at_one_and_no_lines_above_one() {
    let lines = |slot: i32| Rect::new(GRIP_LINE_INSET.0, slot * PITCH + GRIP_LINE_INSET.1, GRIP_LINE.0, GRIP_LINE.1);
    let mut twice = effect(Kind::Utility);
    twice.set_times(2);
    let rows = vec![effect(Kind::Eq), twice];
    let list = drawn(&mut Rack::new(W), &rows);
    let inked = |rect: Rect| list.fills().iter().any(|fill| fill.rect == rect && fill.tone == Tone::Ink100);
    assert!(inked(lines(0)), "wanted the three lines on the handle of a row at 1");
    assert!(!inked(lines(1)), "wanted the handle of a row at 2 to show its number in place of the lines");
}
