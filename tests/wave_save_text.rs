use bp::app::{App, Event, Opened, Request};
use bp::draw::{Fill, Rect, Tone};
use bp::face::{CELL, FRAME, PITCH};
use bp::grid::PLAYHEAD_W;
use bp::json::{self, Json};
use bp::keyboard::Key;
use bp::project::{Clip, Project, TICKS_PER_BEAT, Track};
use bp::wav::Wave;
use support::{HEIGHT, RATE, WIDTH, centre, clip_block, painted, wav};

const SOURCE: &str = "/x/tone.wav";
const NAME: &str = "tone";
const REVERSED: &str = "tone reverse";
const FILE: &str = "/tmp/wave-save-text-under-test.json";
const OTHER_FILE: &str = "/tmp/wave-save-text-other-under-test.json";
const CLIP_BEATS: i32 = 2;
const FRAMES: usize = 48_000;
const BAND_CELLS: i32 = 10;
const ROWS_UNDER_WAVE: i32 = 3;
const SHOWN: usize = 120;
const SAVED_AT: &str = "\"savedAt\": ";

fn tone() -> Vec<i16> {
    (0..FRAMES).map(|frame| ((frame as f64 * 0.013).sin() * 12_000.0 * (1.0 - frame as f64 / FRAMES as f64)).round() as i16).collect()
}

fn other_tone() -> Vec<i16> {
    (0..FRAMES / 2).map(|frame| ((frame as f64 * 0.031).cos() * 9_000.0).round() as i16).collect()
}

fn project() -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0] = Track::audio();
    project.tracks[0].clips.push(Clip::audio(0, CLIP_BEATS * TICKS_PER_BEAT, String::from(NAME), String::from(NAME)));
    project
}

fn fresh() -> App {
    let mut app = App::new();
    app.draw(WIDTH, HEIGHT);
    app.take_requests();
    app
}

