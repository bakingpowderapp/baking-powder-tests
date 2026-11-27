use bp::app::{App, Event, Opened, Request};
use bp::arp::Arp;
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::browser::{Found, Item};
use bp::draw::Rect;
use bp::face::{self, CELL, FRAME, PITCH, W};
use bp::factory::{EFFECTS, PRESETS};
use bp::fx::{Kind, Params, Slot};
use bp::json::{self, Json};
use bp::project::{Preset, Project, RACK_ROWS, Row, Track, VELOCITY};
use support::{AUDIBLE, BLOCK, BLOCKS, HEIGHT, HUSH_DB, LEVEL, SAMPLE_RATE, WIDTH, browser_rect, centre, core, effect, face_rect, lane_rows, midi, painted, peak, unseeded};

const ROOT: &str = "/x/Presets";
const BORDER: i32 = 8;
const AUDIO_TRACK: usize = 2;
const SETTLE: usize = 40;
const HUSHED: f32 = 0.1;
const CLOSE: f32 = 0.005;

fn open(project: &Project, selected: usize) -> App {
    let session = format!("{{\"project\":{},\"selected\":{selected},\"aside\":\"browser\",\"keyboard\":false,\"folders\":[\"{ROOT}\"],\"unfolded\":[\"{ROOT}\"]}}", project.to_json().write());
    let mut app = App::new();
    app.resume(&session);
    app.take_requests();
    app
}

fn file(name: &str, preset: &Preset) -> Found {
    Found { path: format!("{ROOT}/{name}.json"), name: format!("{name}.json"), item: Item::read(&preset.to_json().write()) }
}

fn rack(rows: Vec<Row>) -> Preset {
    Preset::Rack(bp::project::Sound { rack: rows, lfos: Vec::new() })
}

fn row(row: Row) -> Preset {
    Preset::Row(row, Vec::new())
}

fn utility(gain_db: f64) -> Row {
    let mut slot = Slot::new(Kind::Utility);
    if let Params::Utility(utility) = &mut slot.params {
        utility.gain_db = gain_db;
    }
    Row::Effect(slot)
}

fn utilities(count: usize) -> Vec<Row> {
    (0..count).map(|_| utility(0.0)).collect()
}

fn stock_offered(kind: Kind) -> bool {
    PRESETS.iter().filter(|(path, _)| path.starts_with(EFFECTS)).any(|(_, text)| json::parse(text).and_then(|json| Preset::from_json(&json)).is_some_and(|preset| matches!(preset, Preset::Row(Row::Effect(slot), _) if slot.kind() == kind)))
}

fn save_square(face: Rect, kind: Kind) -> Rect {
    let slot = 1 + i32::from(stock_offered(kind));
    Rect::new(face.x + W + BORDER - 2 * FRAME, face.y + slot * PITCH, CELL, CELL)
}

fn with_audio_track(rack: Vec<Row>) -> Project {
    let mut project = Project::new();
    project.add_track(AUDIO_TRACK, Track::audio());
    project.tracks[AUDIO_TRACK].rack = rack;
    project
}

fn browser_row(app: &mut App, index: i32) -> Rect {
    let browser = browser_rect(&painted(app));
    let mark = Rect::new(browser.x + PITCH, browser.y + (index + 1) * PITCH, CELL, CELL);
    Rect::new(mark.x + PITCH, mark.y, browser.x + browser.w - mark.x - PITCH, CELL)
}

fn track_rows(app: &mut App, count: usize) -> Vec<Rect> {
    let rows = lane_rows(&painted(app));
    assert_eq!(rows.len(), count, "wanted {count} track lanes painted, got {}", rows.len());
    rows
}

fn onto_tile(rows: &[Rect], track: usize) -> (i32, i32) {
    (rows[track].x / 2, rows[track].y + rows[track].h / 2)
}

fn onto_lane(rows: &[Rect], track: usize) -> (i32, i32) {
    centre(rows[track])
}

