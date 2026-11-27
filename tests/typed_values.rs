use bp::app::{App, Event, Request};
use bp::arp::Arp;
use bp::division::{Division, LABELS};
use bp::draw::{Rect, Tone};
use bp::face::{CELL, FRAME, Faced, Format, OFF_ON, PITCH, READOUT_ROWS, VALUE_SCALE, View, option_of, read, spell};
use bp::font::GLYPH_H;
use bp::fx::{Kind, Params, Slot};
use bp::json;
use bp::keyboard::Key;
use bp::project::{Host, Project, Row};
use support::{LEVEL, centre, core, face_rect, midi, painted};

const TRACK: usize = 0;
const DELAY: usize = 2;
const HOST: Host = Host::Track(TRACK);
const LEFT_TIME: (&str, &str) = ("TIME", "LEFT TIME");
const DIVISION_ROW: i32 = 2;
const DIVISION_COLUMN: i32 = 0;
const DIVISION_SPAN: i32 = 3;
const DEFAULT_MS: f64 = 375.0;
const TYPED_MS: f64 = 250.0;
const HALF_SECOND_MS: f64 = 500.0;
const CLAMPED_MS: f64 = bp::fx::params::Delay::MAX_TIME;
const ELSEWHERE: (i32, i32) = (2, 2);
const MODES: [&str; 3] = ["CLEAN", "TAPE", "BBD"];
const EIGHTH: usize = 16;
const DOTTED_EIGHTH: usize = 14;
const SAMPLES: [f64; 9] = [0.0, 0.37, 1.0, 12.5, 375.0, 1234.5, 20_000.0, -6.5, 60.0];

const FORMATS: [Format; 34] = [
    Format::Percent,
    Format::SignedPercent,
    Format::Semitones,
    Format::Cents,
    Format::SignedCents,
    Format::CentsTenth,
    Format::Integer,
    Format::SignedInteger,
    Format::Hertz,
    Format::LfoHertz,
    Format::HertzOffAt20,
    Format::HertzOffAt20k,
    Format::Millis,
    Format::MillisWhole,
    Format::MillisTenth,
    Format::MillisHundredths,
    Format::MillisOffBelowHalf,
    Format::SignedMillis,
    Format::Seconds,
    Format::TwoPlaces,
    Format::Shelf,
    Format::GlideTime,
    Format::Decibels,
    Format::DecibelsUnsigned,
    Format::Floor,
    Format::Degrees,
    Format::Spread,
    Format::Times,
    Format::Ratio,
    Format::Range,
    Format::Octaves,
    Format::Balance,
    Format::Flat,
    Format::Release,
];

const READINGS: [(Format, &str, &str); 19] = [
    (Format::MillisWhole, "375", "375 ms"),
    (Format::MillisWhole, "0.5 s", "500 ms"),
    (Format::MillisWhole, "500ms", "500 ms"),
    (Format::Hertz, "1.2k", "1.20 kHz"),
    (Format::Hertz, "1.2 kHz", "1.20 kHz"),
    (Format::Hertz, "440", "440 Hz"),
    (Format::Decibels, "-6", "-6.0 dB"),
    (Format::Percent, "50", "50%"),
    (Format::Percent, "50%", "50%"),
    (Format::Semitones, "+3", "+3 st"),
    (Format::Seconds, "800ms", "800 ms"),
    (Format::Seconds, "2", "2.00 s"),
    (Format::Range, "off", "off"),
    (Format::Balance, "50l", "50L"),
    (Format::Balance, "c", "C"),
    (Format::Shelf, "hp 30", "HP 30%"),
    (Format::Ratio, "4:1", "4.0:1"),
    (Format::Times, "x1.5", "×1.50"),
    (Format::Release, "100ms", "100 ms"),
];

const UNREADABLE: [(Format, &str); 5] = [(Format::MillisWhole, "abc"), (Format::MillisWhole, ""), (Format::MillisWhole, "1/8"), (Format::Percent, "inf"), (Format::Release, "0")];

fn racked(synced: bool) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    let mut slot = Slot::new(Kind::Delay);
    if let Params::Delay(delay) = &mut slot.params {
        delay.sync_l = synced;
    }
    project.tracks[TRACK].rack = vec![midi(Arp::DEFAULT), core(LEVEL), Row::Effect(slot)];
    project
}

fn open(project: &Project) -> App {
    let session = format!("{{\"project\":{},\"open\":{DELAY},\"keyboard\":false}}", project.to_json().write());
    let mut app = App::new();
    app.resume(&session);
    painted(&mut app);
    app.take_requests();
    app
}

fn press(app: &mut App, (x, y): (i32, i32)) {
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
}

fn key(app: &mut App, key: Key) -> bool {
    app.event(Event::Key { key, shift: false, command: false, repeat: false })
}

fn typed(app: &mut App, text: &str) {
    for c in text.chars() {
        key(app, Key::Char(c));
    }
}

fn on_face(app: &mut App, cell: Rect) -> (i32, i32) {
    let face = face_rect(&painted(app));
    centre(Rect::new(face.x + cell.x, face.y + cell.y, cell.w, cell.h))
}

