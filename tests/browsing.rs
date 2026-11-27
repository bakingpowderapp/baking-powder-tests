use bp::app::{App, Event, Request};
use bp::browser::{Found, Item};
use bp::draw::Rect;
use bp::face::{CELL, PITCH};
use bp::core::Params;
use bp::fx::{Kind, Slot};
use bp::json;
use bp::keyboard::Key;
use bp::project::{Layer, Preset, Project, Row};
use support::{CORE_ROW, browser_rect, core_layer, painted};

const ROOT: &str = "/x/Presets";
const UNDER_MIDI_AND_CORE: usize = CORE_ROW + 1;

fn core_at(level: f64) -> Row {
    let mut patch = Params::DEFAULT;
    patch.osc1.level = level;
    Row::Core(core_layer(1.0, patch))
}

fn file(name: &str, row: Row) -> Found {
    Found { path: format!("{ROOT}/{name}.json"), name: format!("{name}.json"), item: Item::read(&Preset::Row(row, Vec::new()).to_json().write()) }
}

fn open(files: Vec<Found>) -> App {
    let session = format!("{{\"project\":{},\"aside\":\"browser\",\"keyboard\":false,\"folders\":[\"{ROOT}\"],\"unfolded\":[\"{ROOT}\"]}}", Project::new().to_json().write());
    let mut app = App::new();
    app.resume(&session);
    app.take_requests();
    app.listed(ROOT, files);
    app.take_requests();
    app
}

fn browser_row(app: &mut App, index: i32) -> Rect {
    let browser = browser_rect(&painted(app));
    let mark = Rect::new(browser.x + PITCH, browser.y + (index + 1) * PITCH, CELL, CELL);
    Rect::new(mark.x + PITCH, mark.y, browser.x + browser.w - mark.x - PITCH, CELL)
}

fn click(app: &mut App, rect: Rect) {
    let (x, y) = (rect.x + rect.w / 2, rect.y + rect.h / 2);
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
}

fn press(app: &mut App, key: Key) {
    app.event(Event::Key { key, shift: false, command: false, repeat: false });
    app.event(Event::KeyUp { key });
}

fn rack(app: &mut App) -> Vec<Row> {
    let text = app.take_requests().into_iter().rev().find_map(|request| if let Request::Remember { json } = request { Some(json) } else { None }).expect("wanted the session remembered after the load");
    let session = json::parse(&text).expect("wanted the remembered session to parse");
    Project::from_json(session.get("project").expect("wanted the project in the session")).expect("wanted the project to read").tracks[0].rack.clone()
}

fn levels(rack: &[Row]) -> Vec<f64> {
    rack.iter().filter_map(|row| match row {
        Row::Core(Layer { patch, .. }) => Some(patch.osc1.level),
        Row::Midi(_) | Row::Effect(_) | Row::Bus(_) => None,
    }).collect()
}

#[test]
fn browsing_instrument_presets_replaces_the_shown_instrument() {
    let mut app = open(vec![file("1 first", core_at(0.1)), file("2 second", core_at(0.9)), file("3 third", core_at(0.5))]);
    let first = browser_row(&mut app, 0);
    click(&mut app, first);
    let loaded = rack(&mut app);
    assert_eq!((loaded.len(), levels(&loaded)), (2, vec![0.1]), "wanted the click to put the first Core preset in the Core's place under the MIDI row, got {loaded:?}");
    assert!(loaded[0].is_midi(), "wanted the MIDI row kept at row 0, got {loaded:?}");
    press(&mut app, Key::Down);
    let stepped = rack(&mut app);
    assert_eq!((stepped.len(), levels(&stepped)), (2, vec![0.9]), "wanted Down to swap in the second Core preset, got {stepped:?}");
    press(&mut app, Key::Down);
    let third = rack(&mut app);
    assert_eq!((third.len(), levels(&third)), (2, vec![0.5]), "wanted Down onto the third preset to take the place in turn, got {third:?}");
    press(&mut app, Key::Up);
    let back = rack(&mut app);
    assert_eq!((back.len(), levels(&back)), (2, vec![0.9]), "wanted Up to bring the second Core preset back in place, got {back:?}");
}

#[test]
fn browsing_effect_presets_joins_once_and_then_replaces_its_kind() {
    let quiet = |gain_db: f64| {
        let mut slot = Slot::new(Kind::Utility);
        if let bp::fx::Params::Utility(utility) = &mut slot.params {
            utility.gain_db = gain_db;
        }
        Row::Effect(slot)
    };
    let mut app = open(vec![file("1 soft", quiet(-6.0)), file("2 loud", quiet(6.0)), file("3 delay", Row::Effect(Slot::new(Kind::Delay)))]);
    let first = browser_row(&mut app, 0);
    click(&mut app, first);
    let joined = rack(&mut app);
    assert_eq!(joined.len(), UNDER_MIDI_AND_CORE + 1, "wanted the utility to join after the MIDI row and the Core, got {joined:?}");
    press(&mut app, Key::Down);
    let swapped = rack(&mut app);
    let gain = match &swapped[UNDER_MIDI_AND_CORE] {
        Row::Effect(Slot { params: bp::fx::Params::Utility(utility), .. }) => utility.gain_db,
        other => panic!("wanted a utility in row {UNDER_MIDI_AND_CORE}, got {other:?}"),
    };
    assert_eq!((swapped.len(), gain), (UNDER_MIDI_AND_CORE + 1, 6.0), "wanted Down to swap the utility for the louder one in place, got {swapped:?}");
    press(&mut app, Key::Down);
    let delayed = rack(&mut app);
    assert!(delayed.len() == UNDER_MIDI_AND_CORE + 2 && matches!(delayed[UNDER_MIDI_AND_CORE + 1], Row::Effect(ref slot) if slot.kind() == Kind::Delay), "wanted a delay to join rather than replace a utility, got {delayed:?}");
}