fn click(app: &mut App, rect: Rect) {
    let (x, y) = centre(rect);
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
}

fn drag(app: &mut App, from: Rect, to: (i32, i32)) {
    let (x, y) = centre(from);
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Move { x: to.0, y: to.1 });
    app.event(Event::Up { x: to.0, y: to.1 });
}

fn remembered(app: &mut App) -> Option<Json> {
    let text = app.take_requests().into_iter().rev().find_map(|request| if let Request::Remember { json } = request { Some(json) } else { None })?;
    Some(json::parse(&text).expect("wanted the remembered session to parse"))
}

fn project_of(session: &Json) -> Project {
    Project::from_json(session.get("project").expect("wanted the remembered session to hold the project")).expect("wanted the remembered project to read")
}

fn landed(app: &mut App) -> Project {
    project_of(&remembered(app).expect("wanted the session remembered after the drop"))
}

fn after(app: &mut App) -> Option<Project> {
    remembered(app).map(|session| project_of(&session))
}

fn all_racks(project: &Project) -> Vec<Vec<Row>> {
    project.tracks.iter().map(|track| track.rack.clone()).collect()
}

fn heard(project: &Project) -> f32 {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, None, &[], &[]);
    audio.note_on(0, 60, VELOCITY);
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(BLOCKS);
    for _ in 0..BLOCKS {
        renderer.render(&mut l, &mut r);
        out.push(l.clone());
    }
    peak(&out[SETTLE..])
}

#[test]
fn a_core_row_dragged_onto_a_midi_track_joins_that_track_and_selects_it() {
    let mut app = open(&Project::new(), 0);
    app.listed(ROOT, vec![file("Bright", &row(core(0.5)))]);
    let rows = track_rows(&mut app, 2);
    let from = browser_row(&mut app, 0);
    drag(&mut app, from, onto_tile(&rows, 1));
    let session = remembered(&mut app).expect("wanted the session remembered after the drop");
    let project = project_of(&session);
    assert_eq!(unseeded(&project.tracks[1].rack), vec![midi(Arp::DEFAULT), core(1.0), core(0.5)], "wanted the Core row to join MIDI 2's rack after its own Core");
    assert_eq!(unseeded(&project.tracks[0].rack), vec![midi(Arp::DEFAULT), core(1.0)], "wanted MIDI 1, the selected track, left alone");
    assert_eq!(session.get("selected").and_then(Json::as_i64), Some(1), "wanted the track the row landed on selected");
}

#[test]
fn a_rack_with_a_core_dragged_onto_a_midi_track_replaces_its_rack() {
    let mut project = Project::new();
    project.tracks[1].rack = vec![midi(Arp::DEFAULT), core(1.0), effect(Kind::Eq)];
    let mut app = open(&project, 0);
    app.listed(ROOT, vec![file("Chrome", &rack(vec![core(0.5), effect(Kind::Motion)]))]);
    let rows = track_rows(&mut app, 2);
    let from = browser_row(&mut app, 0);
    drag(&mut app, from, onto_lane(&rows, 1));
    let project = landed(&mut app);
    assert_eq!(unseeded(&project.tracks[1].rack), vec![midi(Arp::DEFAULT), core(0.5), effect(Kind::Motion)], "wanted the Core rack to become MIDI 2's rack under its MIDI row, its EQ gone");
    assert_eq!(unseeded(&project.tracks[0].rack), vec![midi(Arp::DEFAULT), core(1.0)], "wanted MIDI 1 left alone");
}

