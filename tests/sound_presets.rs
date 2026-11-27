use bp::app::{App, Event, Request};
use bp::arp::Arp;
use bp::browser::{Found, Item};
use bp::draw::Rect;
use bp::face::{CELL, PITCH};
use bp::fx::Kind;
use bp::json;
use bp::project::{Aim, LaneLfo, Preset, Project, RACK_ROWS, Track};
use support::{CORE_ROW, browser_rect, centre, core, effect, midi, painted};

const ROOT: &str = "/x/Sounds";

fn open(project: &Project) -> App {
    let session = format!("{{\"project\":{},\"selected\":0,\"aside\":\"browser\",\"keyboard\":false,\"folders\":[\"{ROOT}\"],\"unfolded\":[\"{ROOT}\"]}}", project.to_json().write());
    let mut app = App::new();
    app.resume(&session);
    app.take_requests();
    app
}

fn file(name: &str, preset: &Preset) -> Found {
    Found { path: format!("{ROOT}/{name}.json"), name: format!("{name}.json"), item: Item::read(&preset.to_json().write()) }
}

fn browser_row(app: &mut App, index: i32) -> Rect {
    let browser = browser_rect(&painted(app));
    let mark = Rect::new(browser.x + PITCH, browser.y + (index + 1) * PITCH, CELL, CELL);
    Rect::new(mark.x + PITCH, mark.y, browser.x + browser.w - mark.x - PITCH, CELL)
}

fn click(app: &mut App, rect: Rect) {
    let (x, y) = centre(rect);
    app.event(Event::Modifiers { shift: false, command: false });
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    app.event(Event::Modifiers { shift: false, command: false });
}

fn landed(app: &mut App) -> Project {
    let text = app.take_requests().into_iter().rev().find_map(|request| if let Request::Remember { json } = request { Some(json) } else { None }).expect("wanted the session remembered after the load");
    let session = json::parse(&text).expect("wanted the remembered session to parse");
    Project::from_json(session.get("project").expect("wanted the remembered session to hold the project")).expect("wanted the remembered project to read")
}

#[test]
fn a_tracks_sound_is_its_rack_and_row_lanes_and_a_blank_track_takes_it_whole() {
    let mut track = Track::blank();
    track.rack = vec![midi(Arp::DEFAULT), core(1.0), effect(Kind::Eq)];
    track.automation.lfos.push(LaneLfo::blank(Aim::knob(CORE_ROW, "OSC 1", "WAVE 1")));
    track.automation.lfos.push(LaneLfo::blank(Aim::Gain));
    let sound = track.sound();
    assert_eq!(sound.rack, track.rack, "wanted the track's rack in the sound");
    assert_eq!(sound.lfos.iter().map(|lfo| lfo.aim.words()).collect::<Vec<_>>(), vec![("OSC 1", "WAVE 1")], "wanted the row lane kept and the fader lane left out");
    let mut taken = Track::blank();
    taken.take_sound(sound.clone());
    assert_eq!(taken.sound(), sound, "a blank track did not take the sound whole");
}

#[test]
fn a_row_preset_pressed_on_a_full_rack_replaces_the_row_it_shows() {
    let mut project = Project::new();
    project.tracks[0].rack = [midi(Arp::DEFAULT), core(1.0)].into_iter().chain((2..RACK_ROWS).map(|_| effect(Kind::Utility))).collect();
    let mut app = open(&project);
    app.listed(ROOT, vec![file("Bright", &Preset::Row(core(0.5), Vec::new()))]);
    let row = browser_row(&mut app, 0);
    click(&mut app, row);
    let loaded = landed(&mut app);
    assert_eq!(loaded.tracks[0].rack.len(), RACK_ROWS, "wanted the full rack to stay full");
    assert_eq!(loaded.tracks[0].rack[CORE_ROW].unseeded(), core(0.5), "wanted the shown Core replaced by the pressed one though the rack has no free row");
    assert_eq!(&loaded.tracks[0].rack[..CORE_ROW], &project.tracks[0].rack[..CORE_ROW], "wanted the MIDI row left as it was");
    assert_eq!(&loaded.tracks[0].rack[CORE_ROW + 1..], &project.tracks[0].rack[CORE_ROW + 1..], "wanted the utilities left as they were");
}
