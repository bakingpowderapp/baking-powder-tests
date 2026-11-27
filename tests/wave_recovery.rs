use bp::app::{App, Event, Opened, Request};
use bp::browser::{Found, Item, Kind};
use bp::json::{self, Json};
use bp::keyboard::Key;
use bp::project::{Clip, Project, TICKS_PER_BEAT, Track};
use std::f64::consts::TAU;
use support::{AUDIBLE, BLOCK, HEIGHT, SAMPLE_RATE, WIDTH, amplitude, frames, painted, wav};

const SONG: &str = "/recovery/songs/song.json";
const NEXT: &str = "/recovery/songs/next.json";
const ORIGINAL: &str = "/recovery/takes/kick.wav";
const SHELVED: &str = "/recovery/library/old/kick.wav";
const BROKEN: &str = "/recovery/library/kick.wav";
const DROPPED: &str = "/recovery/desk/kick.wav";
const GONE: &str = "/recovery/gone/kick.wav";
const LIBRARY: &str = "/recovery/library";
const HEARTS: &str = "/Hearts.json";
const EMPTY: &str = "/recovery/empty";
const DESK: &str = "/recovery/desk";
const DEFAULT: &str = "/recovery/default.json";
const NAME: &str = "kick";
const SNARE: &str = "snare";
const GONE_SNARE: &str = "/recovery/gone/snare.wav";
const SHELVED_SNARE: &str = "/recovery/library/snare.wav";
const FOLDER_DROPPED_AT: (i32, i32) = (WIDTH - 20, HEIGHT - 20);
const TONES: [(&str, f64); 3] = [(ORIGINAL, 300.0), (SHELVED, 1_200.0), (DROPPED, 700.0)];
const TONE_SECONDS: f64 = 0.25;
const TONE_PEAK: f64 = 8_192.0;
const LISTENED: (usize, usize) = (2_400, 9_600);
const MAX_FRAMES: usize = 4_096;
const IDLE_KEY: Key = Key::Char('x');
const MOST_ANSWERS: usize = 12;
const FILE_CELL_CENTRE: (i32, i32) = (60, 26);
const NEW_PROJECT_ITEM_CENTRE: (i32, i32) = (60, 69);
const NEW_DEFAULT_PROJECT_ITEM_CENTRE: (i32, i32) = (60, 101);

#[derive(Clone, Copy, Debug)]
enum Order {
    ReadsFirst,
    NewestReadFirst,
    ScansFirst,
    ScansThenNewestRead,
}

const ORDERS: [Order; 4] = [Order::ReadsFirst, Order::NewestReadFirst, Order::ScansFirst, Order::ScansThenNewestRead];

impl Order {
    fn next(self, waiting: &[Request]) -> Option<usize> {
        let reads: Vec<usize> = waiting.iter().enumerate().filter(|(_, request)| matches!(request, Request::Read { .. })).map(|(at, _)| at).collect();
        let scan = waiting.iter().position(|request| matches!(request, Request::Scan { .. }));
        let (scans_first, newest) = match self {
            Order::ReadsFirst => (false, false),
            Order::NewestReadFirst => (false, true),
            Order::ScansFirst => (true, false),
            Order::ScansThenNewestRead => (true, true),
        };
        let read = if newest { reads.last().copied() } else { reads.first().copied() };
        if scans_first { scan.or(read) } else { read.or(scan) }
    }
}

#[derive(Clone, Copy, Debug)]
enum Way {
    NewProject,
    DefaultProject,
    Relaunch,
    Undo,
}

const WAYS: [Way; 4] = [Way::NewProject, Way::DefaultProject, Way::Relaunch, Way::Undo];

#[derive(Clone)]
struct Disk {
    files: Vec<&'static str>,
    shelves: Vec<(&'static str, Vec<&'static str>)>,
}

impl Disk {
    fn bytes(&self, path: &str) -> Option<Vec<u8>> {
        self.files.iter().find(|known| **known == path).map(|known| wav(&tone(hz_of(known))))
    }

    fn listing(&self, root: &str) -> Vec<Found> {
        let shelved = self.shelves.iter().filter(|(shelf, _)| *shelf == root).flat_map(|(_, paths)| paths);
        shelved.map(|path| Found { path: String::from(*path), name: String::from(path.rsplit('/').next().unwrap_or(path)), item: Item::Wave(u64::MAX) }).collect()
    }
}

struct Shell {
    app: App,
    disk: Disk,
    waiting: Vec<Request>,
    session: Option<String>,
    saved: Option<String>,
    fresh: bool,
    texts: Vec<(String, String)>,
}

impl Shell {
    fn new(disk: Disk) -> Shell {
        let mut app = App::new();
        app.draw(WIDTH, HEIGHT);
        app.take_requests();
        Shell { app, disk, waiting: Vec::new(), session: None, saved: None, fresh: false, texts: Vec::new() }
    }

    fn open(&mut self, path: &str, song: &str) {
        let opened = self.app.opened(String::from(path), Some(song), None);
        assert_eq!(opened, Opened::Project, "wanted {path} to open as a song, got {opened:?}");
    }