#[test]
fn an_effects_rack_dragged_onto_an_audio_track_joins_its_chain_and_dropped_off_the_tracks_does_nothing() {
    let before = with_audio_track(vec![effect(Kind::Eq)]);
    let mut app = open(&before, 0);
    app.listed(ROOT, vec![file("Space", &rack(vec![effect(Kind::Delay), effect(Kind::Reverb)]))]);
    let rows = track_rows(&mut app, 3);
    let from = browser_row(&mut app, 0);
    drag(&mut app, from, (WIDTH / 2, rows[0].y / 2));
    let untouched = after(&mut app);
    assert!(untouched.as_ref().is_none_or(|project| all_racks(project) == all_racks(&before)), "wanted a rack let go above the tracks to change nothing, got {:?}", untouched.map(|project| all_racks(&project)));
    let from = browser_row(&mut app, 0);
    drag(&mut app, from, onto_tile(&rows, AUDIO_TRACK));
    let project = landed(&mut app);
    assert_eq!(project.tracks[AUDIO_TRACK].rack, vec![effect(Kind::Eq), effect(Kind::Delay), effect(Kind::Reverb)], "wanted the delay and reverb added after the audio track's EQ");
}

#[test]
fn an_audio_tracks_rack_saves_as_an_effects_rack_that_joins_a_midi_rack_after_its_core() {
    let strip = vec![effect(Kind::Eq), effect(Kind::Compressor)];
    let mut app = open(&with_audio_track(strip.clone()), AUDIO_TRACK);
    let face = face_rect(&painted(&mut app));
    let save = save_square(face, Kind::Eq);
    click(&mut app, save);
    let labels = [strip[0].label(), Preset::RACK];
    click(&mut app, face::option_rect(face::panel_rect(save, &labels, 0, Rect::new(0, 0, WIDTH, HEIGHT)), 1));
    let saved: Vec<(String, String)> = app.take_requests().into_iter().filter_map(|request| match request {
        Request::Preset { name, json } => Some((name, json)),
        _ => None,
    }).collect();
    assert_eq!(saved.len(), 1, "wanted one preset written after SAVE then RACK, got {}", saved.len());
    let (name, text) = &saved[0];
    let preset = Preset::from_json(&json::parse(text).expect("wanted the saved preset to be valid JSON")).expect("wanted the saved file to read as a preset");
    assert_eq!((name.as_str(), &preset), ("Rack.json", &rack(strip.clone())), "wanted the audio track's rack saved as an effects rack with no Core");
    app.listed(ROOT, vec![file("Strip", &preset)]);
    let rows = track_rows(&mut app, 3);
    let from = browser_row(&mut app, 0);
    drag(&mut app, from, onto_tile(&rows, 0));
    let project = landed(&mut app);
    assert_eq!(unseeded(&project.tracks[0].rack), vec![midi(Arp::DEFAULT), core(1.0), effect(Kind::Eq), effect(Kind::Compressor)], "wanted the effects rack to join MIDI 1's rack after its Core");
    let face = face_rect(&painted(&mut app));
    let from = browser_row(&mut app, 0);
    drag(&mut app, from, centre(face));
    let project = landed(&mut app);
    assert_eq!(unseeded(&project.tracks[0].rack), vec![midi(Arp::DEFAULT), core(1.0), effect(Kind::Eq), effect(Kind::Compressor), effect(Kind::Eq), effect(Kind::Compressor)], "wanted the effects rack dropped on the Core to join the shown rack, its MIDI row and Core kept");
}

#[test]
fn a_core_racks_cores_are_stripped_on_an_audio_track_and_a_core_row_is_refused() {
    let mut app = open(&with_audio_track(vec![effect(Kind::Eq)]), 0);
    app.listed(ROOT, vec![file("1 Chrome", &rack(vec![midi(Arp::DEFAULT), core(1.0), effect(Kind::Motion), core(0.5), effect(Kind::Delay)])), file("2 Bright", &row(core(0.5)))]);
    let rows = track_rows(&mut app, 3);
    let from = browser_row(&mut app, 0);
    drag(&mut app, from, onto_lane(&rows, AUDIO_TRACK));
    let stripped = vec![effect(Kind::Eq), effect(Kind::Motion), effect(Kind::Delay)];
    let project = landed(&mut app);
    assert_eq!(project.tracks[AUDIO_TRACK].rack, stripped, "wanted the MIDI row and both Cores left out and the Motion row and delay added after the EQ");
    let from = browser_row(&mut app, 1);
    drag(&mut app, from, onto_lane(&rows, AUDIO_TRACK));
    let refused = after(&mut app);
    assert!(refused.as_ref().is_none_or(|project| project.tracks[AUDIO_TRACK].rack == stripped), "wanted a Core row refused by the audio track, got {:?}", refused.map(|project| project.tracks[AUDIO_TRACK].rack.clone()));
}

