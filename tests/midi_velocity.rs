use bp::app::{App, Event};
use bp::arp::MidiRow;
use bp::draw::Rect;
use bp::face::{CELL, COLUMNS, Faced, PITCH, READOUT_ROWS, ROWS, THROW, View};
use bp::json::{self, Json};
use bp::keyboard::Key;
use bp::project::{Cores, Host, Project, Row, Track, settled_rack};
use support::{LEVEL, centre, core, face_rect, painted, remembered};

const TRACK: usize = 0;
const HOST: Host = Host::Track(TRACK);
const MIDI_ROW: usize = 0;
const DEFAULT: u8 = 100;
const MOST: u8 = 127;
const LEAST: u8 = 1;
const SOFT: u8 = 64;
const VEL: (&str, &str) = ("MIDI", "VELOCITY");
const SHORT_DRAG: i32 = 30;
const READ: [(&str, u8); 8] = [("64", 64), ("1", 1), ("0", 1), ("-5", 1), ("127", 127), ("500", 127), ("63.6", 64), ("\"loud\"", 100)];
const TYPED: [(&str, u8); 5] = [("64", 64), ("127", 127), ("1", 1), ("0", 1), ("500", 127)];

fn velocity_of(row: &Row) -> u8 {
    match row {
        Row::Midi(midi) => midi.velocity,
        other => panic!("wanted a MIDI row, got {}", other.label()),
    }
}

fn song(velocity: u8) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[TRACK].rack = vec![Row::Midi(MidiRow { velocity, ..MidiRow::DEFAULT }), core(LEVEL)];
    project
}

fn open(project: &Project) -> App {
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false,\"selected\":{TRACK},\"open\":{MIDI_ROW}}}", project.to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    painted(&mut app);
    app.take_requests();
    app
}

fn vel_square() -> Rect {
    let row = MidiRow::DEFAULT;
    let squares = (READOUT_ROWS..ROWS).flat_map(|line| (0..COLUMNS).map(move |column| Rect::new(column * PITCH, line * PITCH, CELL, CELL)));
    let named = |square: &Rect| {
        let (x, y) = centre(*square);
        row.index_at(x, y, View::default()).and_then(|index| row.setting_of(index)).is_some_and(|(band, name, _)| (band, name) == VEL)
    };
    squares.into_iter().find(named).unwrap_or_else(|| panic!("wanted a {} {} square on the MIDI row's face", VEL.0, VEL.1))
}

fn on_face(app: &mut App) -> (i32, i32) {
    let face = face_rect(&painted(app));
    let square = vel_square();
    centre(Rect::new(face.x + square.x, face.y + square.y, square.w, square.h))
}

fn press(app: &mut App, (x, y): (i32, i32)) {
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
}

fn drag(app: &mut App, (x, y): (i32, i32), dy: i32) {
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Move { x, y: y + dy });
    app.event(Event::Up { x, y: y + dy });
}

fn key(app: &mut App, key: Key) {
    app.event(Event::Key { key, shift: false, command: false, repeat: false });
}

fn heard(app: &mut App) -> u8 {
    painted(app);
    velocity_of(&remembered(app).rack(HOST)[MIDI_ROW])
}

#[test]
fn every_new_midi_track_starts_at_velocity_100() {
    let fresh = Project::new();
    let midi: Vec<&Track> = fresh.tracks.iter().filter(|track| !track.audio && !track.group).collect();
    assert!(!midi.is_empty(), "wanted a fresh project to hold a MIDI track");
    for track in midi.into_iter().chain([&Track::blank()]) {
        let got = velocity_of(&track.rack[MIDI_ROW]);
        assert_eq!(got, DEFAULT, "wanted a new MIDI track's row at velocity {DEFAULT}, got {got}");
    }
    let settled = settled_rack(Cores::Required, vec![core(LEVEL)]);
    let got = velocity_of(&settled[MIDI_ROW]);
    assert_eq!(got, DEFAULT, "wanted the MIDI row a rack gains at velocity {DEFAULT}, got {got}");
    let inserted = MidiRow::DEFAULT.velocity;
    assert_eq!(inserted, DEFAULT, "wanted an inserted MIDI row at velocity {DEFAULT}, got {inserted}");
}