    fn browse(&mut self, folder: &str) {
        self.app.dropped(String::from(folder), Kind::Folder, FOLDER_DROPPED_AT.0, FOLDER_DROPPED_AT.1);
        let asked = self.asked();
        assert!(asked.is_empty(), "wanted {folder} added to the browser with nothing read or scanned, got {asked:?}");
    }

    fn asked(&mut self) -> Vec<Request> {
        let mut asked = Vec::new();
        for request in self.app.take_requests() {
            match request {
                Request::Remember { json } => self.session = Some(json),
                Request::Write { json, .. } | Request::SaveAs { json, .. } => self.saved = Some(json),
                Request::Read { path } if path.ends_with(HEARTS) => self.app.read(path, None),
                Request::Read { .. } | Request::Scan { .. } => asked.push(request),
                Request::Fresh => self.fresh = true,
                _ => {}
            }
        }
        self.waiting.extend(asked.iter().cloned());
        asked
    }

    fn click(&mut self, (x, y): (i32, i32)) {
        self.app.event(Event::Move { x, y });
        self.app.event(Event::Down { x, y, clicks: 1 });
        self.app.event(Event::Up { x, y });
        self.app.draw(WIDTH, HEIGHT);
    }

    fn stepped(&mut self, shift: bool) -> bool {
        self.app.event(Event::Key { key: Key::Char('z'), shift, command: true, repeat: false })
    }

    fn undo(&mut self) {
        assert!(self.stepped(false), "wanted an undo to take the song back a step, got nothing undone");
    }

    fn redo(&mut self) {
        assert!(self.stepped(true), "wanted a redo to take the song forward a step, got nothing redone");
    }

    fn begin_default(&mut self, song: Option<&str>) {
        self.click(FILE_CELL_CENTRE);
        self.click(NEW_DEFAULT_PROJECT_ITEM_CENTRE);
        self.asked();
        assert!(std::mem::take(&mut self.fresh), "wanted New Default Project to ask the shell for the default song, got no such request");
        self.app.begin(String::from(DEFAULT), song);
    }

    fn resume(&mut self, session: &Json) {
        assert!(self.app.resume(&session.write()), "wanted the remembered session to resume");
    }

    fn leave(&mut self, way: Way) {
        match way {
            Way::NewProject => {
                self.click(FILE_CELL_CENTRE);
                self.click(NEW_PROJECT_ITEM_CENTRE);
            }
            Way::DefaultProject => self.begin_default(None),
            Way::Relaunch => {
                let blank = Shell::new(self.disk.clone()).quit();
                self.resume(&blank);
            }
            Way::Undo => self.undo(),
        }
    }

    fn answer(&mut self, request: Request) {
        match request {
            Request::Read { path } => {
                let text = self.texts.iter().find(|(known, _)| *known == path).map(|(_, text)| text.clone().into_bytes());
                let bytes = text.or_else(|| self.disk.bytes(&path));
                self.app.read(path, bytes);
            }
            Request::Scan { path } => {
                let found = self.disk.listing(&path);
                self.app.scanned(path, found);
            }
            _ => {}
        }
    }

    fn answer_waiting(&mut self) {
        for request in std::mem::take(&mut self.waiting) {
            self.answer(request);
        }
    }

    fn settle_reads(&mut self) {
        self.drain(|waiting| waiting.iter().position(|request| matches!(request, Request::Read { .. })));
    }

    fn settle(&mut self, order: Order) {
        self.drain(|waiting| order.next(waiting));
    }

    fn drain(&mut self, next: impl Fn(&[Request]) -> Option<usize>) {
        for answered in 0.. {
            self.asked();
            let Some(at) = next(&self.waiting) else {
                return;
            };
            assert!(answered < MOST_ANSWERS, "wanted the app to stop asking within {MOST_ANSWERS} answers, but after them it still waits on {:?}", self.waiting);
            let request = self.waiting.remove(at);
            self.answer(request);
        }
    }

    fn quit(&mut self) -> Json {
        self.app.event(Event::Quit);
        self.asked();
        json::parse(self.session.as_deref().expect("wanted the session remembered")).expect("wanted the remembered session to parse")
    }

    fn resumed(&self, session: &Json) -> Shell {
        let mut shell = Shell::new(self.disk.clone());
        shell.texts = self.texts.clone();
        shell.resume(session);
        shell
    }

    fn relaunched(&self, session: &Json) -> Shell {
        let mut shell = self.resumed(session);
        shell.settle(Order::ReadsFirst);
        shell
    }

    fn saved(&mut self) -> Json {
        self.app.event(Event::Key { key: Key::Char('s'), shift: false, command: true, repeat: false });
        self.asked();
        json::parse(self.saved.as_deref().expect("wanted the save handed to the shell")).expect("wanted the saved song to parse")
    }

    fn listened(&mut self) -> Vec<f32> {
        let mut renderer = self.app.audio_started(SAMPLE_RATE, MAX_FRAMES, BLOCK as u32);
        self.app.event(Event::Key { key: Key::Space, shift: false, command: false, repeat: false });
        let (mut left, mut right) = (vec![0.0; LISTENED.1], vec![0.0; LISTENED.1]);
        for (l, r) in left.chunks_mut(BLOCK).zip(right.chunks_mut(BLOCK)) {
            renderer.render(l, r);
        }
        left.split_off(LISTENED.0)
    }
}

