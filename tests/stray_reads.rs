use bp::app::{App, Event, Opened, Request};
use bp::audio::Renderer;
use bp::browser::{Found, Item, Kind};
use bp::face::{CELL, PITCH};
use bp::json::{self, Json};
use bp::keyboard::Key;
use bp::project::{Clip, Project, TICKS_PER_BEAT, Track};
use std::f64::consts::TAU;
use support::{AUDIBLE, BLOCK, HEIGHT, SAMPLE_RATE, WIDTH, amplitude, browser_rect, frames, painted, wav};

const LIBRARY: &str = "/stray/library";
const HEARTS: &str = "/Hearts.json";
const KICK: &str = "/stray/library/kick.wav";
const SNARE: &str = "/stray/library/snare.wav";
const TWIN: &str = "/stray/desk/kick.wav";
const SONG: &str = "/stray/songs/song.json";
const NAME: &str = "kick";
const TONES: [(&str, f64); 3] = [(KICK, 300.0), (SNARE, 700.0), (TWIN, 1_200.0)];
const TONE_SECONDS: f64 = 0.25;
const TONE_PEAK: f64 = 8_192.0;
const LISTENED: (usize, usize) = (2_400, 9_600);
const MAX_FRAMES: usize = 4_096;
const FOLDER_ROW: i32 = 0;
const KICK_ROW: i32 = 1;
const SNARE_ROW: i32 = 2;
const STOP_BUTTON: (i32, i32) = (219, 26);
const BELOW_THE_TRACKS: (i32, i32) = (600, 700);
const IDLE_KEY: Key = Key::Char('x');
const MOST_ANSWERS: usize = 12;

struct Shell {
    app: App,
    unreadable: Vec<&'static str>,
    waiting: Vec<Request>,
    session: Option<String>,
}

impl Shell {
    fn new() -> Shell {
        let mut app = App::new();
        app.draw(WIDTH, HEIGHT);
        app.take_requests();
        Shell { app, unreadable: Vec::new(), waiting: Vec::new(), session: None }
    }

    fn browsing() -> Shell {
        let mut shell = Shell::new();
        shell.app.add_folder(String::from(LIBRARY));
        shell.app.listed(LIBRARY, library());
        shell.asked();
        shell
    }

    fn asked(&mut self) -> Vec<Request> {
        let mut asked = Vec::new();
        for request in self.app.take_requests() {
            match request {
                Request::Remember { json } => self.session = Some(json),
                Request::Read { path } if path.ends_with(HEARTS) => self.app.read(path, None),
                Request::Read { .. } | Request::Scan { .. } => asked.push(request),
                _ => {}
            }
        }
        self.waiting.extend(asked.iter().cloned());
        asked
    }

    fn answer(&mut self, request: Request) {
        match request {
            Request::Read { path } => {
                let bytes = (!self.unreadable.contains(&path.as_str())).then(|| file(&path));
                self.app.read(path, bytes);
            }
            Request::Scan { path } => self.app.scanned(path, library()),
            _ => {}
        }
    }

    fn settle(&mut self) {
        for answered in 0.. {
            self.asked();
            if self.waiting.is_empty() {
                return;
            }
            assert!(answered < MOST_ANSWERS, "wanted the app to stop asking within {MOST_ANSWERS} answers, but after them it still waits on {:?}", self.waiting);
            let request = self.waiting.remove(0);
            self.answer(request);
        }
    }

    fn open(&mut self, song: &str) {
        let opened = self.app.opened(String::from(SONG), Some(song), None);
        assert_eq!(opened, Opened::Project, "wanted {SONG} to open as a song, got {opened:?}");
    }

    fn row(&mut self, row: i32) -> (i32, i32) {
        let browser = browser_rect(&painted(&mut self.app));
        (browser.x + browser.w / 2, browser.y + row * PITCH + CELL / 2)
    }

    fn press(&mut self, (x, y): (i32, i32)) {
        self.app.event(Event::Move { x, y });
        self.app.event(Event::Down { x, y, clicks: 1 });
        self.app.event(Event::Up { x, y });
    }

    fn press_row(&mut self, row: i32) {
        let at = self.row(row);
        self.press(at);
    }

    fn key(&mut self, key: Key) {
        self.app.event(Event::Key { key, shift: false, command: false, repeat: false });
    }