fn open() -> App {
    let session = format!("{{\"project\":{},\"keyboard\":false,\"palette\":false,\"automation\":false,\"beat_w\":120,\"origins\":[{{\"name\":\"{NAME}\",\"from\":\"{SOURCE}\"}}]}}", project().to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    loop {
        let requests = app.take_requests();
        if requests.is_empty() {
            break;
        }
        for request in requests {
            if let Request::Read { path } = request {
                app.read(path, Some(wav(&tone())));
            }
        }
    }
    app
}

fn reopened(text: &str) -> App {
    let mut app = fresh();
    assert_eq!(app.opened(String::from(FILE), Some(text), None), Opened::Project, "wanted the saved text to open as a project");
    app.take_requests();
    app
}

fn command(app: &mut App, letter: char, shift: bool) {
    app.event(Event::Key { key: Key::Char(letter), shift, command: true, repeat: false });
}

fn save(app: &mut App) -> String {
    command(app, 's', false);
    app.take_requests().into_iter().find_map(|request| match request {
        Request::Write { json, .. } | Request::SaveAs { json, .. } => Some(json),
        _ => None,
    }).expect("wanted a save request after Cmd+S")
}

fn click(app: &mut App, (x, y): (i32, i32)) {
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
}

fn editor_body(fills: &[Fill]) -> Rect {
    let inked = |fill: &&Fill| fill.tone == Tone::Ink100;
    let shelf_under = |line: Rect| fills.iter().filter(inked).find(|fill| fill.rect.h == FRAME && fill.rect.w > CELL && fill.rect.x == line.x - FRAME && fill.rect.y == line.y + line.h).map(|fill| fill.rect);
    let lines: Vec<(Rect, Rect)> = fills.iter().filter(inked).filter(|fill| fill.rect.w == PLAYHEAD_W && fill.rect.h >= CELL).filter_map(|fill| Some((fill.rect, shelf_under(fill.rect)?))).collect();
    let [(entry, shelf)] = lines.as_slice() else {
        panic!("wanted one entry line standing on the span lane's frame, found {}: {lines:?}", lines.len());
    };
    Rect::new(entry.x, entry.y, shelf.w - 2 * FRAME, entry.h)
}

fn press_reverse(app: &mut App) {
    let clip = clip_block(&painted(app));
    click(app, centre(clip));
    let body = editor_body(&painted(app));
    click(app, centre(Rect::new(body.x - FRAME + (BAND_CELLS + 1) * PITCH, body.y + body.h + ROWS_UNDER_WAVE * PITCH, CELL, CELL)));
}

fn parsed(text: &str) -> Json {
    json::parse(text).expect("wanted the save to parse")
}

fn pool(text: &str) -> Vec<Wave> {
    parsed(text).get("waves").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(Wave::from_json).collect()
}

fn names(pool: &[Wave]) -> Vec<&str> {
    pool.iter().map(|wave| wave.name.as_str()).collect()
}

fn clip_waves(text: &str) -> Vec<String> {
    parsed(text).get("tracks").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(|track| track.get("clips").and_then(Json::as_array)).flatten().filter_map(|clip| clip.get("wave")?.get("name")?.as_str().map(String::from)).collect()
}

fn says_when_saved(line: &str) -> bool {
    line.trim_start().starts_with(SAVED_AT)
}

fn expect_same_text(what: &str, wanted: &str, got: &str) {
    if let Some((at, (a, b))) = wanted.lines().zip(got.lines()).enumerate().find(|(_, (a, b))| a != b && !(says_when_saved(a) && says_when_saved(b))) {
        panic!("{what}: line {at} wanted {:?} got {:?}", a.chars().take(SHOWN).collect::<String>(), b.chars().take(SHOWN).collect::<String>());
    }
    assert_eq!(got.len(), wanted.len(), "{what}: wanted {} bytes of text, got {}", wanted.len(), got.len());
}

fn expect_samples(what: &str, wave: Option<&Wave>, wanted: &[i16]) {
    let Some(wave) = wave else {
        panic!("{what}: wanted a wave of {} samples in the pool, found none", wanted.len());
    };
    if let Some(at) = wave.pcm.iter().zip(wanted).position(|(a, b)| a != b) {
        panic!("{what}: sample {at} wanted {} got {}", wanted[at], wave.pcm[at]);
    }
    assert_eq!(wave.pcm.len(), wanted.len(), "{what}: wanted {} samples, got {}", wanted.len(), wave.pcm.len());
}

#[test]
fn a_wave_saves_the_same_text_twice_and_again_after_reopening() {
    let mut app = open();
    let first = save(&mut app);
    expect_samples("the wave read from its file", pool(&first).iter().find(|wave| wave.name == NAME), &tone());
    expect_same_text("a second save without reopening", &first, &save(&mut app));
    expect_same_text("the save written again from its own parse", &first, &parsed(&first).write());
    let mut copy = reopened(&first);
    expect_same_text("a save of the reopened copy", &first, &save(&mut copy));
    expect_same_text("a second save of the reopened copy", &first, &save(&mut copy));
}

#[test]
fn reversing_the_clip_saves_its_wave_backwards_and_undo_saves_the_first_text_again() {
    let mut app = open();
    let first = save(&mut app);
    press_reverse(&mut app);
    let reversed = save(&mut app);
    assert_eq!(clip_waves(&reversed), vec![String::from(REVERSED)], "wanted the clip to name the reversed wave after REV, got {:?}", clip_waves(&reversed));
    let waves = pool(&reversed);
    assert_eq!(names(&waves), vec![REVERSED], "wanted only the reversed wave in the pool after REV, got {:?}", names(&waves));
    let backwards: Vec<i16> = tone().into_iter().rev().collect();
    expect_samples("the reversed wave", waves.first(), &backwards);
    expect_same_text("a second save after REV", &reversed, &save(&mut app));
    command(&mut app, 'z', false);
    app.take_requests();
    expect_same_text("a save after undoing REV", &first, &save(&mut app));
    command(&mut app, 'z', true);
    app.take_requests();
    expect_same_text("a save after redoing REV", &reversed, &save(&mut app));
    let mut copy = reopened(&reversed);
    let again = save(&mut copy);
    expect_same_text("a save of the reopened reversed copy", &reversed, &again);
    expect_samples("the reversed wave after reopening", pool(&again).first(), &backwards);
}

#[test]
fn a_wave_of_the_same_name_from_another_file_saves_its_own_samples() {
    let mut app = open();
    let first = save(&mut app);
    let mut document = parsed(&first);
    if let Json::Object(fields) = &mut document {
        fields.retain(|(key, _)| key != "waves");
        fields.push((String::from("waves"), Json::Array(vec![Wave::sound(String::from(NAME), f64::from(RATE), 1, other_tone()).to_json()])));
    }
    let other = document.write();
    assert_eq!(app.opened(String::from(OTHER_FILE), Some(&other), None), Opened::Project, "wanted the other file to open as a project");
    app.take_requests();
    let saved = save(&mut app);
    expect_samples("the other file's wave", pool(&saved).iter().find(|wave| wave.name == NAME), &other_tone());
    expect_same_text("a save of the other file", &other, &saved);
}