fn hz_of(path: &str) -> f64 {
    TONES.iter().find(|(known, _)| *known == path).map(|(_, hz)| *hz).unwrap_or_else(|| panic!("wanted a tone for {path}"))
}

fn tone(hz: f64) -> Vec<i16> {
    (0..frames(TONE_SECONDS)).map(|frame| (TONE_PEAK * (TAU * hz * frame as f64 / SAMPLE_RATE).sin()).round() as i16).collect()
}

fn song(source: &str) -> String {
    song_of(vec![linked(NAME, source)])
}

fn song_of(waves: Vec<Json>) -> String {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0] = Track::audio();
    project.tracks[0].clips.push(Clip::audio(0, TICKS_PER_BEAT, String::from(NAME), String::from(NAME)));
    let mut document = project.to_json();
    if let Json::Object(fields) = &mut document {
        fields.push((String::from("waves"), Json::Array(waves)));
    }
    document.write()
}

fn listing(song: &str, folders: &[&str]) -> String {
    let mut document = json::parse(song).expect("wanted the song to parse");
    if let Json::Object(fields) = &mut document {
        fields.push((String::from("folders"), Json::Array(folders.iter().map(|folder| Json::String(String::from(*folder))).collect())));
    }
    document.write()
}

fn named(name: &str) -> Json {
    Json::Object(vec![(String::from("name"), Json::String(String::from(name)))])
}

fn linked(name: &str, source: &str) -> Json {
    Json::Object(vec![(String::from("name"), Json::String(String::from(name))), (String::from("sourcePath"), Json::String(String::from(source)))])
}

fn embedded(saved: &Json) -> String {
    let mut song = saved.clone();
    if let Json::Object(fields) = &mut song
        && let Some((_, Json::Array(waves))) = fields.iter_mut().find(|(key, _)| key == "waves")
    {
        for wave in waves {
            if let Json::Object(entry) = wave {
                entry.retain(|(key, _)| key != "sourcePath");
            }
        }
    }
    song.write()
}

fn relinked(saved: &Json, source: &str) -> String {
    let mut song = saved.clone();
    if let Json::Object(fields) = &mut song
        && let Some((_, Json::Array(waves))) = fields.iter_mut().find(|(key, _)| key == "waves")
    {
        for wave in waves {
            if let Json::Object(entry) = wave {
                for (_, value) in entry.iter_mut().filter(|(key, _)| key == "sourcePath") {
                    *value = Json::String(String::from(source));
                }
            }
        }
    }
    song.write()
}

fn without_clips(saved: &Json) -> String {
    let mut song = json::parse(&embedded(saved)).expect("wanted the saved song to parse once its links were taken out");
    if let Json::Object(fields) = &mut song
        && let Some((_, Json::Array(tracks))) = fields.iter_mut().find(|(key, _)| key == "tracks")
    {
        for track in tracks {
            if let Json::Object(entry) = track {
                entry.retain(|(key, _)| key != "clips");
            }
        }
    }
    song.write()
}

fn read(path: &str) -> Request {
    Request::Read { path: String::from(path) }
}

fn scan(path: &str) -> Request {
    Request::Scan { path: String::from(path) }
}

fn origin(session: &Json) -> Option<String> {
    let origins = session.get("origins").and_then(Json::as_array)?;
    let kick = origins.iter().find(|item| item.get("name").and_then(Json::as_str) == Some(NAME))?;
    kick.get("from").and_then(Json::as_str).map(String::from)
}

fn assert_heard(shell: &mut Shell, wanted: &str, when: &str) {
    let window = shell.listened();
    let heard: Vec<(&str, f64)> = TONES.iter().map(|(path, hz)| (*path, amplitude(&window, *hz))).collect();
    let sounding: Vec<&str> = heard.iter().filter(|(_, level)| *level > f64::from(AUDIBLE)).map(|(path, _)| *path).collect();
    assert_eq!(sounding, vec![wanted], "wanted {wanted}'s tone alone {when}, heard {heard:?}");
}

fn assert_relaunch_plays(shell: &mut Shell, wanted: &str, when: &str) {
    let session = shell.quit();
    let origin = origin(&session);
    assert_eq!(origin.as_deref(), Some(wanted), "wanted the session remembered on quitting to name {wanted}, {when}, got {origin:?}");
    assert_heard(&mut shell.relaunched(&session), wanted, &format!("after a relaunch, {when}"));
}

fn notice_shown(app: &mut App) -> bool {
    let before = painted(app);
    app.event(Event::Key { key: IDLE_KEY, shift: false, command: true, repeat: false });
    before != painted(app)
}

fn asked_in_turn(shell: &mut Shell, turns: &[(Request, &str)]) {
    for (wanted, after) in turns {
        let asked = shell.asked();
        assert_eq!(asked, vec![wanted.clone()], "wanted {wanted:?} asked alone after {after}, got {asked:?}");
        shell.answer_waiting();
    }
    let asked = shell.asked();
    let last = turns.last().map(|(wanted, _)| format!("{wanted:?}")).unwrap_or_default();
    assert!(asked.is_empty(), "wanted nothing more asked once {last} was answered, got {asked:?}");
}