    fn audition_kick(&mut self) {
        let (x, y) = self.row(KICK_ROW);
        self.app.event(Event::Move { x, y });
        self.settle();
        self.press((x, y));
        let asked = self.asked();
        assert_eq!(asked, vec![read(KICK)], "wanted the press to ask for the file to play, its picture already read, got {asked:?}");
    }

    fn drop_wave(&mut self, path: &str) {
        self.app.dropped(String::from(path), Kind::Wave, BELOW_THE_TRACKS.0, BELOW_THE_TRACKS.1);
        self.settle();
    }

    fn waves_placed(&self) -> Vec<String> {
        let session = json::parse(self.session.as_deref().expect("wanted the session remembered")).expect("wanted the remembered session to parse");
        let project = Project::from_json(session.get("project").expect("wanted the remembered session to hold the project")).expect("wanted the remembered project to read");
        project.tracks.iter().flat_map(|track| &track.clips).filter_map(|clip| clip.file.as_ref()).map(|file| file.name.clone()).collect()
    }

    fn listening(&mut self) -> Renderer {
        self.app.audio_started(SAMPLE_RATE, MAX_FRAMES, BLOCK as u32)
    }
}

fn tone(hz: f64) -> Vec<i16> {
    (0..frames(TONE_SECONDS)).map(|frame| (TONE_PEAK * (TAU * hz * frame as f64 / SAMPLE_RATE).sin()).round() as i16).collect()
}

fn file(path: &str) -> Vec<u8> {
    let (_, hz) = TONES.iter().find(|(known, _)| *known == path).unwrap_or_else(|| panic!("wanted a tone for {path}"));
    wav(&tone(*hz))
}

fn library() -> Vec<Found> {
    [KICK, SNARE].iter().map(|path| Found { path: String::from(*path), name: String::from(path.rsplit('/').next().unwrap_or(path)), item: Item::Wave(file(path).len() as u64) }).collect()
}

fn song(source: &str) -> String {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0] = Track::audio();
    project.tracks[0].clips.push(Clip::audio(0, TICKS_PER_BEAT, String::from(NAME), String::from(NAME)));
    let mut document = project.to_json();
    let text = |value: &str| Json::String(String::from(value));
    if let Json::Object(fields) = &mut document {
        fields.push((String::from("waves"), Json::Array(vec![Json::Object(vec![(String::from("name"), text(NAME)), (String::from("sourcePath"), text(source))])])));
    }
    document.write()
}

fn read(path: &str) -> Request {
    Request::Read { path: String::from(path) }
}

fn sounding(renderer: &mut Renderer) -> Vec<&'static str> {
    let (mut left, mut right) = (vec![0.0; LISTENED.1], vec![0.0; LISTENED.1]);
    for (l, r) in left.chunks_mut(BLOCK).zip(right.chunks_mut(BLOCK)) {
        renderer.render(l, r);
    }
    let window = left.split_off(LISTENED.0);
    TONES.iter().filter(|(_, hz)| amplitude(&window, *hz) > f64::from(AUDIBLE)).map(|(path, _)| *path).collect()
}

fn notice_shown(app: &mut App) -> bool {
    let before = painted(app);
    app.event(Event::Key { key: IDLE_KEY, shift: false, command: true, repeat: false });
    before != painted(app)
}

fn assert_no_wave_left(shell: &mut Shell, stopped: &str) {
    shell.drop_wave(TWIN);
    let placed = shell.waves_placed();
    assert_eq!(placed, vec![NAME], "wanted another file named {NAME}, dropped on the song after {stopped}, to keep its plain name, the stopped audition having left no wave of that name, got {placed:?}");
}

fn assert_song_plays(shell: &mut Shell, when: &str) {
    let mut renderer = shell.listening();
    shell.key(Key::Space);
    let heard = sounding(&mut renderer);
    assert_eq!(heard, vec![KICK], "wanted the song to play its kick {when}, heard {heard:?}");
}

#[test]
fn an_audition_stopped_by_a_second_press_before_its_file_is_read_leaves_no_wave_behind() {
    let mut shell = Shell::browsing();
    shell.audition_kick();
    shell.press_row(KICK_ROW);
    shell.settle();
    assert_no_wave_left(&mut shell, "a second press of the row stopped its audition before the file was read");
}