fn left_time_cell(app: &mut App, project: &Project) -> (i32, i32) {
    let Row::Effect(slot) = &project.rack(HOST)[DELAY] else {
        panic!("wanted the delay in row {DELAY}");
    };
    let cell = slot.automated_rect(LEFT_TIME.0, LEFT_TIME.1, View::default()).expect("wanted the delay's left time cell on its face");
    on_face(app, cell)
}

fn division_cell(app: &mut App) -> (i32, i32) {
    on_face(app, Rect::new(DIVISION_COLUMN * PITCH, (READOUT_ROWS + DIVISION_ROW) * PITCH, DIVISION_SPAN * PITCH + FRAME, CELL))
}

fn readout(app: &mut App) -> (i32, i32) {
    let face = face_rect(&painted(app));
    centre(Rect::new(face.x, face.y, face.w, READOUT_ROWS * PITCH + FRAME))
}

fn caret_shown(app: &mut App) -> bool {
    let fills = painted(app);
    let face = face_rect(&fills);
    let tv = Rect::new(face.x, face.y, face.w, READOUT_ROWS * PITCH + FRAME);
    fills.iter().any(|fill| fill.tone == Tone::Ink100 && fill.rect.w == VALUE_SCALE && fill.rect.h == GLYPH_H * VALUE_SCALE && tv.contains(fill.rect.x, fill.rect.y))
}

fn remembered_delay(app: &mut App) -> Option<(f64, Division)> {
    let text = app.take_requests().into_iter().rev().find_map(|request| if let Request::Remember { json } = request { Some(json) } else { None })?;
    let session = json::parse(&text).expect("wanted the remembered session to parse");
    let project = Project::from_json(session.get("project").expect("wanted the remembered session to hold the project")).expect("wanted the remembered project to read");
    let Row::Effect(slot) = &project.rack(HOST)[DELAY] else {
        panic!("wanted the delay remembered in row {DELAY}");
    };
    let Params::Delay(delay) = &slot.params else {
        panic!("wanted the remembered row to be a delay");
    };
    Some((delay.time_l_ms, delay.division_l))
}

fn left_time(app: &mut App, last: f64) -> f64 {
    remembered_delay(app).map_or(last, |(time, _)| time)
}

