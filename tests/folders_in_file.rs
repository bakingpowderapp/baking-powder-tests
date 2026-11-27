use bp::app::{App, Event, Opened, Request};
use bp::json::{self, Json};
use bp::keyboard::Key;
use bp::project::Project;
use support::{HEIGHT, WIDTH, call};

const PATH: &str = "/x/song.json";
const SAMPLES: &str = "/x/Samples";
const SCORES: &str = "/x/Scores";
const LOOPS: &str = "/x/Loops";
const FACTORY: &str = "/x/Factory";
const MANUAL_NAME: &str = "Baking Powder Manual.json";
const WHOLE_SONG: &str = "{}";
const FILE_CELL_CENTRE: (i32, i32) = (60, 26);
const SAVE_AS_DEFAULT_ITEM_CENTRE: (i32, i32) = (60, 261);
const EXPORT_ITEM_CENTRE: (i32, i32) = (60, 325);
const WITHOUT_BOTH_ITEM_CENTRE: (i32, i32) = (404, 389);
const MANUAL_ITEM_CENTRE: (i32, i32) = (404, 421);

fn resumed(folders: &[&str], extra: &str) -> App {
    let listed: Vec<String> = folders.iter().map(|folder| format!("\"{folder}\"")).collect();
    let session = format!("{{\"project\":{},\"automation\":false,\"keyboard\":false,\"palette\":false,\"folders\":[{}]{extra}}}", Project::new().to_json().write(), listed.join(","));
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.draw(WIDTH, HEIGHT);
    app.take_requests();
    app
}

fn parsed(text: &str) -> Json {
    json::parse(text).expect("wanted the payload to parse as JSON")
}

fn strings(json: &Json, key: &str) -> Vec<String> {
    json.get(key).and_then(Json::as_array).map_or_else(Vec::new, |items| items.iter().filter_map(Json::as_str).map(String::from).collect())
}

fn names(json: &Json) -> Vec<String> {
    let Json::Object(fields) = json else {
        panic!("wanted the document to be an object");
    };
    fields.iter().map(|(key, _)| key.clone()).collect()
}

fn at(keys: &[String], key: &str) -> usize {
    keys.iter().position(|known| known == key).unwrap_or_else(|| panic!("wanted {key} among {keys:?}"))
}

fn saved(app: &mut App) -> Json {
    app.event(Event::Key { key: Key::Char('s'), shift: false, command: true, repeat: false });
    let payloads: Vec<String> = app.take_requests().into_iter().filter_map(|request| match request {
        Request::Write { json, .. } | Request::SaveAs { json, .. } => Some(json),
        _ => None,
    }).collect();
    let [payload] = payloads.as_slice() else {
        panic!("wanted exactly one save request per press, got {}", payloads.len());
    };
    parsed(payload)
}

fn read(app: &mut App) -> Json {
    let reply = call(app, "read", WHOLE_SONG);
    assert!(!reply.failed, "wanted the whole song read, got {}", reply.text);
    parsed(&reply.text)
}

fn remembered(requests: &[Request]) -> Json {
    let text = requests.iter().rev().find_map(|request| if let Request::Remember { json } = request { Some(json) } else { None }).expect("wanted the session remembered");
    parsed(text)
}

fn opened(app: &mut App, text: &str) -> Vec<Request> {
    assert_eq!(app.opened(String::from(PATH), Some(text), None), Opened::Project, "wanted the file to open as a project");
    app.take_requests()
}

fn with_folders(folders: &[&str]) -> String {
    let mut document = Project::new().to_json();
    if let Json::Object(fields) = &mut document {
        fields.push((String::from("folders"), Json::Array(folders.iter().map(|folder| Json::String(String::from(*folder))).collect())));
    }
    document.write()
}

fn clicked(app: &mut App, path: &[(i32, i32)]) -> Vec<Request> {
    for &(x, y) in path {
        app.event(Event::Move { x, y });
        app.event(Event::Down { x, y, clicks: 1 });
        app.event(Event::Up { x, y });
        app.draw(WIDTH, HEIGHT);
    }
    app.take_requests()
}

fn handed_over(requests: Vec<Request>, after: &str) -> (String, Json) {
    let files: Vec<(String, String)> = requests.into_iter().filter_map(|request| match request {
        Request::Preset { name, json } => Some((name, json)),
        _ => None,
    }).collect();
    let [(name, json)] = files.as_slice() else {
        panic!("wanted one file handed over after {after}, got {}", files.len());
    };
    (name.clone(), parsed(json))
}

#[test]
fn a_saved_project_holds_no_folders_whatever_the_browser_holds() {
    let file = saved(&mut resumed(&[SAMPLES, SCORES], ""));
    assert!(file.get("folders").is_none(), "wanted the browser's folders left out of the saved file, got {:?}", strings(&file, "folders"));
    let versions = file.get("versions").and_then(Json::as_array).unwrap_or(&[]);
    let [version] = versions else {
        panic!("wanted the save to mint one version, got {}", versions.len());
    };
    assert!(version.get("project").and_then(|project| project.get("folders")).is_none(), "wanted the version to hold the project without folders");
}

