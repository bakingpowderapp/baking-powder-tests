use bp::app::{App, Event, Request};
use bp::arp::Arp;
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::browser::{Found, Item};
use bp::core::Params as CoreParams;
use bp::core::params::Envelope;
use bp::draw::Rect;
use bp::face::{CELL, PITCH};
use bp::factory::PRESETS;
use bp::fx::params::motion::MotionBand;
use bp::fx::params::{Chorus, MotionMode, Utility};
use bp::fx::{Kind, Params, Slot};
use bp::json::{self, Json};
use bp::project::{Preset, Project, Row, VELOCITY};
use support::{AUDIBLE, BLOCK, SAMPLE_RATE, browser_rect, centre, core, core_layer, midi, one_band, painted, peak_of, stage, worst_delta};

const ROOT: &str = "/x/Presets";
const KEY: u8 = 60;
const RENDER_SECONDS: f64 = 0.25;
const WIDENERS: [&str; 2] = ["Effects/Motion/Stereo Widener 1.json", "Effects/Motion/Stereo Widener 2.json"];
const LISTED: &str = "Stereo Widener 1.json";
const WET: f64 = 0.5904;
const DOUBLE_DB: f64 = 6.020_599_913_28;
const ALIKE: f32 = 1e-4;
const OPEN_ROW: usize = 2;

fn click() -> CoreParams {
    let mut patch = CoreParams::DEFAULT;
    patch.osc1.level = 0.0;
    patch.noise.on = true;
    patch.noise.level = 1.0;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 0.0, release_ms: 1.0, ..Envelope::DEFAULT };
    patch
}

fn chorus(wet: f64, trim_db: f64) -> Row {
    let chorus = Chorus { crossfade: true, trim_db, ..Chorus::DEFAULT };
    Row::Effect(Slot { wet, ..one_band(MotionBand { mode: MotionMode::Chorus, chorus, ..MotionBand::DEFAULT }) })
}

fn utility(gain_db: f64) -> Row {
    Row::Effect(Slot { params: Params::Utility(Utility { gain_db, ..Utility::DEFAULT }), ..Slot::new(Kind::Utility) })
}

fn rendered(rack: Vec<Row>) -> Vec<f32> {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = [midi(Arp::DEFAULT), Row::Core(core_layer(1.0, click()))].into_iter().chain(rack).collect();
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(&project, None, &[], &[]);
    audio.note_on(0, KEY, VELOCITY);
    let mut out = Vec::new();
    let total = (RENDER_SECONDS * SAMPLE_RATE) as usize;
    let (mut left, mut right) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    while out.len() < total {
        let n = BLOCK.min(total - out.len());
        renderer.render(&mut left[..n], &mut right[..n]);
        out.extend_from_slice(&left[..n]);
    }
    out
}

fn widener(name: &str) -> (String, Preset) {
    let text = PRESETS.iter().find(|(known, _)| *known == name).unwrap_or_else(|| panic!("wanted {name} in the bank")).1;
    let preset = Preset::from_json(&json::parse(text).unwrap_or_else(|| panic!("wanted {name} to parse"))).unwrap_or_else(|| panic!("wanted {name} to open"));
    (String::from(text), preset)
}

fn widener_slot(name: &str) -> Slot {
    match widener(name).1 {
        Preset::Row(Row::Effect(slot), lfos) if slot.kind() == Kind::Motion => {
            assert!(lfos.is_empty(), "wanted {name} without LFO lanes");
            slot
        }
        _ => panic!("wanted {name} to be a single Motion row"),
    }
}

fn chorus_of(slot: &Slot) -> &Chorus {
    match &slot.params {
        Params::Motion(motion) if motion.count() == 1 && motion.bands[0].mode == MotionMode::Chorus => &motion.bands[0].chorus,
        _ => panic!("wanted a Motion row of one chorus band"),
    }
}

fn untrimmed(slot: Slot) -> Row {
    let mut slot = slot;
    if let Params::Motion(motion) = &mut slot.params {
        motion.bands[0].chorus.trim_db = 0.0;
    }
    Row::Effect(slot)
}

