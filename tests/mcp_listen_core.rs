use bp::app::{App, Request};
use bp::arp::Arp;
use bp::fx::Kind;
use bp::json::{self, Json};
use bp::mcp::{Call, Reply};
use bp::project::{Clip, Project, Row};
use std::path::Path;
use std::time::Duration;
use support::{CHORD, CORE_ROW, HEIGHT, LEVEL, WIDTH, core, half_wet, midi};

const QUIET: f64 = 0.125;
const SECOND_CORE: usize = CORE_ROW + 1;
const DELAY_ROW: usize = SECOND_CORE + 1;
const HEADER: usize = 68;
const PATIENCE: Duration = Duration::from_secs(120);
const TICK: f32 = 0.01;
const PAUSE: Duration = Duration::from_millis(2);

fn folder(name: &str) -> String {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("wanted {}: {error}", root.display()));
    root.to_string_lossy().into_owned()
}

fn open(root: &str, rack: Vec<Row>) -> App {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = rack;
    project.tracks[0].clips.push(Clip::chord(0, String::new(), &CHORD));
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false}}", project.to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.draw(WIDTH, HEIGHT);
    app.add_folder(String::from(root));
    app.take_requests();
    app
}

fn both(root: &str) -> App {
    open(root, vec![midi(Arp::DEFAULT), core(LEVEL), core(QUIET), half_wet(Kind::Delay)])
}

fn listened(app: &mut App, args: &str) -> (Reply, Vec<Vec<u8>>) {
    let (listen, waiting) = Call::new("listen", json::parse(args).unwrap_or_else(|| panic!("wanted {args} to parse")));
    app.deliver(listen);
    let answer = std::thread::spawn(move || waiting.reply(PATIENCE));
    let mut files = Vec::new();
    while !answer.is_finished() {
        app.tick(TICK);
        for request in app.take_requests() {
            if let Request::Exported { path, bytes } = request {
                app.exported(path, true);
                files.push(bytes);
            }
        }
        std::thread::sleep(PAUSE);
    }
    (answer.join().expect("wanted the listen answered"), files)
}

fn kept(app: &mut App, root: &str, name: &str, more: &str) -> (Json, Vec<u8>) {
    let (reply, files) = listened(app, &format!(r#"{{"track":0{more},"path":"{root}/{name}.wav"}}"#));
    assert!(!reply.failed, "wanted the listen answered, got {}", reply.text);
    let [bytes] = files.as_slice() else {
        panic!("wanted one wave kept by the listen, got {}", files.len());
    };
    assert!(bytes[HEADER..].iter().any(|byte| *byte != 0), "wanted the kept wave to hold sound");
    (json::parse(&reply.text).unwrap_or_else(|| panic!("wanted the listen to answer JSON, got {}", reply.text)), bytes.clone())
}

fn refused(app: &mut App, args: &str) -> String {
    let (reply, files) = listened(app, args);
    assert!(reply.failed && files.is_empty(), "wanted listen {args} refused and no wave kept, got {}", reply.text);
    reply.text
}

fn apart(got: &[u8], wanted: &[u8]) -> Option<usize> {
    got.iter().zip(wanted).position(|(a, b)| a != b).or((got.len() != wanted.len()).then_some(got.len().min(wanted.len())))
}

#[test]
fn a_listen_with_track_and_row_hears_that_core_alone_its_tracks_effects_passed_over() {
    let root = folder("listen_core_alone");
    let (heard, first) = kept(&mut both(&root), &root, "first", &format!(r#","row":{CORE_ROW}"#));
    assert_eq!((heard.get("row").and_then(Json::as_f64), heard.get("effects").and_then(Json::as_bool)), (Some(CORE_ROW as f64), Some(false)), "wanted the reply to name the Core heard alone and say its effects were passed over, got {}", heard.write());
    let (_, alone) = kept(&mut open(&root, vec![midi(Arp::DEFAULT), core(LEVEL)]), &root, "alone", "");
    assert_eq!(apart(&first, &alone), None, "wanted the first Core heard alone byte for byte as a track that holds that Core and nothing else, first byte apart");
    let (_, second) = kept(&mut both(&root), &root, "second", &format!(r#","row":{SECOND_CORE}"#));
    let (_, other) = kept(&mut open(&root, vec![midi(Arp::DEFAULT), core(0.0), core(QUIET)]), &root, "other", "");
    assert_eq!(apart(&second, &other), None, "wanted the second Core heard alone byte for byte as the track with the first Core silent and no effects, first byte apart");
    assert!(apart(&first, &second).is_some(), "wanted the two Cores to sound apart, so each listen proves which was heard");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn effects_true_keeps_the_tracks_effects_on_the_core_heard_alone() {
    let root = folder("listen_core_effects");
    let (heard, through) = kept(&mut both(&root), &root, "through", &format!(r#","row":{CORE_ROW},"effects":true"#));
    assert_eq!(heard.get("effects").and_then(Json::as_bool), Some(true), "wanted the reply to say the effects were kept, got {}", heard.write());
    let (_, wanted) = kept(&mut open(&root, vec![midi(Arp::DEFAULT), core(LEVEL), core(0.0), half_wet(Kind::Delay)]), &root, "wanted", "");
    assert_eq!(apart(&through, &wanted), None, "wanted the first Core heard through the delay byte for byte as the track with the second Core silent, first byte apart");
    let (_, dry) = kept(&mut both(&root), &root, "dry", &format!(r#","row":{CORE_ROW}"#));
    assert!(apart(&through, &dry).is_some(), "wanted the delay heard with effects true and not without");
    let (whole, _) = kept(&mut both(&root), &root, "whole", "");
    assert!(whole.get("row").is_none() && whole.get("effects").is_none(), "wanted a listen to the whole track to name no row, got {}", whole.write());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn row_is_refused_without_its_track_at_a_row_that_is_no_core_and_effects_without_row() {
    let root = folder("listen_core_refused");
    let mut app = both(&root);
    let trackless = refused(&mut app, &format!(r#"{{"row":{CORE_ROW}}}"#));
    assert!(trackless.contains("pass track with it"), "wanted row without track refused, got {trackless}");
    let delay = refused(&mut app, &format!(r#"{{"track":0,"row":{DELAY_ROW}}}"#));
    assert!(delay.contains(&format!("tracks/0/rack/{DELAY_ROW} is not a Core row")), "wanted the delay's row refused, got {delay}");
    let effects = refused(&mut app, r#"{"track":0,"effects":true}"#);
    assert!(effects.contains("effects goes with row"), "wanted effects without row refused, got {effects}");
    let _ = std::fs::remove_dir_all(&root);
}