#[test]
fn an_agent_reads_the_added_folders_ahead_of_the_tracks_and_never_the_pinned_factory() {
    let mut app = resumed(&[SAMPLES, SCORES], "");
    app.factory(String::from(FACTORY));
    app.take_requests();
    let document = read(&mut app);
    assert_eq!(strings(&document, "folders"), vec![SAMPLES, SCORES], "wanted the document an agent reads to list both folders in the browser's order, the factory's left out, got {:?}", strings(&document, "folders"));
    let keys = names(&document);
    assert!(at(&keys, "scale") < at(&keys, "folders") && at(&keys, "folders") < at(&keys, "tracks"), "wanted folders read between the key and the tracks, got {keys:?}");
}

#[test]
fn an_empty_browser_writes_no_folders() {
    let file = saved(&mut resumed(&[], ""));
    assert!(file.get("folders").is_none(), "wanted no folders key from an empty browser, got {:?}", file.get("folders").map(Json::write));
}

#[test]
fn opening_a_file_adds_none_of_the_folders_it_lists() {
    let mut app = resumed(&[SAMPLES], "");
    app.factory(String::from(FACTORY));
    app.take_requests();
    let requests = opened(&mut app, &with_folders(&[SCORES, SAMPLES, FACTORY, SCORES]));
    let asked: Vec<&Request> = requests.iter().filter(|request| matches!(request, Request::List { .. } | Request::Scan { .. } | Request::Read { .. })).collect();
    assert!(asked.is_empty(), "wanted nothing listed, scanned or read for the folders the file lists, got {asked:?}");
    let session = remembered(&requests);
    assert_eq!(strings(&session, "folders"), vec![SAMPLES], "wanted the browser left with the one folder it had, none taken from the file, got {:?}", strings(&session, "folders"));
    assert!(strings(&session, "unfolded").is_empty(), "wanted nothing unfolded after the open, got {:?}", strings(&session, "unfolded"));
}

#[test]
fn a_file_naming_the_hidden_factory_and_another_folder_adds_neither() {
    let mut app = resumed(&[], ",\"factory\":false");
    app.factory(String::from(FACTORY));
    app.take_requests();
    let session = remembered(&opened(&mut app, &with_folders(&[FACTORY, LOOPS])));
    assert!(session.get("folders").is_none(), "wanted the browser left empty, neither the hidden factory's path nor the other folder taken from the file, got {:?}", strings(&session, "folders"));
}

#[test]
fn a_saved_file_reopened_beside_other_folders_brings_none_and_takes_none_away() {
    let text = saved(&mut resumed(&[SAMPLES, SCORES], "")).write();
    let mut app = resumed(&[LOOPS], "");
    let session = remembered(&opened(&mut app, &text));
    assert_eq!(strings(&session, "folders"), vec![LOOPS], "wanted the browser to keep its own folder and gain none from the opened project, got {:?}", strings(&session, "folders"));
    let again = saved(&mut app);
    assert!(again.get("folders").is_none(), "wanted the next save to hold no folders either, got {:?}", strings(&again, "folders"));
}

#[test]
fn save_as_default_leaves_the_folders_out() {
    let mut app = resumed(&[SAMPLES], "");
    let templates: Vec<String> = clicked(&mut app, &[FILE_CELL_CENTRE, SAVE_AS_DEFAULT_ITEM_CENTRE]).into_iter().filter_map(|request| match request {
        Request::Default { json: Some(json) } => Some(json),
        _ => None,
    }).collect();
    let [template] = templates.as_slice() else {
        panic!("wanted one default written after File then Save As Default, got {}", templates.len());
    };
    assert!(parsed(template).get("folders").is_none(), "wanted the default project without folders, got {:?}", strings(&parsed(template), "folders"));
}

#[test]
fn an_exported_project_holds_no_folders() {
    let mut app = resumed(&[SAMPLES, SCORES], "");
    let (_, file) = handed_over(clicked(&mut app, &[FILE_CELL_CENTRE, EXPORT_ITEM_CENTRE, WITHOUT_BOTH_ITEM_CENTRE]), "File, Export Project then Without Both");
    assert!(file.get("folders").is_none(), "wanted the browser's folders left out of the export, got {:?}", strings(&file, "folders"));
    assert!(file.get("versions").is_none() && file.get("waves").is_none(), "wanted the bare export without versions or waves, got keys {:?}", names(&file));
}

#[test]
fn the_manual_explains_folders_beside_the_transport() {
    let mut app = resumed(&[], "");
    let (name, manual) = handed_over(clicked(&mut app, &[FILE_CELL_CENTRE, EXPORT_ITEM_CENTRE, MANUAL_ITEM_CENTRE]), "File, Export Project then Manual");
    assert_eq!(name, MANUAL_NAME, "wanted the manual offered under its own name");
    let line = manual.get("folders").and_then(Json::as_str).unwrap_or_else(|| panic!("wanted a folders line in the manual, got keys {:?}", names(&manual)));
    assert!(line.contains("browser") && line.contains("absolute path"), "wanted the line to say the folders are the browser's absolute paths, got {line:?}");
    let keys = names(&manual);
    assert!(at(&keys, "loopEndBeat") < at(&keys, "folders") && at(&keys, "folders") < at(&keys, "tracks"), "wanted folders explained after the loop and before the tracks, got {keys:?}");
}