fn open_with_chorus_shown(listed: &str) -> App {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), core(1.0), Row::Effect(stage(MotionMode::Chorus))];
    let session = format!("{{\"project\":{},\"selected\":0,\"open\":{OPEN_ROW},\"aside\":\"browser\",\"keyboard\":false,\"palette\":false,\"folders\":[\"{ROOT}\"],\"unfolded\":[\"{ROOT}\"]}}", project.to_json().write());
    let mut app = App::new();
    app.resume(&session);
    app.take_requests();
    app.listed(ROOT, vec![Found { path: format!("{ROOT}/{LISTED}"), name: String::from(LISTED), item: Item::read(listed) }]);
    app
}

fn browser_row(app: &mut App, index: i32) -> Rect {
    let browser = browser_rect(&painted(app));
    let mark = Rect::new(browser.x + PITCH, browser.y + (index + 1) * PITCH, CELL, CELL);
    Rect::new(mark.x + PITCH, mark.y, browser.x + browser.w - mark.x - PITCH, CELL)
}

fn pressed(app: &mut App, rect: Rect) -> Vec<Row> {
    let (x, y) = centre(rect);
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    let text = app.take_requests().into_iter().rev().find_map(|request| if let Request::Remember { json } = request { Some(json) } else { None }).expect("wanted the session remembered after the press");
    let session = json::parse(&text).expect("wanted the remembered session to parse");
    Project::from_json(session.get("project").expect("wanted the remembered session to hold the project")).expect("wanted the remembered project to read").tracks[0].rack.clone()
}

#[test]
fn the_trim_scales_the_whole_output_after_the_mix() {
    for wet in [0.0, WET] {
        let plain = rendered(vec![chorus(wet, 0.0)]);
        let doubled = rendered(vec![chorus(wet, DOUBLE_DB)]);
        assert!(peak_of(&plain) > AUDIBLE, "wanted the click heard through the chorus at wet {wet}");
        let twice: Vec<f32> = plain.iter().map(|sample| 2.0 * sample).collect();
        let delta = worst_delta(&twice, &doubled);
        assert!(delta < ALIKE, "at wet {wet} a +6 dB trim strayed {delta} from twice the untrimmed output, so the trim missed part of the mix");
    }
}

#[test]
fn a_widener_is_one_chorus_row_written_as_the_app_writes_it() {
    for name in WIDENERS {
        let (text, preset) = widener(name);
        chorus_of(&widener_slot(name));
        let mut bare = json::parse(&text).expect("wanted the preset file to parse");
        if let Json::Object(fields) = &mut bare {
            fields.retain(|(key, _)| key != "description");
        }
        assert_eq!(bare, preset.to_json(), "wanted {name} written as the app writes a Motion row, its description aside");
    }
}

#[test]
fn a_widener_sounds_as_its_chorus_untrimmed_into_a_utility_at_its_trim() {
    for name in WIDENERS {
        let slot = widener_slot(name);
        let trim_db = chorus_of(&slot).trim_db;
        let now = rendered(vec![Row::Effect(slot)]);
        let chained = rendered(vec![untrimmed(slot), utility(trim_db)]);
        assert!(peak_of(&now) > AUDIBLE, "wanted {name} heard");
        let delta = worst_delta(&now, &chained);
        assert!(delta < ALIKE, "{name} strayed {delta} from its chorus untrimmed into a Utility at {trim_db} dB");
    }
}

#[test]
fn a_widener_pressed_with_a_chorus_open_takes_the_open_rows_place() {
    let (text, _) = widener(WIDENERS[0]);
    let wanted = widener_slot(WIDENERS[0]);
    let mut app = open_with_chorus_shown(&text);
    let row = browser_row(&mut app, 0);
    let rack = pressed(&mut app, row);
    assert_eq!(rack.len(), 3, "wanted {LISTED} to take the open chorus's place rather than join after it, got {} rows", rack.len());
    let Row::Effect(slot) = &rack[OPEN_ROW] else {
        panic!("wanted the Motion row still at row {OPEN_ROW}, got {:?}", rack[OPEN_ROW]);
    };
    assert_eq!(slot.kind(), Kind::Motion, "wanted row {OPEN_ROW} still a Motion row, got {:?}", slot.kind());
    let (chorus, listed) = (chorus_of(slot), chorus_of(&wanted));
    assert_eq!((chorus.center_ms, chorus.trim_db, slot.wet), (listed.center_ms, listed.trim_db, wanted.wet), "wanted the open chorus to take the preset's settings");
}
