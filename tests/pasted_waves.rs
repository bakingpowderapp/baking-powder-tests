use bp::app::{App, Event, Opened, Request};
use bp::browser::{Found, Item};
use bp::json::{self, Json};
use bp::keyboard::Key;
use bp::project::{Clip, Project, TICKS_PER_BEAT, Track};
use bp::wav::Wave;
use std::f64::consts::TAU;
use support::{AUDIBLE, BLOCK, HEIGHT, SAMPLE_RATE, WIDTH, amplitude, frames, wav};

const NAME: &str = "kick";
const PASTED: &str = "kick 2";
const SPARE: &str = "kick 3";
const LIBRARY: &str = "/pasted/library";
const HEARTS: &str = "/Hearts.json";
const KICKS: [(&str, f64); 3] = [("/pasted/takes/low/kick.wav", 310.0), ("/pasted/takes/mid/kick.wav", 730.0), ("/pasted/library/kick.wav", 1_170.0)];
const LOW: usize = 0;
const HIGH: usize = 2;
const SONGS: [&str; 3] = ["/pasted/songs/low.json", "/pasted/songs/mid.json", "/pasted/songs/high.json"];
const EMPTY_SONG: &str = "/pasted/songs/empty.json";
const MARKS: [&str; 3] = ["Low", "Mid", "High"];
const EMPTY_MARK: &str = "Empty";
const TONE_SECONDS: f64 = 0.25;
const TONE_PEAK: f64 = 8_192.0;
const LISTENED: (usize, usize) = (2_400, 9_600);
const MAX_FRAMES: usize = 4_096;
const MOST_ANSWERS: usize = 64;
const FILE_CELL_CENTRE: (i32, i32) = (60, 26);
const NEW_PROJECT_ITEM_CENTRE: (i32, i32) = (60, 69);
const WALKS: u64 = 96;
const WALK_STEPS: usize = 40;
const WALK_SEED: u64 = 0x9E37_79B9_7F4A_7C15;
const MOVES: u64 = 12;

struct Shell {
    app: App,
    files: Vec<(String, Vec<u8>)>,
    shelves: Vec<(&'static str, Vec<&'static str>)>,
    waiting: Vec<Request>,
    session: Option<String>,
    saved: Option<String>,
    seed: u64,
}

impl Shell {
    fn new(shelves: Vec<(&'static str, Vec<&'static str>)>) -> Shell {
        let mut files: Vec<(String, Vec<u8>)> = KICKS.iter().map(|(path, hz)| (String::from(*path), wav(&tone(*hz)))).collect();
        files.extend((0..SONGS.len()).map(|kick| (String::from(SONGS[kick]), song(kick, linked(kick)).into_bytes())));
        files.push((String::from(EMPTY_SONG), empty().into_bytes()));
        Shell { app: launched(), files, shelves, waiting: Vec::new(), session: None, saved: None, seed: 1 }
    }

    fn bytes(&self, path: &str) -> Option<Vec<u8>> {
        self.files.iter().find(|(known, _)| known == path).map(|(_, bytes)| bytes.clone())
    }

    fn open(&mut self, path: &str, text: &str) {
        let opened = self.app.opened(String::from(path), Some(text), None);
        assert_eq!(opened, Opened::Project, "wanted {path} to open as a song, got {opened:?}");
    }

    fn open_file(&mut self, path: &str) {
        let text = String::from_utf8(self.bytes(path).unwrap_or_else(|| panic!("wanted {path} on the disk"))).expect("wanted the song's file as text");
        self.open(path, &text);
    }

    fn browse(&mut self, folder: &str) {
        self.app.add_folder(String::from(folder));
        let asked = self.gather();
        assert!(asked.is_empty(), "wanted {folder} added to the browser with nothing read or scanned, got {asked:?}");
    }

