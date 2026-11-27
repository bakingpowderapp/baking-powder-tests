use bp::app::{App, Event, Request};
use bp::arp::Arp;
use bp::browser::{Found, Item};
use bp::draw::Rect;
use bp::face::{CELL, PITCH};
use bp::fx::Kind;
use bp::json;
use bp::project::{Aim, LaneLfo, Node, Preset, Project, TICKS_PER_BEAT};
use support::{CORE_ROW, browser_rect, centre, core, effect, midi, painted, unseeded};

const ROOT: &str = "/x/Presets";
const DELAY: usize = 3;
const BAND: &str = "LOOP";
const PARAM: &str = "FEEDBACK";
const NODE: Node = Node { tick: TICKS_PER_BEAT, travel: 0.5, bend: 0.0 };
const AMOUNT: f64 = 0.4;
const LEVELS: [f64; 2] = [0.5, 0.25];
const TRACK_ARP: Arp = Arp { on: true, ..Arp::DEFAULT };

fn racked() -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(TRACK_ARP), core(1.0), effect(Kind::Eq), effect(Kind::Delay)];
    project.tracks[0].automation.set_node(&Aim::knob(DELAY, BAND, PARAM), None, NODE);
    project
}

fn preset() -> Preset {
    let mut lfo = LaneLfo::blank(Aim::knob(1, "OSC 1", "LEVEL"));
    lfo.amount = AMOUNT;
    Preset::Rack(bp::project::Sound { rack: vec![core(LEVELS[0]), core(LEVELS[1]), effect(Kind::Motion)], lfos: vec![lfo] })
}

fn open() -> App {
    let session = format!("{{\"project\":{},\"selected\":0,\"aside\":\"browser\",\"keyboard\":false,\"palette\":false,\"folders\":[\"{ROOT}\"],\"unfolded\":[\"{ROOT}\"]}}", racked().to_json().write());
    let mut app = App::new();
    app.resume(&session);
    app.take_requests();
    app.listed(ROOT, vec![Found { path: format!("{ROOT}/Layers.json"), name: String::from("Layers.json"), item: Item::read(&preset().to_json().write()) }]);
    app
}

fn browser_row(app: &mut App) -> Rect {
    let browser = browser_rect(&painted(app));
    let mark = Rect::new(browser.x + PITCH, browser.y + PITCH, CELL, CELL);
    Rect::new(mark.x + PITCH, mark.y, browser.x + browser.w - mark.x - PITCH, CELL)
}

fn press(app: &mut App, shift: bool) -> Project {
    let (x, y) = centre(browser_row(app));
    app.event(Event::Modifiers { shift, command: false });
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    app.event(Event::Modifiers { shift: false, command: false });
    let text = app.take_requests().into_iter().rev().find_map(|request| if let Request::Remember { json } = request { Some(json) } else { None }).expect("wanted the session remembered after the press");
    let session = json::parse(&text).expect("wanted the remembered session to parse");
    Project::from_json(session.get("project").expect("wanted the remembered session to hold the project")).expect("wanted the remembered project to read")
}

fn curve_rows(project: &Project) -> Vec<usize> {
    project.tracks[0].automation.curves.iter().filter(|curve| matches!(&curve.aim, Aim::Knob { band, .. } if band == BAND)).map(|curve| curve.aim.row_or_top()).collect()
}

fn lfo_rows(project: &Project) -> Vec<(usize, f64)> {
    project.tracks[0].automation.lfos.iter().filter(|lfo| matches!(&lfo.aim, Aim::Knob { band, .. } if band == "OSC 1")).map(|lfo| (lfo.aim.row_or_top(), lfo.amount)).collect()
}

#[test]
fn a_preset_pressed_with_shift_swaps_in_only_its_cores_and_the_effects_stay() {
    let mut app = open();
    let project = press(&mut app, true);
    assert_eq!(unseeded(&project.tracks[0].rack), vec![midi(TRACK_ARP), core(LEVELS[0]), core(LEVELS[1]), effect(Kind::Eq), effect(Kind::Delay)], "wanted the preset's two Cores in place of the rack's one, the MIDI row, the EQ and delay staying");
    assert_eq!(curve_rows(&project), vec![DELAY + 1], "wanted the delay's curve to follow it down a row");
    assert_eq!(lfo_rows(&project), vec![(CORE_ROW + 1, AMOUNT)], "wanted the preset's LFO on its second Core");
}

#[test]
fn a_preset_pressed_without_shift_still_replaces_the_whole_rack() {
    let mut app = open();
    let project = press(&mut app, false);
    assert_eq!(unseeded(&project.tracks[0].rack), vec![midi(TRACK_ARP), core(LEVELS[0]), core(LEVELS[1]), effect(Kind::Motion)], "wanted the preset's whole rack under the track's own MIDI row, the EQ and delay gone");
    assert!(curve_rows(&project).is_empty(), "wanted the delay's curve gone with the delay, got curves on rows {:?}", curve_rows(&project));
}