fn original_beside_another() -> Disk {
    Disk { files: vec![ORIGINAL, SHELVED], shelves: vec![(LIBRARY, vec![SHELVED])] }
}

fn another_without_the_original() -> Disk {
    Disk { files: vec![SHELVED], shelves: vec![(LIBRARY, vec![SHELVED])] }
}

fn no_kick_anywhere() -> Disk {
    Disk { files: Vec::new(), shelves: vec![(EMPTY, Vec::new())] }
}

fn mid_search(disk: Disk, folder: &str) -> Shell {
    let mut shell = Shell::new(disk);
    shell.browse(folder);
    shell.open(SONG, &song(GONE));
    shell.settle_reads();
    assert_eq!(shell.waiting, vec![scan(folder)], "wanted the song's scan held once its saved path failed to read, got {:?}", shell.waiting);
    shell
}

#[test]
fn a_linked_wave_that_reads_is_never_searched_for() {
    let mut shell = Shell::new(original_beside_another());
    shell.browse(LIBRARY);
    shell.open(SONG, &song(ORIGINAL));
    asked_in_turn(&mut shell, &[(read(ORIGINAL), "the open, while the original's read is in flight")]);
}

#[test]
fn the_original_plays_after_a_relaunch_whatever_order_the_shell_answers_in() {
    for order in ORDERS {
        let mut shell = Shell::new(original_beside_another());
        shell.browse(LIBRARY);
        shell.open(SONG, &song(ORIGINAL));
        shell.settle(order);
        let when = format!("answered {order:?}");
        assert_heard(&mut shell, ORIGINAL, &format!("from the opened song, {when}"));
        assert_relaunch_plays(&mut shell, ORIGINAL, &when);
    }
}

#[test]
fn a_search_answered_after_the_wave_loaded_another_way_changes_nothing() {
    let mut shell = Shell::new(Disk { files: vec![SHELVED, DROPPED], shelves: vec![(LIBRARY, vec![SHELVED])] });
    shell.browse(LIBRARY);
    shell.open(SONG, &song(GONE));
    shell.settle_reads();
    assert_eq!(shell.waiting, vec![scan(LIBRARY)], "wanted a scan of the browser folder waiting once the saved path failed to read, got {:?}", shell.waiting);
    shell.app.dropped(String::from(DROPPED), Kind::Wave, WIDTH - 20, HEIGHT - 20);
    shell.settle_reads();
    shell.answer_waiting();
    let asked = shell.asked();
    assert!(asked.is_empty(), "wanted the late scan to ask nothing for a wave that had loaded, got {asked:?}");
    assert_heard(&mut shell, DROPPED, "from the song");
    assert_relaunch_plays(&mut shell, DROPPED, "the scan answered after the drop");
}

#[test]
fn a_scan_held_from_the_last_song_takes_nothing_from_the_next() {
    for order in ORDERS {
        let mut shell = Shell::new(original_beside_another());
        shell.browse(LIBRARY);
        shell.open(SONG, &song(GONE));
        shell.settle_reads();
        assert_eq!(shell.waiting, vec![scan(LIBRARY)], "wanted the first song's scan held, got {:?}", shell.waiting);
        shell.open(NEXT, &song(ORIGINAL));
        shell.settle(order);
        let when = format!("answered {order:?}");
        assert_heard(&mut shell, ORIGINAL, &format!("from the next song, {when}"));
        assert_relaunch_plays(&mut shell, ORIGINAL, &when);
    }
}

#[test]
fn a_scan_held_from_the_last_song_shows_the_next_no_missing_notice() {
    for order in ORDERS {
        let mut shell = Shell::new(Disk { files: vec![ORIGINAL], shelves: vec![(EMPTY, Vec::new())] });
        shell.browse(EMPTY);
        shell.open(SONG, &song(GONE));
        shell.settle_reads();
        assert_eq!(shell.waiting, vec![scan(EMPTY)], "wanted the first song's scan held, got {:?}", shell.waiting);
        shell.open(NEXT, &song(ORIGINAL));
        shell.settle(order);
        assert!(!notice_shown(&mut shell.app), "wanted no missing-wave notice over the next song, whose wave read, answered {order:?}, but the next key press took one off the screen");
        assert_heard(&mut shell, ORIGINAL, &format!("from the next song, answered {order:?}"));
    }
}

#[test]
fn a_dead_link_under_a_browser_folder_is_found_by_a_scan_asked_once_its_read_fails() {
    let mut shell = Shell::new(Disk { files: vec![SHELVED], shelves: vec![(LIBRARY, vec![SHELVED])] });
    shell.browse(LIBRARY);
    shell.open(SONG, &song(GONE));
    asked_in_turn(&mut shell, &[(read(GONE), "the open"), (scan(LIBRARY), "the saved path failed to read"), (read(SHELVED), "the scan listed the name")]);
    assert_heard(&mut shell, SHELVED, "from the song");
    assert_relaunch_plays(&mut shell, SHELVED, "found by the scan");
}