#[test]
fn an_effects_rack_joins_the_end_of_the_rack_and_one_that_does_not_fit_is_refused() {
    let mut app = open(&with_audio_track(utilities(8)), 0);
    app.listed(ROOT, vec![file("1 Space", &rack(vec![effect(Kind::Motion), effect(Kind::Delay), effect(Kind::Reverb), effect(Kind::Eq)])), file("2 Wall", &rack(utilities(RACK_ROWS - 1)))]);
    let rows = track_rows(&mut app, 3);
    let from = browser_row(&mut app, 0);
    drag(&mut app, from, onto_lane(&rows, AUDIO_TRACK));
    let project = landed(&mut app);
    let joined: Vec<Row> = utilities(8).into_iter().chain([effect(Kind::Motion), effect(Kind::Delay), effect(Kind::Reverb), effect(Kind::Eq)]).collect();
    assert_eq!(project.tracks[AUDIO_TRACK].rack, joined, "wanted all four effects after the utilities");
    let from = browser_row(&mut app, 1);
    drag(&mut app, from, onto_lane(&rows, AUDIO_TRACK));
    let refused = after(&mut app);
    assert!(refused.as_ref().is_none_or(|project| project.tracks[AUDIO_TRACK].rack == joined), "wanted the {} utilities, which do not fit beside twelve rows, refused whole, got {:?}", RACK_ROWS - 1, refused.map(|project| project.tracks[AUDIO_TRACK].rack.clone()));
}

#[test]
fn a_preset_file_dropped_from_the_desktop_onto_a_track_lands_on_it() {
    let mut app = open(&Project::new(), 0);
    let rows = track_rows(&mut app, 2);
    let at = onto_tile(&rows, 1);
    assert_eq!(app.opened(String::from("/x/Slap.json"), Some(&row(effect(Kind::Delay)).to_json().write()), Some(at)), Opened::Preset, "wanted the dropped row preset taken");
    let project = landed(&mut app);
    assert_eq!(unseeded(&project.tracks[1].rack), vec![midi(Arp::DEFAULT), core(1.0), effect(Kind::Delay)], "wanted the delay to join MIDI 2, the track under the drop");
    assert_eq!(unseeded(&project.tracks[0].rack), vec![midi(Arp::DEFAULT), core(1.0)], "wanted MIDI 1, the selected track, left alone");
}

#[test]
fn an_effect_added_to_the_last_free_rows_is_heard_in_the_tracks_chain() {
    let mut nearly_full = Project::new();
    nearly_full.tracks.truncate(1);
    nearly_full.tracks[0].rack = [midi(Arp::DEFAULT), core(LEVEL)].into_iter().chain(utilities(RACK_ROWS - 4)).collect();
    let dry = heard(&nearly_full);
    assert!(dry > AUDIBLE, "wanted the held note heard before the drop, got a peak of {dry}");
    let mut app = open(&nearly_full, 0);
    app.listed(ROOT, vec![file("Hush", &rack(vec![utility(0.0), utility(HUSH_DB)]))]);
    let rows = track_rows(&mut app, 1);
    let from = browser_row(&mut app, 0);
    drag(&mut app, from, onto_tile(&rows, 0));
    let project = landed(&mut app);
    assert_eq!(project.tracks[0].rack.len(), RACK_ROWS, "wanted both utilities to fill the rack to its last row");
    let hushed = heard(&project);
    assert!((hushed / dry - HUSHED).abs() < CLOSE, "wanted the note {HUSH_DB} dB quieter through the utility in the last row, a peak ratio of {HUSHED}, got {}", hushed / dry);
}