    fn gather(&mut self) -> Vec<Request> {
        let mut asked = Vec::new();
        for request in self.app.take_requests() {
            match request {
                Request::Remember { json } => self.session = Some(json),
                Request::SaveAs { json, .. } => self.saved = Some(json),
                Request::Write { path, json, save } => {
                    self.saved = Some(json.clone());
                    self.files.retain(|(known, _)| *known != path);
                    self.files.push((path.clone(), json.into_bytes()));
                    self.app.saved(path, save);
                }
                Request::Read { path } if path.ends_with(HEARTS) => self.app.read(path, None),
                Request::Read { .. } | Request::Scan { .. } => asked.push(request),
                _ => {}
            }
        }
        self.waiting.extend(asked.iter().cloned());
        asked
    }

    fn key(&mut self, key: char, shift: bool) {
        self.app.event(Event::Key { key: Key::Char(key), shift, command: true, repeat: false });
    }

    fn copy(&mut self) {
        self.key('c', false);
    }

    fn paste(&mut self) {
        self.key('v', false);
    }

    fn undo(&mut self) {
        self.key('z', false);
    }

    fn click(&mut self, (x, y): (i32, i32)) {
        self.app.event(Event::Move { x, y });
        self.app.event(Event::Down { x, y, clicks: 1 });
        self.app.event(Event::Up { x, y });
        self.app.draw(WIDTH, HEIGHT);
    }

    fn new_project(&mut self) {
        self.click(FILE_CELL_CENTRE);
        self.click(NEW_PROJECT_ITEM_CENTRE);
    }

    fn answer(&mut self, at: usize) {
        match self.waiting.remove(at) {
            Request::Read { path } => {
                let bytes = self.bytes(&path);
                self.app.read(path, bytes);
            }
            Request::Scan { path } => {
                let shelved = self.shelves.iter().filter(|(shelf, _)| *shelf == path).flat_map(|(_, paths)| paths);
                let found = shelved.map(|path| Found { path: String::from(*path), name: String::from(path.rsplit('/').next().unwrap_or(path)), item: Item::Wave(u64::MAX) }).collect();
                self.app.scanned(path, found);
            }
            _ => {}
        }
    }

    fn settle(&mut self) {
        for answered in 0.. {
            self.gather();
            if self.waiting.is_empty() {
                return;
            }
            assert!(answered < MOST_ANSWERS, "wanted the app to stop asking within {MOST_ANSWERS} answers, but after them it still waits on {:?}", self.waiting);
            self.answer(0);
        }
    }

    fn saved(&mut self) -> Json {
        self.saved = None;
        self.key('s', false);
        self.gather();
        json::parse(self.saved.as_deref().expect("wanted the save handed to the shell")).expect("wanted the saved song to parse")
    }

    fn heard(&mut self) -> Vec<usize> {
        self.app.audio_stopped();
        let mut renderer = self.app.audio_started(SAMPLE_RATE, MAX_FRAMES, BLOCK as u32);
        self.app.event(Event::Key { key: Key::Space, shift: false, command: false, repeat: false });
        let (mut left, mut right) = (vec![0.0; LISTENED.1], vec![0.0; LISTENED.1]);
        for (l, r) in left.chunks_mut(BLOCK).zip(right.chunks_mut(BLOCK)) {
            renderer.render(l, r);
        }
        self.app.audio_stopped();
        let window = left.split_off(LISTENED.0);
        (0..KICKS.len()).filter(|kick| amplitude(&window, KICKS[*kick].1) > f64::from(AUDIBLE)).collect()
    }

    fn relaunch(&mut self) {
        self.app.event(Event::Quit);
        self.gather();
        let session = self.session.clone().expect("wanted the session remembered on quitting");
        self.app = launched();
        self.waiting.clear();
        assert!(self.app.resume(&session), "wanted the remembered session to resume");
    }

    fn marked(&self) -> Option<Vec<usize>> {
        let session = json::parse(self.session.as_deref()?)?;
        let tracks = session.get("project")?.get("tracks")?.as_array()?;
        mark_of(tracks.first()?)?;
        let mut marks: Vec<usize> = tracks.iter().filter_map(mark_of).collect();
        marks.sort_unstable();
        marks.dedup();
        Some(marks)
    }