#[test]
fn a_wave_found_nowhere_ends_missing_with_its_path_kept_and_a_notice_shown() {
    let mut shell = Shell::new(Disk { files: Vec::new(), shelves: vec![(EMPTY, Vec::new())] });
    shell.browse(EMPTY);
    shell.open(SONG, &song(GONE));
    asked_in_turn(&mut shell, &[(read(GONE), "the open"), (scan(EMPTY), "the saved path failed to read")]);
    assert!(notice_shown(&mut shell.app), "wanted a notice that the wave is missing once the scan came back empty, but the next key press left the screen as it was");
    let file = shell.saved();
    let entries = file.get("waves").and_then(Json::as_array).unwrap_or(&[]);
    let kept: Vec<(Option<&str>, Option<&str>, bool)> = entries.iter().map(|entry| (entry.get("name").and_then(Json::as_str), entry.get("sourcePath").and_then(Json::as_str), entry.get("pcm16Base64").is_some())).collect();
    assert_eq!(kept, vec![(Some(NAME), Some(GONE), false)], "wanted the missing wave saved as a link to its path, no samples, got {kept:?}");
    let session = shell.quit();
    let mut relaunched = shell.resumed(&session);
    let asked = relaunched.asked();
    assert_eq!(asked, vec![read(GONE)], "wanted a relaunch to look for the wave at its kept path, got {asked:?}");
}

#[test]
fn a_path_already_tried_is_not_read_again_when_a_scan_lists_it() {
    let mut shell = Shell::new(Disk { files: vec![SHELVED], shelves: vec![(LIBRARY, vec![BROKEN, SHELVED])] });
    shell.browse(LIBRARY);
    shell.open(SONG, &song(BROKEN));
    asked_in_turn(&mut shell, &[(read(BROKEN), "the open"), (scan(LIBRARY), "the saved path failed to read"), (read(SHELVED), "the scan listed the failed path first")]);
    assert_heard(&mut shell, SHELVED, "from the song");
}

#[test]
fn a_scan_held_from_the_last_song_asks_nothing_whichever_way_a_blank_song_replaced_it() {
    for way in WAYS {
        let mut shell = mid_search(another_without_the_original(), LIBRARY);
        shell.leave(way);
        shell.answer_waiting();
        let asked = shell.asked();
        assert!(asked.is_empty(), "wanted nothing asked once a blank song had replaced the last by {way:?} and the last song's scan listed its wave, got {asked:?}");
    }
}

#[test]
fn a_scan_held_from_the_last_song_shows_no_missing_notice_whichever_way_a_blank_song_replaced_it() {
    for way in WAYS {
        let mut shell = mid_search(no_kick_anywhere(), EMPTY);
        shell.leave(way);
        shell.answer_waiting();
        assert!(!notice_shown(&mut shell.app), "wanted no missing-wave notice over the blank song that replaced the last by {way:?}, having no wave to miss, but the next key press took one off the screen");
    }
}

#[test]
fn a_path_that_failed_is_read_again_when_its_song_is_begun_anew_as_the_default() {
    let mut shell = Shell::new(Disk { files: Vec::new(), shelves: Vec::new() });
    let linked = song(ORIGINAL);
    shell.open(SONG, &linked);
    asked_in_turn(&mut shell, &[(read(ORIGINAL), "the open, its file not there to read")]);
    shell.disk.files.push(ORIGINAL);
    shell.begin_default(Some(&linked));
    asked_in_turn(&mut shell, &[(read(ORIGINAL), "the same song was begun anew as the default, its file back")]);
    assert_heard(&mut shell, ORIGINAL, "from the default song");
}

#[test]
fn a_wave_being_searched_for_is_still_found_when_its_song_is_undone_and_redone() {
    for order in ORDERS {
        let mut shell = mid_search(another_without_the_original(), LIBRARY);
        shell.undo();
        shell.redo();
        shell.settle(order);
        let when = format!("its song undone and redone mid-search, answered {order:?}");
        assert_heard(&mut shell, SHELVED, &format!("from the song, {when}"));
        assert_relaunch_plays(&mut shell, SHELVED, &when);
    }
}

#[test]
fn a_wave_being_searched_for_ends_missing_with_a_notice_when_its_song_is_undone_and_redone() {
    for order in ORDERS {
        let mut shell = mid_search(no_kick_anywhere(), EMPTY);
        shell.undo();
        shell.redo();
        shell.settle(order);
        assert!(notice_shown(&mut shell.app), "wanted a notice that the wave is missing once its song was undone and redone mid-search and the scan came back empty, answered {order:?}, but the next key press left the screen as it was");
    }
}

#[test]
fn a_loaded_wave_is_not_asked_for_again_when_its_song_is_undone_and_redone() {
    let mut shell = Shell::new(original_beside_another());
    shell.browse(LIBRARY);
    shell.open(SONG, &song(ORIGINAL));
    shell.settle(Order::ReadsFirst);
    shell.undo();
    shell.redo();
    let asked = shell.asked();
    assert!(asked.is_empty(), "wanted nothing asked for a song with no wave missing once it was undone and redone, got {asked:?}");
    assert_heard(&mut shell, ORIGINAL, "from the song undone and redone");
}

