use bp::app::{App, Event, Request};
use bp::arp::{Arp, MidiRow, Style};
use bp::browser::{Found, Item};
use bp::division::Division;
use bp::draw::Rect;
use bp::face::{self, CELL, FRAME, PITCH, W};
use bp::fx::Kind;
use bp::json;
use bp::project::{Aim, LaneLfo, Preset, Project, Row, Sound, Track};
use support::{CORE_ROW, HEIGHT, WIDTH, browser_rect, centre, core, effect, face_rect, midi, painted, remembered, unseeded};

const ROOT: &str = "/x/Sounds";
const BORDER: i32 = 8;
const SAVE_SLOT: i32 = 1;
const MIDI_ROW: usize = 0;
const GATE_SWING: f64 = 0.3;

fn kept() -> Arp {
    Arp { on: true, style: Style::Down, rate: Division::Sixteenth, gate: 0.4, swing: 0.25, ..Arp::DEFAULT }
}

fn carried() -> Arp {
    Arp { on: true, style: Style::Random, rate: Division::Twelfth, gate: 0.8, steps: 3, ..Arp::DEFAULT }
}

fn open(project: &Project, open: usize) -> App {
    let session = format!("{{\"project\":{},\"selected\":0,\"open\":{open},\"aside\":\"browser\",\"keyboard\":false,\"palette\":false,\"folders\":[\"{ROOT}\"],\"unfolded\":[\"{ROOT}\"]}}", project.to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    painted(&mut app);
    app.take_requests();
    app
}

fn racked(rack: Vec<Row>) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = rack;
    project
}

fn sound(rack: Vec<Row>) -> Preset {
    Preset::Rack(Track { rack, ..Track::blank() }.sound())
}

fn file(name: &str, preset: &Preset) -> Found {
    Found { path: format!("{ROOT}/{name}.json"), name: format!("{name}.json"), item: Item::read(&preset.to_json().write()) }
}

fn click(app: &mut App, rect: Rect) {
    let (x, y) = centre(rect);
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    painted(app);
}

fn loaded(app: &mut App, preset: &Preset) -> Project {
    app.listed(ROOT, vec![file("Pressed", preset)]);
    let browser = browser_rect(&painted(app));
    let mark = Rect::new(browser.x + PITCH, browser.y + PITCH, CELL, CELL);
    click(app, Rect::new(mark.x + PITCH, mark.y, browser.x + browser.w - mark.x - PITCH, CELL));
    remembered(app)
}

fn saved(app: &mut App, labels: &[&str], option: usize) -> (String, Preset) {
    let face = face_rect(&painted(app));
    let save = Rect::new(face.x + W + BORDER - 2 * FRAME, face.y + SAVE_SLOT * PITCH, CELL, CELL);
    click(app, save);
    click(app, face::option_rect(face::panel_rect(save, labels, 0, Rect::new(0, 0, WIDTH, HEIGHT)), option));
    let written: Vec<(String, String)> = app.take_requests().into_iter().filter_map(|request| match request {
        Request::Preset { name, json } => Some((name, json)),
        _ => None,
    }).collect();
    assert_eq!(written.len(), 1, "wanted one preset written after SAVE then {}, got {}", labels[option], written.len());
    let (name, text) = written.into_iter().next().expect("one preset");
    let preset = Preset::from_json(&json::parse(&text).unwrap_or_else(|| panic!("wanted the saved preset to be valid JSON, got {text}"))).unwrap_or_else(|| panic!("wanted the saved file to read as a preset, got {text}"));
    (name, preset)
}

#[test]
fn a_rack_preset_without_a_midi_row_keeps_row_zeros_midi_row_as_it_was() {
    let mut app = open(&racked(vec![midi(kept()), core(1.0), effect(Kind::Delay)]), CORE_ROW);
    let project = loaded(&mut app, &sound(vec![core(0.3), effect(Kind::Reverb)]));
    assert_eq!(unseeded(&project.tracks[0].rack), vec![midi(kept()), core(0.3), effect(Kind::Reverb)], "wanted the preset's rack under the track's own MIDI row, its arp as it was");
}

#[test]
fn a_rack_preset_with_a_midi_row_replaces_the_whole_rack() {
    let mut app = open(&racked(vec![midi(kept()), core(1.0), effect(Kind::Delay)]), CORE_ROW);
    let incoming = vec![midi(carried()), core(0.3), effect(Kind::Reverb)];
    let project = loaded(&mut app, &sound(incoming.clone()));
    assert_eq!(unseeded(&project.tracks[0].rack), incoming, "wanted a rack preset carrying a MIDI row to replace the rack whole, row 0 included");
}

#[test]
fn a_midi_row_saves_as_a_one_row_preset_and_lands_back_in_place() {
    let row = midi(carried());
    let mut source = racked(vec![row, core(1.0)]);
    source.tracks[0].automation.lfos.push(LaneLfo { amount: GATE_SWING, ..LaneLfo::blank(Aim::knob(MIDI_ROW, "ARP", "GATE")) });
    let mut app = open(&source, MIDI_ROW);
    let (name, preset) = saved(&mut app, &[MidiRow::LABEL, Preset::RACK], 0);
    let lfo = LaneLfo { amount: GATE_SWING, ..LaneLfo::blank(Aim::knob(0, "ARP", "GATE")) };
    assert_eq!((name.as_str(), &preset), ("MIDI.json", &Preset::Row(row, vec![lfo.clone()])), "wanted the shown MIDI row saved as a one-row preset carrying its gate LFO");
    let mut app = open(&racked(vec![midi(kept()), core(0.6)]), MIDI_ROW);
    let project = loaded(&mut app, &preset);
    assert_eq!(unseeded(&project.tracks[0].rack), vec![row, core(0.6)], "wanted the MIDI row preset to land in place of the shown MIDI row, the Core left as it was");
    assert_eq!(project.tracks[0].automation.lfos, vec![lfo], "wanted the preset's gate LFO to land on the MIDI row");
}

#[test]
fn saving_the_effects_leaves_the_midi_row_and_the_core_behind() {
    let mut app = open(&racked(vec![midi(kept()), core(1.0), effect(Kind::Eq), effect(Kind::Delay)]), MIDI_ROW);
    let (name, preset) = saved(&mut app, &[MidiRow::LABEL, Preset::RACK, "Effects"], 2);
    let Preset::Rack(Sound { rack, .. }) = &preset else {
        panic!("wanted the effects saved as a rack, got {preset:?}");
    };
    assert_eq!((name.as_str(), rack.clone()), ("Effects.json", vec![effect(Kind::Eq), effect(Kind::Delay)]), "wanted the effects saved without the MIDI row and the Core");
}
