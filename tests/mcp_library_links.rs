#![cfg(unix)]
use bp::app::App;
use bp::json::{self, Json};
use bp::project::Project;
use std::path::{Path, PathBuf};
use support::{call, wav};

const BROWSED: &str = "root";
const OWN_WAVE: &str = "root/own.wav";
const LINK: &str = "root/Library";
const SECOND_LINK: &str = "root/Mirror";
const TO_LINKED: &str = "../ext/SampleLib";
const LINKED_KICK: &str = "ext/SampleLib/kick.wav";
const LINK_BACK: &str = "ext/SampleLib/Home";
const TO_BROWSED: &str = "../../root";
const WAVE_LINK: &str = "root/alias.wav";
const TO_OWN_WAVE: &str = "own.wav";
const NESTED: &str = "root/Drums";
const NESTED_KICK: &str = "root/Drums/kick.wav";
const NESTED_TOM: &str = "root/Drums/Toms/tom.wav";
const SILENCE: [i16; 8] = [0; 8];
const FOLDERS: &str = "{}";
const DEEP: &str = r#"{"path":"root","deep":true}"#;
const KICKS: &str = r#"{"find":"kick"}"#;
const LINKED_DEEP: [&str; 3] = ["folder Library", "wave Library/kick.wav", "wave own.wav"];
const LINKED_KICKS: [&str; 1] = ["wave root/Library/kick.wav"];
const LINKED_BACK_DEEP: [&str; 4] = ["folder Library", "folder Library/Home", "wave Library/kick.wav", "wave own.wav"];
const LINKED_TWICE_DEEP: [&str; 4] = ["folder Library", "wave Library/kick.wav", "folder Mirror", "wave own.wav"];
const OWN_AND_LINKED: usize = 2;
const PLAIN_FILES: [usize; 2] = [3, 2];
const PLAIN_DEEP: [&str; 5] = ["folder Drums", "folder Drums/Toms", "wave Drums/Toms/tom.wav", "wave Drums/kick.wav", "wave own.wav"];
const PLAIN_KICKS: [&str; 2] = ["wave root/Drums/kick.wav", "wave Drums/kick.wav"];
const WAVE_AND_ITS_LINK: [&str; 2] = ["wave alias.wav", "wave own.wav"];

struct Disk {
    top: PathBuf,
}

impl Disk {
    fn new(name: &str) -> Disk {
        let top = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
        let _ = std::fs::remove_dir_all(&top);
        Disk { top }
    }

    fn within_its_folder(&self, path: &str) -> PathBuf {
        let whole = self.top.join(path);
        let folder = whole.parent().expect("wanted a folder to hold the wave or the link");
        std::fs::create_dir_all(folder).unwrap_or_else(|error| panic!("wanted {}: {error}", folder.display()));
        whole
    }

    fn wave(self, path: &str) -> Disk {
        let file = self.within_its_folder(path);
        std::fs::write(&file, wav(&SILENCE)).unwrap_or_else(|error| panic!("wanted {} written: {error}", file.display()));
        self
    }

    fn link(self, path: &str, to: &str) -> Disk {
        let link = self.within_its_folder(path);
        std::os::unix::fs::symlink(to, &link).unwrap_or_else(|error| panic!("wanted {} linked to {to}: {error}", link.display()));
        self
    }

    fn browsed(self, folders: &[&str]) -> App {
        let list: Vec<String> = folders.iter().map(|folder| Json::String(shown(&self.top.join(folder))).write()).collect();
        let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false,\"folders\":[{}]}}", Project::new().to_json().write(), list.join(","));
        let mut app = App::new();
        assert!(app.resume(&session), "wanted the session to resume");
        app.take_requests();
        app
    }
}

fn shown(path: &Path) -> String {
    path.to_str().map(String::from).expect("wanted the temporary folder's path to be text")
}

fn linked(name: &str) -> Disk {
    Disk::new(name).wave(OWN_WAVE).wave(LINKED_KICK).link(LINK, TO_LINKED)
}

fn plain(name: &str) -> App {
    Disk::new(name).wave(OWN_WAVE).wave(NESTED_KICK).wave(NESTED_TOM).browsed(&[BROWSED, NESTED])
}

fn answered(app: &mut App, args: &str) -> Json {
    let reply = call(app, "library", args);
    assert!(!reply.failed, "wanted library {args} answered, got {}", reply.text);
    json::parse(&reply.text).unwrap_or_else(|| panic!("wanted library {args} to answer JSON, got {}", reply.text))
}

fn text<'a>(entry: &'a Json, key: &str) -> &'a str {
    entry.get(key).and_then(Json::as_str).unwrap_or_else(|| panic!("wanted {key} in {}", entry.write()))
}