#[test]
fn a_wave_known_missing_is_not_looked_for_again_when_its_song_is_undone_and_redone() {
    let mut shell = Shell::new(no_kick_anywhere());
    shell.browse(EMPTY);
    shell.open(SONG, &song(GONE));
    shell.settle(Order::ReadsFirst);
    assert!(notice_shown(&mut shell.app), "wanted a notice that the wave is missing once the scan came back empty, but the next key press left the screen as it was");
    shell.undo();
    shell.redo();
    let asked = shell.asked();
    assert!(asked.is_empty(), "wanted the wave, already known missing, not looked for again once its song was undone and redone, got {asked:?}");
}

#[test]
fn a_wave_saved_by_name_alone_and_found_nowhere_is_told_once_when_its_song_is_undone_and_redone_mid_search() {
    let mut shell = Shell::new(no_kick_anywhere());
    shell.browse(EMPTY);
    shell.open(SONG, &song_of(vec![named(NAME)]));
    let asked = shell.asked();
    assert_eq!(asked, vec![scan(EMPTY)], "wanted the browser's folder scanned for the wave saved by name alone, got {asked:?}");
    shell.undo();
    shell.redo();
    shell.settle(Order::ReadsFirst);
    assert!(notice_shown(&mut shell.app), "wanted a notice that the wave is missing once its song was undone and redone mid-search and the scans came back empty, but the next key press left the screen as it was");
    shell.undo();
    shell.redo();
    let asked = shell.asked();
    assert!(asked.is_empty(), "wanted the wave, now known missing, not looked for again when its song was undone and redone once more, got {asked:?}");
}

#[test]
fn a_wave_embedded_in_its_song_still_loads_when_the_song_is_undone_and_redone_while_a_relaunch_reads_it() {
    let mut first = Shell::new(original_beside_another());
    first.open(SONG, &song(ORIGINAL));
    first.settle(Order::ReadsFirst);
    let embedded = embedded(&first.saved());
    let mut shell = Shell::new(Disk { files: Vec::new(), shelves: Vec::new() });
    shell.texts.push((String::from(SONG), embedded.clone()));
    shell.open(SONG, &embedded);
    shell.settle(Order::ReadsFirst);
    let session = shell.quit();
    let mut relaunched = shell.resumed(&session);
    let asked = relaunched.asked();
    assert_eq!(asked, vec![read(SONG)], "wanted a relaunch to read the song's own file for the wave embedded in it, got {asked:?}");
    relaunched.undo();
    relaunched.redo();
    relaunched.settle(Order::ReadsFirst);
    assert!(!notice_shown(&mut relaunched.app), "wanted no missing notice for a wave whose file was still being read when its song was undone and redone, but the next key press took one off the screen");
    assert_heard(&mut relaunched, ORIGINAL, "from the relaunched song, undone and redone while its file was being read");
}

#[test]
fn a_wave_saved_by_name_alone_is_found_under_a_browser_folder() {
    let mut shell = Shell::new(another_without_the_original());
    shell.browse(LIBRARY);
    shell.open(SONG, &song_of(vec![named(NAME)]));
    asked_in_turn(&mut shell, &[(scan(LIBRARY), "the open, the browser holding the folder its wave lies under"), (read(SHELVED), "the scan listed the name")]);
    assert_heard(&mut shell, SHELVED, "from the song");
    assert_relaunch_plays(&mut shell, SHELVED, "found under the browser's folder");
}

#[test]
fn a_wave_saved_by_name_alone_is_not_looked_for_under_a_folder_only_its_song_lists() {
    let mut shell = Shell::new(another_without_the_original());
    shell.open(SONG, &listing(&song_of(vec![named(NAME)]), &[LIBRARY]));
    let asked = shell.asked();
    assert!(asked.is_empty(), "wanted nothing read or scanned for a wave whose folder the song lists and the browser lacks, got {asked:?}");
    assert!(notice_shown(&mut shell.app), "wanted a notice that the wave is missing, no browser folder holding it, but the next key press left the screen as it was");
}

#[test]
fn a_linked_wave_whose_read_fails_asks_no_second_scan_of_a_folder_already_waited_on() {
    let mut shell = Shell::new(Disk { files: vec![SHELVED], shelves: vec![(LIBRARY, vec![SHELVED, SHELVED_SNARE])] });
    shell.browse(LIBRARY);
    shell.open(SONG, &song_of(vec![named(NAME), linked(SNARE, GONE_SNARE)]));
    let asked = shell.asked();
    assert_eq!(asked, vec![scan(LIBRARY), read(GONE_SNARE)], "wanted the browser's folder scanned for the wave saved by name alone and the linked wave read from its path, got {asked:?}");
    let failed = shell.waiting.remove(1);
    shell.answer(failed);
    let asked = shell.asked();
    assert!(asked.is_empty(), "wanted no second scan of the folder already waited on once the linked wave's read failed, got {asked:?}");
    shell.answer_waiting();
    let asked = shell.asked();
    assert_eq!(asked, vec![read(SHELVED), read(SHELVED_SNARE)], "wanted both waves read from where the one scan listed them, got {asked:?}");
}