#[test]
fn the_midi_face_shows_a_vel_square_reading_the_rows_velocity() {
    let square = vel_square();
    let (x, y) = centre(square);
    for velocity in [DEFAULT, MOST, SOFT, LEAST] {
        let row = MidiRow { velocity, ..MidiRow::DEFAULT };
        let got = row.index_at(x, y, View::default()).and_then(|index| row.setting_of(index)).map(|(_, _, value)| value);
        assert_eq!(got, Some(f64::from(velocity)), "wanted the VEL square to read {velocity}, got {got:?}");
    }
}

#[test]
fn a_velocity_is_written_only_off_100_and_reads_back() {
    let plain = Row::Midi(MidiRow::DEFAULT).to_json();
    assert!(plain.get("velocity").is_none(), "wanted a row at {DEFAULT} to leave velocity out, got {}", plain.write());
    for velocity in [SOFT, MOST, LEAST] {
        let row = Row::Midi(MidiRow { velocity, ..MidiRow::DEFAULT });
        let json = row.to_json();
        let written = json.get("velocity").and_then(Json::as_f64);
        assert_eq!(written, Some(f64::from(velocity)), "wanted velocity {velocity} written under velocity, got {}", json.write());
        assert_eq!(Row::from_json(&json), Some(row), "the MIDI row's velocity {velocity} changed in the save");
    }
    let project = song(SOFT);
    let back = Project::from_json(&project.to_json()).expect("wanted the saved project to open");
    let got = velocity_of(&back.rack(HOST)[MIDI_ROW]);
    assert_eq!(got, SOFT, "wanted the project to reopen with its row at velocity {SOFT}, got {got}");
}

#[test]
fn a_read_velocity_stops_at_its_ends_and_a_missing_one_is_100() {
    for (text, wanted) in READ {
        let json = json::parse(&format!("{{\"kind\": \"midi\", \"velocity\": {text}}}")).unwrap_or_else(|| panic!("wanted valid JSON for velocity {text}"));
        let got = Row::from_json(&json).as_ref().map(velocity_of);
        assert_eq!(got, Some(wanted), "wanted velocity {text} read as {wanted}, got {got:?}");
    }
    let bare = json::parse("{\"kind\": \"midi\"}").expect("wanted valid JSON");
    let got = Row::from_json(&bare).as_ref().map(velocity_of);
    assert_eq!(got, Some(DEFAULT), "wanted a row without a velocity read at {DEFAULT}, got {got:?}");
}

#[test]
fn a_velocity_set_by_name_lands_and_stops_at_its_ends() {
    for (value, wanted) in [(64.0, SOFT), (127.0, MOST), (0.0, LEAST), (500.0, MOST)] {
        let mut row = MidiRow::DEFAULT;
        let found = row.assign(VEL.0, VEL.1, value);
        assert!(found.is_some(), "wanted {} {} found on the MIDI row's face", VEL.0, VEL.1);
        assert_eq!(row.velocity, wanted, "wanted {value} set by name to land at {wanted}, got {}", row.velocity);
    }
}

#[test]
fn a_typed_velocity_lands_on_return_and_stops_at_its_ends() {
    for (text, wanted) in TYPED {
        let mut app = open(&song(DEFAULT));
        let square = on_face(&mut app);
        press(&mut app, square);
        press(&mut app, square);
        for c in text.chars() {
            key(&mut app, Key::Char(c));
        }
        key(&mut app, Key::Return);
        let got = heard(&mut app);
        assert_eq!(got, wanted, "wanted {text} typed into VEL to land at {wanted}, got {got}");
    }
}

#[test]
fn a_drag_moves_the_velocity_between_1_and_127_and_a_shift_press_returns_100() {
    let mut app = open(&song(DEFAULT));
    let square = on_face(&mut app);
    drag(&mut app, square, SHORT_DRAG);
    let lowered = heard(&mut app);
    assert!(lowered > LEAST && lowered < DEFAULT, "wanted a {SHORT_DRAG} px drag down to lower the velocity part of the way from {DEFAULT}, got {lowered}");
    drag(&mut app, square, THROW as i32);
    let floor = heard(&mut app);
    assert_eq!(floor, LEAST, "wanted a full drag down to stop at velocity {LEAST}, got {floor}");
    drag(&mut app, square, -(THROW as i32));
    let top = heard(&mut app);
    assert_eq!(top, MOST, "wanted a full drag up to stop at velocity {MOST}, got {top}");
    app.event(Event::Modifiers { shift: true, command: false });
    press(&mut app, square);
    app.event(Event::Modifiers { shift: false, command: false });
    let back = heard(&mut app);
    assert_eq!(back, DEFAULT, "wanted a SHIFT-press to return the velocity to {DEFAULT}, got {back}");
}