#[test]
fn every_spelled_value_reads_back_to_the_same_spelling() {
    let mut failures = Vec::new();
    for format in FORMATS {
        for value in SAMPLES {
            let shown = spell(format, value);
            match read(format, &shown) {
                Some(back) if spell(format, back) == shown => {}
                Some(back) => failures.push(format!("{format:?} spelled {shown} read back as {back} spelling {}", spell(format, back))),
                None => failures.push(format!("{format:?} spelled {shown} could not be read back")),
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn typed_text_reads_in_the_base_unit_with_suffixes_and_words_understood() {
    for (format, text, wanted) in READINGS {
        let got = read(format, text).map(|value| spell(format, value));
        assert_eq!(got.as_deref(), Some(wanted), "wanted {text:?} in {format:?} to read as {wanted}, got {got:?}");
    }
    for (format, text) in UNREADABLE {
        let got = read(format, text);
        assert_eq!(got, None, "wanted {text:?} in {format:?} left unread, got {got:?}");
    }
}

#[test]
fn a_word_cell_matches_its_option_exactly_first_and_then_by_prefix() {
    assert_eq!(option_of(&LABELS, "1/8"), Some(EIGHTH), "wanted 1/8 to land on the plain eighth, not the dotted one");
    assert_eq!(option_of(&LABELS, "1/8."), Some(DOTTED_EIGHTH), "wanted 1/8. to land on the dotted eighth");
    assert_eq!(option_of(&MODES, "ta"), Some(1), "wanted ta to complete to TAPE");
    assert_eq!(option_of(&OFF_ON, "on"), Some(1), "wanted on to land on ON");
    assert_eq!(option_of(&OFF_ON, "o"), Some(0), "wanted o to take the first match, OFF");
    assert_eq!(option_of(&LABELS, "9"), None, "wanted 9 to match no division");
    assert_eq!(option_of(&LABELS, ""), None, "wanted nothing typed to match nothing");
}

#[test]
fn a_second_press_on_a_picked_cell_opens_an_entry_and_return_sets_the_typed_time() {
    let project = racked(false);
    let mut app = open(&project);
    let cell = left_time_cell(&mut app, &project);
    press(&mut app, cell);
    assert!(!caret_shown(&mut app), "wanted the first press to pick the cell without opening an entry");
    press(&mut app, cell);
    assert!(caret_shown(&mut app), "wanted the second press to open an entry on the tv");
    typed(&mut app, "250");
    let before = left_time(&mut app, DEFAULT_MS);
    assert_eq!(before, DEFAULT_MS, "wanted the time untouched until Return, got {before} ms");
    key(&mut app, Key::Return);
    assert!(!caret_shown(&mut app), "wanted Return to close the entry");
    let after = left_time(&mut app, before);
    assert_eq!(after, TYPED_MS, "wanted Return to set the left time to {TYPED_MS} ms, got {after} ms");
}

#[test]
fn a_typed_digit_on_a_picked_cell_opens_the_entry_and_a_suffix_picks_the_unit() {
    let project = racked(false);
    let mut app = open(&project);
    let cell = left_time_cell(&mut app, &project);
    press(&mut app, cell);
    typed(&mut app, "0.5s");
    assert!(caret_shown(&mut app), "wanted the first digit to open an entry on the picked cell");
    key(&mut app, Key::Return);
    let after = left_time(&mut app, DEFAULT_MS);
    assert_eq!(after, HALF_SECOND_MS, "wanted 0.5s to set the left time to {HALF_SECOND_MS} ms, got {after} ms");
}

#[test]
fn return_on_a_picked_cell_opens_the_entry_and_out_of_range_values_clamp() {
    let project = racked(false);
    let mut app = open(&project);
    let cell = left_time_cell(&mut app, &project);
    press(&mut app, cell);
    key(&mut app, Key::Return);
    assert!(caret_shown(&mut app), "wanted Return on a picked cell to open an entry");
    typed(&mut app, "9999");
    key(&mut app, Key::Return);
    let after = left_time(&mut app, DEFAULT_MS);
    assert_eq!(after, CLAMPED_MS, "wanted 9999 ms to clamp to the delay's longest time, {CLAMPED_MS} ms, got {after} ms");
}

#[test]
fn escape_and_a_press_elsewhere_leave_the_entry_without_setting_anything() {
    let project = racked(false);
    let mut app = open(&project);
    let cell = left_time_cell(&mut app, &project);
    press(&mut app, cell);
    press(&mut app, cell);
    typed(&mut app, "250");
    key(&mut app, Key::Escape);
    assert!(!caret_shown(&mut app), "wanted Escape to close the entry");
    let after_escape = left_time(&mut app, DEFAULT_MS);
    assert_eq!(after_escape, DEFAULT_MS, "wanted Escape to leave the left time at {DEFAULT_MS} ms, got {after_escape} ms");
    press(&mut app, cell);
    typed(&mut app, "250");
    press(&mut app, ELSEWHERE);
    assert!(!caret_shown(&mut app), "wanted a press elsewhere to close the entry");
    let after_elsewhere = left_time(&mut app, after_escape);
    assert_eq!(after_elsewhere, DEFAULT_MS, "wanted a press elsewhere to leave the left time at {DEFAULT_MS} ms, got {after_elsewhere} ms");
    press(&mut app, cell);
    typed(&mut app, "250");
    let tv = readout(&mut app);
    press(&mut app, tv);
    assert!(!caret_shown(&mut app), "wanted a press on the tv to close the entry");
    let after_tv = left_time(&mut app, after_elsewhere);
    assert_eq!(after_tv, DEFAULT_MS, "wanted a press on the tv to leave the left time at {DEFAULT_MS} ms, got {after_tv} ms");
}

#[test]
fn unreadable_text_keeps_the_entry_open_and_delete_takes_a_character_back() {
    let project = racked(false);
    let mut app = open(&project);
    let cell = left_time_cell(&mut app, &project);
    press(&mut app, cell);
    press(&mut app, cell);
    typed(&mut app, "25x");
    key(&mut app, Key::Return);
    assert!(caret_shown(&mut app), "wanted Return on unreadable text to keep the entry open");
    let unread = left_time(&mut app, DEFAULT_MS);
    assert_eq!(unread, DEFAULT_MS, "wanted unreadable text to set nothing, got {unread} ms");
    key(&mut app, Key::Delete);
    typed(&mut app, "0");
    key(&mut app, Key::Return);
    let after = left_time(&mut app, unread);
    assert_eq!(after, TYPED_MS, "wanted Delete to take the x back so 250 sets {TYPED_MS} ms, got {after} ms");
}

#[test]
fn a_division_typed_by_name_lands_on_a_synced_delay() {
    let project = racked(true);
    let mut app = open(&project);
    let cell = division_cell(&mut app);
    press(&mut app, cell);
    let tv = readout(&mut app);
    press(&mut app, tv);
    key(&mut app, Key::Return);
    assert!(caret_shown(&mut app), "wanted Return on the picked division cell to open an entry");
    typed(&mut app, "1/8");
    key(&mut app, Key::Return);
    let division = remembered_delay(&mut app).map(|(_, division)| division);
    assert_eq!(division, Some(Division::Eighth), "wanted 1/8 typed into the division cell to set an eighth");
}

#[test]
fn keys_stay_with_the_entry_so_space_neither_types_nor_plays() {
    let project = racked(false);
    let mut app = open(&project);
    let cell = left_time_cell(&mut app, &project);
    press(&mut app, cell);
    press(&mut app, cell);
    typed(&mut app, "2");
    key(&mut app, Key::Space);
    typed(&mut app, "50");
    key(&mut app, Key::Return);
    let after = left_time(&mut app, DEFAULT_MS);
    assert_eq!(after, TYPED_MS, "wanted Space swallowed while typing so 250 sets {TYPED_MS} ms, got {after} ms");

}