    fn pick(&mut self, below: u64) -> u64 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        self.seed % below
    }

    fn wander(&mut self) -> String {
        match self.pick(MOVES) {
            kick @ 0..=2 => {
                self.open_file(SONGS[kick as usize]);
                format!("open {}", MARKS[kick as usize])
            }
            3 => {
                self.open_file(EMPTY_SONG);
                format!("open {EMPTY_MARK}")
            }
            4 | 5 => {
                self.undo();
                String::from("undo")
            }
            6 => {
                self.key('z', true);
                String::from("redo")
            }
            7 => {
                self.copy();
                String::from("copy")
            }
            8 | 9 => {
                self.paste();
                String::from("paste")
            }
            10 => {
                self.settle();
                let saved = self.saved();
                if self.marked().is_some() {
                    for (kick, wave) in kicks_of(&saved) {
                        let held = wave_in(&saved, &wave);
                        assert!(held.as_ref().is_none_or(|pcm| *pcm == tone(KICKS[kick].1)), "wanted the {} track saved with its own kick or a link to it, got another sound embedded as {wave}", MARKS[kick]);
                    }
                }
                String::from("save")
            }
            _ => {
                self.relaunch();
                String::from("relaunch")
            }
        }
    }

    fn plays_its_own(&mut self, walk: u64, trail: &[String]) -> usize {
        self.settle();
        let Some(marks) = self.marked() else {
            return 0;
        };
        let heard = self.heard();
        self.gather();
        let named = |kicks: &[usize]| kicks.iter().map(|kick| MARKS[*kick]).collect::<Vec<_>>();
        assert_eq!(heard, marks, "walk {walk}: wanted each of the tracks {:?} to play its own kick and no other after {trail:?}, heard the kicks of {:?}", named(&marks), named(&heard));
        1
    }
}

fn launched() -> App {
    let mut app = App::new();
    app.draw(WIDTH, HEIGHT);
    app.take_requests();
    app
}

fn tone(hz: f64) -> Vec<i16> {
    (0..frames(TONE_SECONDS)).map(|frame| (TONE_PEAK * (TAU * hz * frame as f64 / SAMPLE_RATE).sin()).round() as i16).collect()
}

fn song(mark: usize, wave: Json) -> String {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0] = Track::audio();
    project.tracks[0].name = String::from(MARKS[mark]);
    project.tracks[0].clips.push(Clip::audio(0, TICKS_PER_BEAT, String::from(NAME), String::from(NAME)));
    let mut document = project.to_json();
    if let Json::Object(fields) = &mut document {
        fields.push((String::from("waves"), Json::Array(vec![wave])));
    }
    document.write()
}

fn empty() -> String {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].name = String::from(EMPTY_MARK);
    project.to_json().write()
}

fn named() -> Json {
    Json::Object(vec![(String::from("name"), Json::String(String::from(NAME)))])
}

fn linked(kick: usize) -> Json {
    Json::Object(vec![(String::from("name"), Json::String(String::from(NAME))), (String::from("sourcePath"), Json::String(String::from(KICKS[kick].0)))])
}

fn mark_of(track: &Json) -> Option<usize> {
    let name = track.get("name")?.as_str()?;
    MARKS.iter().position(|mark| *mark == name)
}

fn kicks_of(saved: &Json) -> Vec<(usize, String)> {
    let tracks = saved.get("tracks").and_then(Json::as_array).unwrap_or(&[]);
    let kick = |track: &Json| Some((mark_of(track)?, String::from(track.get("clips")?.as_array()?.first()?.get("wave")?.get("name")?.as_str()?)));
    tracks.iter().filter_map(kick).collect()
}

fn wave_in(saved: &Json, name: &str) -> Option<Vec<i16>> {
    let waves = saved.get("waves").and_then(Json::as_array)?;
    let entry = waves.iter().find(|entry| entry.get("name").and_then(Json::as_str) == Some(name))?;
    Wave::from_json(entry).map(|wave| wave.pcm.clone())
}

