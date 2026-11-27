use bp::app::{App, Request};
use bp::arp::Arp;
use bp::fx::Kind;
use bp::json::{self, Json};
use bp::project::{Aim, Host, Keeping, LaneLfo, Preset, Project, Row, Sound};
use std::path::Path;
use support::{LEVEL, call, core, effect, midi, unseeded};

const DELAY: usize = 2;
const AMOUNT: f64 = 0.4;
const FEEDBACK: (&str, &str) = ("LOOP", "FEEDBACK");
const WORDS: &str = "A short echo";
const TWELVE_WORDS: &str = "one two three four five six seven eight nine ten eleven twelve";
const AN_OLD_PRESET: &str = r#"{"kind":"delay","description":"An old echo"}"#;
const A_SONG: &str = r#"{"kind":"project","tracks":[{"name":"x"}]}"#;

fn song() -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), core(LEVEL), effect(Kind::Delay), effect(Kind::Reverb)];
    let mut lfo = LaneLfo::blank(Aim::knob(DELAY, FEEDBACK.0, FEEDBACK.1));
    lfo.amount = AMOUNT;
    project.tracks[0].automation.lfos.push(lfo);
    project.master.rack = vec![effect(Kind::Compressor)];
    let text = project.to_json().write();
    Project::from_json(&json::parse(&text).expect("wanted the song to parse")).expect("wanted the song to read")
}

fn folder(name: &str) -> String {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("wanted {}: {error}", root.display()));
    root.to_string_lossy().into_owned()
}

fn open(root: &str) -> App {
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false}}", song().to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.add_folder(String::from(root));
    app.take_requests();
    app
}

fn kept(app: &mut App) -> (Vec<(String, String)>, Vec<String>) {
    let (mut files, mut listed) = (Vec::new(), Vec::new());
    for request in app.take_requests() {
        match request {
            Request::Exported { path, bytes } => files.push((path, String::from_utf8(bytes).expect("wanted the preset written as text"))),
            Request::List { path } => listed.push(path),
            _ => {}
        }
    }
    (files, listed)
}