#[test]
fn an_audition_stopped_by_the_stop_button_before_its_file_is_read_leaves_no_wave_behind() {
    let mut shell = Shell::browsing();
    shell.audition_kick();
    shell.press(STOP_BUTTON);
    shell.settle();
    assert_no_wave_left(&mut shell, "the Stop button stopped its audition before the file was read");
}

#[test]
fn an_audition_left_for_another_file_before_its_own_is_read_leaves_no_wave_behind() {
    let mut shell = Shell::browsing();
    shell.audition_kick();
    shell.press_row(SNARE_ROW);
    shell.settle();
    assert_no_wave_left(&mut shell, "a press of the next file took over its audition before the file was read");
}

#[test]
fn an_audition_begun_and_left_by_the_down_key_before_its_file_is_read_leaves_no_wave_behind() {
    let mut shell = Shell::browsing();
    shell.press_row(FOLDER_ROW);
    shell.key(Key::Down);
    let asked = shell.asked();
    assert_eq!(asked, vec![read(KICK)], "wanted the down key to step from the folder onto the kick and ask for it, got {asked:?}");
    shell.key(Key::Down);
    shell.settle();
    assert_no_wave_left(&mut shell, "the down key stepped past it before the file was read");
}

#[test]
fn an_audition_cut_short_by_opening_a_song_before_its_file_is_read_leaves_no_wave_behind() {
    let mut shell = Shell::browsing();
    shell.audition_kick();
    shell.open(&Project::new().to_json().write());
    shell.settle();
    assert_no_wave_left(&mut shell, "a song was opened over its audition before the file was read");
}

#[test]
fn a_row_pressed_again_after_its_stopped_audition_was_read_asks_for_the_file_again() {
    let mut shell = Shell::browsing();
    shell.audition_kick();
    shell.press_row(KICK_ROW);
    shell.settle();
    shell.press_row(KICK_ROW);
    let asked = shell.asked();
    assert_eq!(asked, vec![read(KICK)], "wanted the next press of the row to ask for the file again, nothing kept from the audition stopped before it was read, got {asked:?}");
}

#[test]
fn a_file_that_fails_to_read_after_its_audition_was_stopped_shows_no_notice() {
    let mut shell = Shell::browsing();
    shell.audition_kick();
    shell.press_row(KICK_ROW);
    shell.unreadable.push(KICK);
    shell.settle();
    assert!(!notice_shown(&mut shell.app), "wanted no notice for a file nothing was waiting for once its audition was stopped, but the next key press took one off the screen");
}

#[test]
fn a_stopped_auditions_read_still_lands_a_drop_of_that_file_made_while_it_was_out() {
    let mut shell = Shell::browsing();
    shell.audition_kick();
    shell.press_row(KICK_ROW);
    shell.drop_wave(KICK);
    let placed = shell.waves_placed();
    assert_eq!(placed, vec![NAME], "wanted the kick dropped on the song to land once, though the audition that first asked for its file was stopped, got {placed:?}");
}

#[test]
fn a_song_reopened_after_its_missing_file_came_back_reads_it_and_plays() {
    let mut shell = Shell::new();
    shell.unreadable.push(KICK);
    shell.open(&song(KICK));
    shell.settle();
    assert!(notice_shown(&mut shell.app), "wanted a notice that the song's kick would not read, but the next key press left the screen as it was");
    shell.unreadable.clear();
    shell.open(&song(KICK));
    shell.settle();
    assert_song_plays(&mut shell, "once it was opened again with the file back");
}

#[test]
fn a_browser_file_its_song_holds_as_missing_is_read_when_pressed_and_is_heard_and_the_song_then_plays_it() {
    let mut shell = Shell::browsing();
    shell.unreadable.push(KICK);
    shell.open(&song(KICK));
    shell.settle();
    assert!(notice_shown(&mut shell.app), "wanted a notice that the song's kick would not read, but the next key press left the screen as it was");
    shell.unreadable.clear();
    let mut renderer = shell.listening();
    shell.audition_kick();
    shell.settle();
    let heard = sounding(&mut renderer);
    assert_eq!(heard, vec![KICK], "wanted the kick heard once the press's read was answered, its song holding it as missing until then, heard {heard:?}");
    assert_song_plays(&mut shell, "once the audition's read had loaded the wave it was missing");
}
