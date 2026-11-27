use bp::app::{App, Event, Opened, Request};
use bp::keyboard::Key;
use bp::font;
use bp::json::{self, Json};
use support::{HEIGHT, WIDTH};

const PATH: &str = "/tmp/versions-under-test.json";
const FILE_CELL_CENTRE: (i32, i32) = (60, 26);
const SETTINGS_ITEM_CENTRE: (i32, i32) = (60, 389);
const SAVES_ROWS_TOP: i32 = 90;
const ROW_PITCH: i32 = 33;
const ROW_MIDDLE: i32 = 14;
const CAP: usize = 50;

fn fresh() -> App {
    let mut app = App::new();
    app.draw(WIDTH, HEIGHT);
    app.take_requests();
    app
}

fn saves_column_x() -> i32 {
    12 + 2 * (font::width("FLATTEN GROUPS AS ONE", 2) + 2 * 10) + 2 * (2 * 8) + 20
}

fn click(app: &mut App, x: i32, y: i32) {
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    app.draw(WIDTH, HEIGHT);
    app.take_requests();
}

fn vary(app: &mut App, wet: f64) {
    let preset = format!("{{\"kind\": \"rack\", \"rack\": [{{\"kind\": \"core\"}}, {{\"kind\": \"freak\", \"wet\": {wet}}}]}}");
    let opened = app.opened(String::from("marker.json"), Some(&preset), None);
    assert_eq!(opened, Opened::Preset, "wanted the marker rack to load as a preset");
    let sessions = app.take_requests().into_iter().filter(|request| matches!(request, Request::Remember { .. })).count();
    assert!(sessions > 0, "wanted a session write after the document changed");
}

fn press_save(app: &mut App) -> (String, u64) {
    app.event(Event::Key { key: Key::Char('s'), shift: false, command: true, repeat: false });
    let payloads: Vec<(String, u64)> = app.take_requests().into_iter().filter_map(|request| match request {
        Request::Write { json, save, .. } | Request::SaveAs { json, save } => Some((json, save)),
        _ => None,
    }).collect();
    assert_eq!(payloads.len(), 1, "wanted exactly one save request per press, got {}", payloads.len());
    payloads.into_iter().next().expect("a save payload")
}

fn confirm(app: &mut App, save: u64) -> String {
    app.saved(String::from(PATH), save);
    let sessions: Vec<String> = app.take_requests().into_iter().filter_map(|request| match request {
        Request::Remember { json } => Some(json),
        _ => None,
    }).collect();
    sessions.into_iter().next_back().unwrap_or_default()
}

fn undo(app: &mut App) {
    app.event(Event::Key { key: Key::Char('z'), shift: false, command: true, repeat: false });
    app.take_requests();
}

fn parsed(payload: &str) -> Json {
    json::parse(payload).expect("the payload parses as JSON")
}

fn versions(file: &Json) -> &[Json] {
    file.get("versions").and_then(Json::as_array).unwrap_or(&[])
}

fn wet_of(document: &Json) -> f64 {
    document.get("tracks")
        .and_then(Json::as_array)
        .and_then(|tracks| tracks.first())
        .and_then(|track| track.get("rack"))
        .and_then(Json::as_array)
        .and_then(|rack| rack.get(2))
        .and_then(|row| row.get("wet"))
        .and_then(Json::as_f64)
        .expect("the marker Freak carries its wet")
}

fn without_versions(file: &Json) -> Json {
    match file {
        Json::Object(fields) => Json::Object(fields.iter().filter(|(key, _)| key.as_str() != "versions").cloned().collect()),
        other => other.clone(),
    }
}

#[test]
fn the_first_save_leads_with_the_document_and_mints_version_one() {
    let mut app = fresh();
    vary(&mut app, 0.42);
    let (text, save) = press_save(&mut app);
    confirm(&mut app, save);
    let file = parsed(&text);
    let minted = versions(&file);
    assert_eq!(minted.len(), 1, "wanted one version after the first save, got {}", minted.len());
    let inner = minted[0].get("project").expect("the version wraps a project");
    assert_eq!(*inner, without_versions(&file), "wanted the newest version to equal the file's own document");
    assert_eq!(wet_of(inner), 0.42, "wanted the saved marker inside version one");
}

#[test]
fn an_unchanged_second_save_burns_no_slot() {
    let mut app = fresh();
    vary(&mut app, 0.42);
    let (first, save) = press_save(&mut app);
    confirm(&mut app, save);
    let (second, save) = press_save(&mut app);
    confirm(&mut app, save);
    assert_eq!(second, first, "wanted an unchanged document to rewrite the file byte for byte");
    assert_eq!(versions(&parsed(&second)).len(), 1, "wanted still one version after saving twice");
}