fn saved(app: &mut App, args: &str) -> (String, Json, String) {
    let tracks = call(app, "read", r#"{"path":"tracks"}"#).text;
    let reply = call(app, "save", args);
    assert!(!reply.failed, "wanted save {args} taken, got {}", reply.text);
    assert_eq!(call(app, "read", r#"{"path":"tracks"}"#).text, tracks, "wanted the song left as it was by a preset saved from it");
    let (files, _) = kept(app);
    let [(path, text)] = files.as_slice() else {
        panic!("wanted one file handed to the shell, got {}", files.len());
    };
    (path.clone(), json::parse(text).unwrap_or_else(|| panic!("wanted the preset to parse, got {text}")), reply.text)
}

fn refused(app: &mut App, args: &str) -> String {
    let reply = call(app, "save", args);
    assert!(reply.failed, "wanted save {args} refused, got {}", reply.text);
    let (files, _) = kept(app);
    assert!(files.is_empty(), "wanted no file handed over for a refused save, got {}", files.len());
    reply.text
}

fn asked(at: &str, path: &str, more: &str) -> String {
    format!(r#"{{"preset":"{at}","path":"{path}","description":"{WORDS}"{more}}}"#)
}

fn seedless(preset: Preset) -> Preset {
    match preset {
        Preset::Rack(sound) => Preset::Rack(Sound { rack: unseeded(&sound.rack), ..sound }),
        other => other,
    }
}

fn read_as(preset: &Json) -> Preset {
    seedless(Preset::from_json(preset).unwrap_or_else(|| panic!("wanted the file to read as a preset, got {}", preset.write())))
}

fn built(host: Host, keeping: Keeping) -> Preset {
    seedless(song().preset_of(host, keeping).expect("wanted the song to hold what the preset keeps"))
}

#[test]
fn a_rack_row_is_saved_as_a_one_row_preset_with_its_lfo_lanes_and_its_description() {
    let root = folder("save_preset_row");
    let mut app = open(&root);
    let path = format!("{root}/Slap.json");
    let (landed, preset, reply) = saved(&mut app, &asked("tracks/0/rack/2", &path, ""));
    assert_eq!(landed, path, "wanted the preset handed over for the path asked");
    assert_eq!(reply, format!("Writing the delay row at tracks/0/rack/2 as a preset to {path}; the song stays as it is"), "wanted the reply to say what was written and where");
    assert_eq!((preset.get("kind").and_then(Json::as_str), preset.get("description").and_then(Json::as_str)), (Some("delay"), Some(WORDS)), "wanted the row's kind and the description given, got {}", preset.write());
    let wanted = built(Host::Track(0), Keeping::Row(DELAY));
    assert!(matches!(&wanted, Preset::Row(Row::Effect(_), lfos) if lfos.len() == 1), "wanted the save panel's own routine to keep the delay with its one LFO lane");
    assert_eq!(read_as(&preset), wanted, "wanted the file to hold the preset the save panel builds for that row");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_whole_rack_or_a_tracks_effects_alone_are_saved_as_a_rack_preset() {
    let root = folder("save_preset_rack");
    let mut app = open(&root);
    let (_, whole, reply) = saved(&mut app, &asked("tracks/0", &format!("{root}/Whole.json"), ""));
    assert!(reply.starts_with("Writing the rack of tracks/0 as a preset to "), "wanted the reply to say the whole rack was written, got {reply}");
    assert_eq!(read_as(&whole), built(Host::Track(0), Keeping::Rack), "wanted the file to hold the preset the save panel builds for the whole rack");
    let (_, effects, reply) = saved(&mut app, &asked("tracks/0", &format!("{root}/Effects.json"), r#","effects":true"#));
    assert!(reply.starts_with("Writing the effects of tracks/0 as a preset to "), "wanted the reply to say the effects alone were written, got {reply}");
    let wanted = built(Host::Track(0), Keeping::Effects);
    assert!(matches!(&wanted, Preset::Rack(sound) if sound.rack.len() == 2 && sound.rack.iter().all(Row::is_effect) && sound.lfos.len() == 1), "wanted the save panel's own routine to keep the two effects and the delay's lane");
    assert_eq!(read_as(&effects), wanted, "wanted the file to hold the preset the save panel builds for the effects alone");
    let (_, master, _) = saved(&mut app, &asked("master", &format!("{root}/Master.json"), ""));
    assert_eq!(read_as(&master), built(Host::Master, Keeping::Rack), "wanted the master's rack saved by its own path");
    let (_, listed) = kept(&mut app);
    assert!(listed.is_empty(), "wanted every request read already");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_saved_preset_is_listed_again_in_its_browser_folder() {
    let root = folder("save_preset_listed");
    let mut app = open(&root);
    let reply = call(&mut app, "save", &asked("tracks/0/rack/2", &format!("{root}/Slap.json"), ""));
    assert!(!reply.failed, "wanted the save taken, got {}", reply.text);
    let (_, listed) = kept(&mut app);
    assert_eq!(listed, [root.as_str()], "wanted the folder the preset landed in listed again");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_preset_needs_a_description_of_twelve_words_at_most() {
    let root = folder("save_preset_words");
    let mut app = open(&root);
    let none = refused(&mut app, &format!(r#"{{"preset":"tracks/0/rack/2","path":"{root}/None.json"}}"#));
    assert!(none.contains("description is needed: twelve words at most"), "wanted a preset with no description refused, got {none}");
    let long = refused(&mut app, &format!(r#"{{"preset":"tracks/0/rack/2","path":"{root}/Long.json","description":"{TWELVE_WORDS} thirteen"}}"#));
    assert!(long.contains("twelve words at most, and this one has 13"), "wanted thirteen words refused with their count, got {long}");
    let reply = call(&mut app, "save", &format!(r#"{{"preset":"tracks/0/rack/2","path":"{root}/Twelve.json","description":"{TWELVE_WORDS}"}}"#));
    assert!(!reply.failed, "wanted twelve words taken, got {}", reply.text);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_file_already_there_is_refused_unless_overwrite_is_asked_and_it_is_a_preset() {
    let root = folder("save_preset_over");
    std::fs::write(format!("{root}/Old.json"), AN_OLD_PRESET).expect("wanted the old preset written");
    std::fs::write(format!("{root}/Song.json"), A_SONG).expect("wanted the song written");
    let mut app = open(&root);
    let there = refused(&mut app, &asked("tracks/0/rack/2", &format!("{root}/Old.json"), ""));
    assert!(there.contains("is already there"), "wanted a file already there refused, got {there}");
    let (_, preset, reply) = saved(&mut app, &asked("tracks/0/rack/2", &format!("{root}/Old.json"), r#","overwrite":true"#));
    assert!(reply.contains("in place of the preset there"), "wanted the reply to say a preset was replaced, got {reply}");
    assert_eq!(preset.get("description").and_then(Json::as_str), Some(WORDS), "wanted the new preset handed over in the old one's place, got {}", preset.write());
    let song = refused(&mut app, &asked("tracks/0/rack/2", &format!("{root}/Song.json"), r#","overwrite":true"#));
    assert!(song.contains("is not a preset; overwrite replaces a preset alone"), "wanted a song's file left alone even with overwrite, got {song}");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_preset_is_refused_at_a_path_with_nothing_to_save_and_for_a_file_that_is_no_json() {
    let root = folder("save_preset_refused");
    let mut app = open(&root);
    let path = format!("{root}/No.json");
    let missing = refused(&mut app, &asked("master/rack/3", &path, ""));
    assert!(missing.contains("No row at master/rack/3: the rack has 1 row") && !missing.contains("1 rows"), "wanted a row that is not there refused, the rack's one row counted in the singular, got {missing}");
    let empty = refused(&mut app, &asked("returns/0", &path, ""));
    assert!(empty.contains("returns/0 has no rows to save"), "wanted an empty rack refused, got {empty}");
    let row = refused(&mut app, &asked("tracks/0/rack/2", &path, r#","effects":true"#));
    assert!(row.contains("effects goes with a track's own path"), "wanted effects refused beside one row, got {row}");
    let clip = refused(&mut app, &asked("tracks/0/clips/0", &path, ""));
    assert!(clip.contains("preset is a path in the song"), "wanted a path that is no rack or row refused, got {clip}");
    let copy = refused(&mut app, &asked("tracks/0/rack/2", &path, r#","copy":true"#));
    assert!(copy.contains("pass one of them"), "wanted preset and copy together refused, got {copy}");
    let wave = refused(&mut app, &asked("tracks/0/rack/2", &format!("{root}/No.wav"), ""));
    assert!(wave.contains("is not a .json file"), "wanted a file that is no .json refused, got {wave}");
    let _ = std::fs::remove_dir_all(&root);
}