fn entries(app: &mut App, args: &str) -> Vec<String> {
    let listing = answered(app, args);
    let entries = listing.get("entries").and_then(Json::as_array).unwrap_or_else(|| panic!("wanted entries in {}", listing.write()));
    entries.iter().map(|entry| format!("{} {}", text(entry, "kind"), text(entry, "path"))).collect()
}

fn files(app: &mut App) -> Vec<usize> {
    let answer = answered(app, FOLDERS);
    let folders = answer.get("folders").and_then(Json::as_array).unwrap_or_else(|| panic!("wanted folders in {}", answer.write()));
    folders.iter().map(|folder| folder.get("files").and_then(Json::as_f64).unwrap_or_else(|| panic!("wanted files counted in {}", folder.write())) as usize).collect()
}

#[test]
fn a_deep_listing_goes_into_a_linked_folder() {
    let mut app = linked("links_deep").browsed(&[BROWSED]);
    let listed = entries(&mut app, DEEP);
    assert_eq!(listed, LINKED_DEEP, "wanted the linked folder listed with the kick it holds, then the browser folder's own wave, got {listed:?}");
}

#[test]
fn a_search_of_every_browser_folder_goes_into_a_linked_folder() {
    let mut app = linked("links_search").browsed(&[BROWSED]);
    let found = entries(&mut app, KICKS);
    assert_eq!(found, LINKED_KICKS, "wanted the kick found inside the linked folder, under its browser folder's name, got {found:?}");
}

#[test]
fn a_browser_folders_file_count_takes_in_a_linked_folder() {
    let mut app = linked("links_count").browsed(&[BROWSED]);
    let counted = files(&mut app);
    assert_eq!(counted, [OWN_AND_LINKED], "wanted the browser folder to count its own wave and the kick in the linked folder, got {counted:?}");
}

#[test]
fn a_deep_listing_ends_at_a_link_back_to_its_browser_folder() {
    let mut app = linked("links_back_deep").link(LINK_BACK, TO_BROWSED).browsed(&[BROWSED]);
    let listed = entries(&mut app, DEEP);
    assert_eq!(listed, LINKED_BACK_DEEP, "wanted the link back listed as a folder and not gone into, each wave listed once, got {listed:?}");
}

#[test]
fn a_file_count_ends_at_a_link_back_to_its_browser_folder() {
    let mut app = linked("links_back_count").link(LINK_BACK, TO_BROWSED).browsed(&[BROWSED]);
    let counted = files(&mut app);
    assert_eq!(counted, [OWN_AND_LINKED], "wanted the own wave and the linked kick counted once each around the link back, got {counted:?}");
}

#[test]
fn a_folder_linked_twice_is_listed_at_both_links_and_gone_into_at_the_first() {
    let mut app = linked("links_twice_deep").link(SECOND_LINK, TO_LINKED).browsed(&[BROWSED]);
    let listed = entries(&mut app, DEEP);
    assert_eq!(listed, LINKED_TWICE_DEEP, "wanted both links listed as folders and the kick once, under the first, got {listed:?}");
}

#[test]
fn a_folder_linked_twice_has_its_files_counted_once() {
    let mut app = linked("links_twice_count").link(SECOND_LINK, TO_LINKED).browsed(&[BROWSED]);
    let counted = files(&mut app);
    assert_eq!(counted, [OWN_AND_LINKED], "wanted the kick in the twice-linked folder counted once beside the own wave, got {counted:?}");
}

#[test]
fn folders_without_links_are_counted_listed_and_searched_as_before() {
    let mut app = plain("links_none");
    let counted = files(&mut app);
    assert_eq!(counted, PLAIN_FILES, "wanted a browser folder and the browser folder inside it to count three waves and two, got {counted:?}");
    let listed = entries(&mut app, DEEP);
    assert_eq!(listed, PLAIN_DEEP, "wanted every folder gone into and every wave listed, got {listed:?}");
    let found = entries(&mut app, KICKS);
    assert_eq!(found, PLAIN_KICKS, "wanted the kick found under both browser folders that hold it, got {found:?}");
}

#[test]
fn a_linked_wave_is_listed_beside_the_wave_it_points_to_as_before() {
    let mut app = Disk::new("links_wave").wave(OWN_WAVE).link(WAVE_LINK, TO_OWN_WAVE).browsed(&[BROWSED]);
    let listed = entries(&mut app, DEEP);
    assert_eq!(listed, WAVE_AND_ITS_LINK, "wanted the link to a wave and the wave it points to both listed, got {listed:?}");
}