#[test]
fn a_wave_two_folders_hold_is_taken_from_the_later_folder_whichever_scan_answers_first() {
    for later_first in [false, true] {
        let mut shell = Shell::new(Disk { files: vec![SHELVED, DROPPED], shelves: vec![(LIBRARY, vec![SHELVED]), (DESK, vec![DROPPED])] });
        shell.browse(LIBRARY);
        shell.browse(DESK);
        shell.open(SONG, &song(GONE));
        shell.settle_reads();
        assert_eq!(shell.waiting, vec![scan(LIBRARY), scan(DESK)], "wanted both of the browser's folders scanned once the song's saved path failed to read, got {:?}", shell.waiting);
        if later_first {
            shell.waiting.reverse();
        }
        let when = format!("the later folder's scan answered {}", if later_first { "first" } else { "last" });
        shell.answer_waiting();
        shell.settle(Order::ReadsFirst);
        assert_heard(&mut shell, DROPPED, &format!("from the song, {when}"));
        assert_relaunch_plays(&mut shell, DROPPED, &when);
    }
}

#[test]
fn a_wave_only_the_earlier_folder_holds_is_found_whichever_scan_answers_first() {
    for later_first in [false, true] {
        let mut shell = Shell::new(Disk { files: vec![SHELVED], shelves: vec![(LIBRARY, vec![SHELVED]), (EMPTY, Vec::new())] });
        shell.browse(LIBRARY);
        shell.browse(EMPTY);
        shell.open(SONG, &song(GONE));
        shell.settle_reads();
        assert_eq!(shell.waiting, vec![scan(LIBRARY), scan(EMPTY)], "wanted both of the browser's folders scanned once the song's saved path failed to read, got {:?}", shell.waiting);
        if later_first {
            shell.waiting.reverse();
        }
        let when = format!("the empty later folder's scan answered {}", if later_first { "first" } else { "last" });
        shell.answer_waiting();
        shell.settle(Order::ReadsFirst);
        assert!(!notice_shown(&mut shell.app), "wanted no missing notice for a wave the earlier folder holds, {when}, but the next key press took one off the screen");
        assert_heard(&mut shell, SHELVED, &format!("from the song, {when}"));
    }
}

#[test]
fn a_wave_no_folder_holds_is_told_missing_only_once_every_scan_has_answered() {
    for later_first in [false, true] {
        let mut shell = Shell::new(Disk { files: Vec::new(), shelves: vec![(LIBRARY, Vec::new()), (EMPTY, Vec::new())] });
        shell.browse(LIBRARY);
        shell.browse(EMPTY);
        shell.open(SONG, &song(GONE));
        shell.settle_reads();
        if later_first {
            shell.waiting.reverse();
        }
        let when = format!("the later folder's scan answered {}", if later_first { "first" } else { "last" });
        let first = shell.waiting.remove(0);
        shell.answer(first);
        assert!(!notice_shown(&mut shell.app), "wanted no missing notice while a folder's scan was still out, {when}, but the next key press took one off the screen");
        shell.answer_waiting();
        let asked = shell.asked();
        assert!(asked.is_empty(), "wanted nothing asked once both scans came back without the wave, {when}, got {asked:?}");
        assert!(notice_shown(&mut shell.app), "wanted the missing notice once both scans came back without the wave, {when}, but no key press took one off the screen");
    }
}

#[test]
fn a_wave_embedded_in_its_song_keeps_its_own_audio_when_the_song_is_undone_and_redone_while_a_relaunch_reads_it() {
    let mut first = Shell::new(Disk { files: vec![SHELVED], shelves: Vec::new() });
    first.open(SONG, &song(SHELVED));
    first.settle(Order::ReadsFirst);
    let song = relinked(&first.saved(), ORIGINAL);
    let mut shell = Shell::new(Disk { files: vec![ORIGINAL], shelves: Vec::new() });
    shell.texts.push((String::from(SONG), song.clone()));
    shell.open(SONG, &song);
    shell.settle(Order::ReadsFirst);
    assert_heard(&mut shell, SHELVED, "from the opened song, whose own audio is not what its source file now holds");
    let session = shell.quit();
    let mut relaunched = shell.resumed(&session);
    let asked = relaunched.asked();
    assert_eq!(asked, vec![read(SONG)], "wanted a relaunch to read the song's own file for the wave embedded in it, got {asked:?}");
    relaunched.undo();
    relaunched.redo();
    relaunched.settle(Order::ReadsFirst);
    assert_heard(&mut relaunched, SHELVED, "from the relaunched song, undone and redone while its file was being read");
    let origin = origin(&relaunched.quit());
    assert_eq!(origin.as_deref(), Some(SONG), "wanted the wave still remembered as coming from its song's own file, got {origin:?}");
}

#[test]
fn a_wave_its_song_does_not_list_is_looked_for_again_when_the_song_is_opened_again() {
    let unlisted = song_of(Vec::new());
    let mut shell = Shell::new(another_without_the_original());
    shell.open(SONG, &unlisted);
    shell.settle(Order::ReadsFirst);
    assert!(notice_shown(&mut shell.app), "wanted a notice that the wave is missing on the first open, no browser folder holding it, but the next key press left the screen as it was");
    shell.browse(LIBRARY);
    shell.open(SONG, &unlisted);
    asked_in_turn(&mut shell, &[(scan(LIBRARY), "the second open, the wave's folder now in the browser"), (read(SHELVED), "the scan listed the name")]);
    assert_heard(&mut shell, SHELVED, "from the song opened again");
}