fn kick_in(saved: &Json) -> Option<Vec<i16>> {
    wave_in(saved, NAME)
}

fn read(kick: usize) -> Request {
    Request::Read { path: String::from(KICKS[kick].0) }
}

#[test]
fn a_track_copied_in_one_song_sounds_and_saves_when_pasted_into_a_new_one() {
    let mut shell = Shell::new(Vec::new());
    shell.open_file(SONGS[LOW]);
    shell.settle();
    shell.copy();
    shell.new_project();
    shell.paste();
    shell.settle();
    assert_eq!(shell.heard(), vec![LOW], "wanted the pasted track to play the low kick it was copied with");
    assert_eq!(kick_in(&shell.saved()), Some(tone(KICKS[LOW].1)), "wanted the new song saved with the low kick embedded");
}

#[test]
fn a_paste_leaves_a_kick_the_open_song_is_still_finding_to_that_song() {
    let mut shell = Shell::new(vec![(LIBRARY, vec![KICKS[HIGH].0])]);
    shell.browse(LIBRARY);
    shell.open_file(SONGS[LOW]);
    shell.settle();
    shell.copy();
    shell.open(SONGS[HIGH], &song(HIGH, named()));
    let asked = shell.gather();
    assert_eq!(asked, vec![Request::Scan { path: String::from(LIBRARY) }], "wanted the second song's kick held on a scan of the browser's folder at the paste, got {asked:?}");
    shell.paste();
    shell.settle();
    assert_eq!(shell.heard(), vec![LOW, HIGH], "wanted the second song to play its own high kick once the browser's folder gave it, beside the low kick of the track pasted in");
    let saved = shell.saved();
    let own = kick_in(&saved);
    assert!(own == Some(tone(KICKS[HIGH].1)), "wanted the second song saved with its own high kick under its name, got {}", if own == Some(tone(KICKS[LOW].1)) { "the first song's low kick" } else { "neither kick" });
    assert!(wave_in(&saved, PASTED) == Some(tone(KICKS[LOW].1)), "wanted the pasted track's low kick saved under a name of its own");
}

#[test]
fn a_track_copied_before_its_wave_was_found_pastes_with_the_audio_found_since() {
    let mut shell = Shell::new(Vec::new());
    shell.open_file(SONGS[LOW]);
    let asked = shell.gather();
    assert_eq!(asked, vec![read(LOW)], "wanted the kick's read still waiting at the copy, got {asked:?}");
    shell.copy();
    shell.settle();
    shell.new_project();
    shell.paste();
    let asked = shell.gather();
    assert!(asked.is_empty(), "wanted the paste to bring the kick found since the copy and ask for nothing, got {asked:?}");
    assert_eq!(shell.heard(), vec![LOW], "wanted the pasted track to play the kick found after the copy");
    assert_eq!(kick_in(&shell.saved()), Some(tone(KICKS[LOW].1)), "wanted the new song saved with the kick embedded");
}

#[test]
fn a_track_copied_and_pasted_before_its_wave_was_read_still_gets_it() {
    let mut shell = Shell::new(Vec::new());
    shell.open_file(SONGS[LOW]);
    shell.gather();
    shell.copy();
    shell.new_project();
    shell.paste();
    shell.settle();
    assert_eq!(shell.heard(), vec![LOW], "wanted the pasted track to play the kick once its read answered in the new song");
    assert_eq!(kick_in(&shell.saved()), Some(tone(KICKS[LOW].1)), "wanted the new song saved with the kick embedded");
}

#[test]
fn a_track_whose_wave_a_folder_holds_pasted_before_the_scan_answered_still_finds_it() {
    let mut shell = Shell::new(vec![(LIBRARY, vec![KICKS[HIGH].0])]);
    shell.browse(LIBRARY);
    shell.open(SONGS[HIGH], &song(HIGH, named()));
    shell.gather();
    shell.copy();
    shell.new_project();
    shell.paste();
    shell.settle();
    assert_eq!(shell.heard(), vec![HIGH], "wanted the pasted track to play the kick the browser's folder holds");
}