#[test]
fn every_distinct_save_appends_newest_last() {
    let mut app = fresh();
    for wet in [0.30, 0.55, 0.80] {
        vary(&mut app, wet);
        let (_, save) = press_save(&mut app);
        confirm(&mut app, save);
    }
    let file = parsed(&press_save(&mut app).0);
    let kept: Vec<f64> = versions(&file).iter().map(|entry| wet_of(entry.get("project").expect("a project per version"))).collect();
    assert_eq!(kept, vec![0.30, 0.55, 0.80], "wanted the three saves oldest to newest");
    assert_eq!(wet_of(&without_versions(&file)), 0.80, "wanted the file's document to be the newest save");
}

#[test]
fn fifty_versions_cap_drops_oldest_first() {
    let mut app = fresh();
    let mut last = String::new();
    for step in 0..55 {
        vary(&mut app, 0.30 + 0.01 * f64::from(step));
        let (text, save) = press_save(&mut app);
        last = text;
        confirm(&mut app, save);
    }
    let file = parsed(&last);
    let kept = versions(&file);
    assert_eq!(kept.len(), CAP, "wanted the cap of {CAP} versions, got {}", kept.len());
    let oldest = wet_of(kept[0].get("project").expect("a project per version"));
    assert_eq!(oldest, 0.30 + 0.01 * 5.0, "wanted saves one through five dropped");
    let newest = wet_of(kept[CAP - 1].get("project").expect("a project per version"));
    assert_eq!(newest, 0.84, "wanted the newest save kept last");
}

#[test]
fn a_cancelled_save_as_leaves_no_phantom_version() {
    let mut app = fresh();
    vary(&mut app, 0.33);
    press_save(&mut app);
    vary(&mut app, 0.66);
    let (text, save) = press_save(&mut app);
    confirm(&mut app, save);
    let file = parsed(&text);
    let kept = versions(&file);
    assert_eq!(kept.len(), 1, "wanted only the confirmed save, got {} versions", kept.len());
    assert_eq!(wet_of(kept[0].get("project").expect("a project per version")), 0.66, "wanted the confirmed document, not the cancelled one");
}

#[test]
fn a_reopened_file_continues_its_history() {
    let mut app = fresh();
    vary(&mut app, 0.25);
    let (_, save) = press_save(&mut app);
    confirm(&mut app, save);
    vary(&mut app, 0.50);
    let (file_text, save) = press_save(&mut app);
    confirm(&mut app, save);
    let mut reopened = fresh();
    assert_eq!(reopened.opened(String::from(PATH), Some(&file_text), None), Opened::Project, "wanted the saved file to open as a project");
    reopened.take_requests();
    vary(&mut reopened, 0.75);
    let file = parsed(&press_save(&mut reopened).0);
    let kept = versions(&file);
    assert_eq!(kept.len(), 3, "wanted the reopened history plus one, got {} versions", kept.len());
    assert_eq!(kept[0], versions(&parsed(&file_text))[0], "wanted the oldest carried entry untouched");
    assert_eq!(wet_of(kept[2].get("project").expect("a project per version")), 0.75, "wanted the new save appended last");
}

#[test]
fn clicking_an_older_save_restores_it_and_saving_bumps_it_newest() {
    let mut app = fresh();
    vary(&mut app, 0.25);
    let (_, save) = press_save(&mut app);
    confirm(&mut app, save);
    vary(&mut app, 0.65);
    let (_, save) = press_save(&mut app);
    confirm(&mut app, save);
    click(&mut app, FILE_CELL_CENTRE.0, FILE_CELL_CENTRE.1);
    click(&mut app, SETTINGS_ITEM_CENTRE.0, SETTINGS_ITEM_CENTRE.1);
    click(&mut app, saves_column_x() + 20, SAVES_ROWS_TOP + ROW_PITCH + ROW_MIDDLE);
    let (text, save) = press_save(&mut app);
    confirm(&mut app, save);
    let file = parsed(&text);
    let kept: Vec<f64> = versions(&file).iter().map(|entry| wet_of(entry.get("project").expect("a project per version"))).collect();
    assert_eq!(kept, vec![0.25, 0.65, 0.25], "wanted the restored save re-minted as the newest version");
    assert_eq!(wet_of(&without_versions(&file)), 0.25, "wanted the restored document leading the file");
    undo(&mut app);
    let after = parsed(&press_save(&mut app).0);
    assert_eq!(wet_of(&without_versions(&after)), 0.65, "wanted undo to return the document from before the restore");
}

#[test]
fn a_resumed_session_carries_the_versions() {
    let mut app = fresh();
    vary(&mut app, 0.40);
    let (file_text, save) = press_save(&mut app);
    let session = confirm(&mut app, save);
    let mut relaunched = fresh();
    relaunched.resume(&session);
    relaunched.take_requests();
    relaunched.event(Event::Key { key: Key::Char('s'), shift: false, command: true, repeat: false });
    let writes: Vec<(String, String)> = relaunched.take_requests().into_iter().filter_map(|request| match request {
        Request::Write { path, json, .. } => Some((path, json)),
        _ => None,
    }).collect();
    assert_eq!(writes.len(), 1, "wanted the resumed session to save straight back to its path");
    assert_eq!(writes[0].0, PATH, "wanted the remembered path");
    assert_eq!(writes[0].1, file_text, "wanted the resumed save to rewrite the same single version");
}