#[test]
fn a_wave_only_a_saved_version_uses_keeps_its_audio_through_a_relaunch_and_a_save() {
    let mut first = Shell::new(original_beside_another());
    first.open(SONG, &song(ORIGINAL));
    first.settle(Order::ReadsFirst);
    let emptied = without_clips(&first.saved());
    let mut shell = Shell::new(Disk { files: Vec::new(), shelves: Vec::new() });
    shell.texts.push((String::from(SONG), emptied.clone()));
    shell.open(SONG, &emptied);
    shell.settle(Order::ReadsFirst);
    let session = shell.quit();
    let mut relaunched = shell.relaunched(&session);
    let resaved = relaunched.saved();
    let versions = resaved.get("versions").and_then(Json::as_array).map_or(0, <[Json]>::len);
    assert!(versions > 0, "wanted the song saved after the relaunch to keep its earlier version, got {versions} versions");
    let waves: Vec<&str> = resaved.get("waves").and_then(Json::as_array).unwrap_or(&[]).iter().filter(|wave| wave.get("pcm16Base64").is_some()).filter_map(|wave| wave.get("name").and_then(Json::as_str)).collect();
    assert_eq!(waves, vec![NAME], "wanted the wave only the earlier version uses still saved with its audio after a relaunch, got the waves {waves:?}");
}

#[test]
fn a_dropped_file_named_as_a_wave_that_is_not_one_is_told_and_lands_nothing_later() {
    let mut shell = Shell::new(Disk { files: vec![DROPPED], shelves: Vec::new() });
    shell.texts.push((String::from(DROPPED), String::from("not a wave")));
    shell.app.dropped(String::from(DROPPED), Kind::Wave, FOLDER_DROPPED_AT.0, FOLDER_DROPPED_AT.1);
    shell.settle_reads();
    assert!(notice_shown(&mut shell.app), "wanted a notice that the dropped file could not be read as a wave, but the next key press left the screen as it was");
    shell.texts.clear();
    shell.app.dropped(String::from(DROPPED), Kind::Wave, FOLDER_DROPPED_AT.0, FOLDER_DROPPED_AT.1);
    shell.settle_reads();
    let session = shell.quit();
    let project = Project::from_json(session.get("project").expect("wanted the remembered session to hold the project")).expect("wanted the remembered project to read");
    let clips: usize = project.tracks.iter().map(|track| track.clips.len()).sum();
    assert_eq!(clips, 1, "wanted one clip, from the second drop alone, once the file read as a wave, got {clips}");
}

#[test]
fn a_default_project_whose_clip_names_a_wave_it_does_not_list_is_told() {
    let mut shell = Shell::new(no_kick_anywhere());
    shell.begin_default(Some(&song_of(Vec::new())));
    shell.settle(Order::ReadsFirst);
    assert!(notice_shown(&mut shell.app), "wanted a notice that the default project's wave is missing, as an opened song gets, but the next key press left the screen as it was");
}

#[test]
fn a_wave_the_song_holds_keeps_its_audio_when_its_file_is_dropped_again_and_no_longer_reads() {
    let mut first = Shell::new(original_beside_another());
    first.open(SONG, &song(ORIGINAL));
    first.settle(Order::ReadsFirst);
    let held = first.saved().write();
    for unreadable in ["not a wave", "RIFF a wave cut short"] {
        let mut shell = Shell::new(Disk { files: Vec::new(), shelves: Vec::new() });
        shell.texts.push((String::from(SONG), held.clone()));
        shell.texts.push((String::from(ORIGINAL), String::from(unreadable)));
        shell.open(SONG, &held);
        shell.settle(Order::ReadsFirst);
        shell.app.dropped(String::from(ORIGINAL), Kind::Wave, FOLDER_DROPPED_AT.0, FOLDER_DROPPED_AT.1);
        let asked = shell.asked();
        assert!(!asked.contains(&Request::Read { path: String::from(ORIGINAL) }), "wanted the song's own copy placed again with its file, now holding {unreadable:?}, left unread, got {asked:?}");
        shell.settle_reads();
        let resaved = shell.saved();
        let waves: Vec<&str> = resaved.get("waves").and_then(Json::as_array).unwrap_or(&[]).iter().filter(|wave| wave.get("pcm16Base64").is_some()).filter_map(|wave| wave.get("name").and_then(Json::as_str)).collect();
        assert_eq!(waves, vec![NAME], "wanted the song's wave still saved with its audio after its file, now holding {unreadable:?}, was dropped again, got the waves {waves:?}");
        let clips: usize = Project::from_json(&resaved).expect("wanted the saved song to read").tracks.iter().map(|track| track.clips.len()).sum();
        assert_eq!(clips, 2, "wanted the drop to place the song's own copy beside its first clip, got {clips} clips");
        assert_heard(&mut shell, ORIGINAL, "from the song, after its wave's file was dropped again");
    }
}