#[test]
fn a_track_pasted_into_a_song_with_a_kick_of_that_name_plays_and_saves_its_own() {
    let mut shell = Shell::new(Vec::new());
    shell.open_file(SONGS[LOW]);
    shell.settle();
    shell.open_file(SONGS[HIGH]);
    shell.settle();
    shell.copy();
    shell.paste();
    shell.settle();
    assert_eq!(shell.heard(), vec![HIGH], "wanted the second song to play its high kick after a paste of its own track");
    shell.undo();
    shell.undo();
    shell.settle();
    assert_eq!(shell.heard(), vec![LOW], "wanted undo back to the first song to play its low kick");
    shell.paste();
    shell.settle();
    assert_eq!(shell.heard(), vec![LOW, HIGH], "wanted the first song to play its low kick and the pasted track the high kick it was copied with");
    let saved = shell.saved();
    assert!(kick_in(&saved) == Some(tone(KICKS[LOW].1)), "wanted the first song saved with its own low kick under its name");
    assert!(wave_in(&saved, PASTED) == Some(tone(KICKS[HIGH].1)), "wanted the pasted track's high kick saved under a name of its own");
}

#[test]
fn a_kick_pasted_again_from_its_song_is_kept_once() {
    let mut shell = Shell::new(Vec::new());
    shell.open_file(SONGS[LOW]);
    shell.settle();
    for _ in 0..2 {
        shell.open_file(SONGS[HIGH]);
        shell.settle();
        shell.copy();
        shell.undo();
        shell.settle();
        shell.paste();
        shell.paste();
        shell.settle();
    }
    assert_eq!(shell.heard(), vec![LOW, HIGH], "wanted the first song to play its low kick and the pasted tracks the high kick");
    let saved = shell.saved();
    assert!(wave_in(&saved, PASTED) == Some(tone(KICKS[HIGH].1)), "wanted the high kick saved under a name of its own");
    assert!(wave_in(&saved, SPARE).is_none(), "wanted the high kick kept once however often its track was pasted, got a second copy saved");
    assert_eq!(kicks_of(&saved).len(), 5, "wanted the first song's track and four pasted ones saved");
}

#[test]
fn a_track_pasted_back_into_its_own_song_plays_that_songs_kick() {
    let mut shell = Shell::new(Vec::new());
    shell.open_file(SONGS[LOW]);
    shell.settle();
    shell.open_file(SONGS[HIGH]);
    shell.settle();
    shell.copy();
    shell.undo();
    shell.settle();
    shell.paste();
    shell.settle();
    shell.open_file(SONGS[HIGH]);
    shell.settle();
    shell.paste();
    shell.settle();
    assert_eq!(shell.heard(), vec![HIGH], "wanted the track pasted back into its own song to play that song's high kick");
    let saved = shell.saved();
    assert!(kick_in(&saved) == Some(tone(KICKS[HIGH].1)), "wanted the second song saved with its high kick under its name");
    assert!(wave_in(&saved, PASTED).is_none(), "wanted the second song to keep its kick once, got a copy saved under the name it took in the first song");
}

#[test]
fn every_track_plays_and_saves_its_own_songs_kick_over_random_walks() {
    let mut checks = 0;
    for walk in 1..=WALKS {
        let mut shell = Shell::new(Vec::new());
        shell.seed = walk.wrapping_mul(WALK_SEED) | 1;
        let mut trail = Vec::new();
        for _ in 0..WALK_STEPS {
            trail.push(shell.wander());
            shell.gather();
            match shell.pick(3) {
                0 => checks += shell.plays_its_own(walk, &trail),
                1 if !shell.waiting.is_empty() => {
                    let at = shell.pick(shell.waiting.len() as u64) as usize;
                    shell.answer(at);
                }
                _ => {}
            }
        }
        checks += shell.plays_its_own(walk, &trail);
    }
    assert!(checks > WALKS as usize, "wanted the walks to check a song with a kick more than once each, got {checks} checks over {WALKS} walks");
}
