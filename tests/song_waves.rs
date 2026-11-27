use bp::app::{App, Event, Opened, Request};
use bp::arp::{Arp, Articulation};
use bp::browser::{Found, Item, Kind};
use bp::core::Params;
use bp::draw::{Fill, Rect, Tone};
use bp::font;
use bp::json::{self, Json};
use bp::keyboard::Key;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, Track, VELOCITY, beats};
use bp::wav::Wave;
use std::f64::consts::TAU;
use std::time::{Duration, Instant};
use support::{AUDIBLE, BLOCK, HEIGHT, SAMPLE_RATE, WIDTH, amplitude, call, centre, core_layer, frames, midi, painted, wav};

const A: &str = "/song-waves/songs/a.json";
const B: &str = "/song-waves/songs/b.json";
const SECOND: &str = "/song-waves/songs/second.json";
const ONE: &str = "/song-waves/songs/one.json";
const COPY: &str = "/song-waves/songs/copy.json";
const DEFAULT: &str = "/song-waves/default.json";
const KICK_FILE: &str = "/song-waves/takes/kick.wav";
const OTHER_KICK_FILE: &str = "/song-waves/desk/kick.wav";
const SNARE_FILE: &str = "/song-waves/takes/snare.wav";
const BOUNCE_FILE: &str = "/song-waves/takes/Master Bounce.wav";
const STOWED: &str = "/song-waves/stowed";
const TAKES: &str = "/song-waves/takes";
const GONE_FILE: &str = "/song-waves/gone/kick.wav";
const KICK: &str = "kick";
const SNARE: &str = "snare";
const BOUNCE: &str = "Master Bounce";
const A_TRACK: &str = "A";
const A_RESAVED: &str = "A resaved";
const A_EDITED: &str = "A edited";
const A_LATER: &str = "A later";
const B_TRACK: &str = "B";
const B_EDITED: &str = "B edited";
const FIRST: &str = "first song";
const NEXT: &str = "second song";
const LOW: &str = "low";
const MID: &str = "mid";
const HIGH: &str = "high";
const TONES: [(&str, f64); 3] = [(LOW, 300.0), (MID, 700.0), (HIGH, 1_200.0)];
const TONE_SECONDS: f64 = 0.25;
const TONE_PEAK: f64 = 8_192.0;
const LISTENED: (usize, usize) = (2_400, 9_600);
const MAX_FRAMES: usize = 4_096;
const MOST_ANSWERS: usize = 12;
const IDLE_KEY: Key = Key::Char('x');
const FILE_CELL_CENTRE: (i32, i32) = (60, 26);
const NEW_PROJECT_ITEM_CENTRE: (i32, i32) = (60, 69);
const SETTINGS_ITEM_CENTRE: (i32, i32) = (60, 389);
const LIST_ROWS_TOP: i32 = 90;
const ROW_PITCH: i32 = 33;
const ROW_MIDDLE: i32 = 14;
const SECOND_ROW: i32 = 1;
const BELOW_THE_TRACKS: (i32, i32) = (WIDTH - 20, HEIGHT - 20);
const RAIL: i32 = 45;
const RAIL_W: i32 = 1900;
const RAIL_STEP: usize = 2;
const TICK: f32 = 1.0 / 60.0;
const PATIENCE: Duration = Duration::from_secs(60);
const HELD_BEATS: f32 = 8.0;
const LOOP: (i32, i32) = (2 * TICKS_PER_BEAT, 6 * TICKS_PER_BEAT);
const LOUD: u16 = 300;

struct Save {
    to: Option<String>,
    text: String,
    id: u64,
}

impl Save {
    fn file(&self) -> Json {
        parsed(&self.text)
    }
}

struct Shell {
    app: App,
    files: Vec<(String, Vec<u8>)>,
    waiting: Vec<Request>,
    session: Option<String>,
    saves: Vec<Save>,
    stows: Option<bool>,
}

impl Shell {
    fn new(files: &[(&str, Vec<u8>)]) -> Shell {
        let mut app = App::new();
        app.draw(WIDTH, HEIGHT);
        app.take_requests();
        Shell { app, files: files.iter().map(|(path, bytes)| (String::from(*path), bytes.clone())).collect(), waiting: Vec::new(), session: None, saves: Vec::new(), stows: None }
    }

    fn bytes(&self, path: &str) -> Option<Vec<u8>> {
        self.files.iter().find(|(known, _)| known == path).map(|(_, bytes)| bytes.clone())
    }

    fn asked(&mut self) -> Vec<Request> {
        let mut asked = Vec::new();
        loop {
            let mut stowed = false;
            for request in self.app.take_requests() {
                match request {
                    Request::Remember { json } => self.session = Some(json),
                    Request::Write { path, json, save } => self.saves.push(Save { to: Some(path), text: json, id: save }),
                    Request::SaveAs { json, save } => self.saves.push(Save { to: None, text: json, id: save }),
                    Request::Read { .. } | Request::Scan { .. } => asked.push(request),
                    Request::Stow { file, bytes } => {
                        if let Some(written) = self.stows {
                            let path = format!("{STOWED}/{file}");
                            if written {
                                self.files.push((path.clone(), bytes));
                            }
                            self.app.stowed(path, written);
                            stowed = true;
                        }
                    }
                    _ => {}
                }
            }
            if !stowed {
                break;
            }
        }
        self.waiting.extend(asked.iter().cloned());
        asked
    }

    fn answer(&mut self, request: Request) {
        match request {
            Request::Read { path } => {
                let bytes = self.bytes(&path);
                self.app.read(path, bytes);
            }
            Request::Scan { path } => {
                let found = self.listing(&path);
                self.app.scanned(path, found);
            }
            _ => {}
        }
    }

    fn listing(&self, root: &str) -> Vec<Found> {
        let under = format!("{root}/");
        let waves = self.files.iter().map(|(path, _)| path).filter(|path| path.starts_with(&under) && path.ends_with(".wav"));
        waves.map(|path| Found { path: path.clone(), name: String::from(&path[under.len()..]), item: Item::Wave(u64::MAX) }).collect()
    }

    fn answer_waiting(&mut self) {
        for request in std::mem::take(&mut self.waiting) {
            self.answer(request);
        }
    }

    fn settle(&mut self) {
        self.drain(|waiting| (!waiting.is_empty()).then_some(0));
    }

    fn settle_reads(&mut self) {
        self.drain(|waiting| waiting.iter().position(|request| matches!(request, Request::Read { .. })));
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

    fn begin_open(&mut self, path: &str) {
        let text = String::from_utf8(self.bytes(path).unwrap_or_else(|| panic!("wanted {path} among the shell's files"))).expect("wanted the song as text");
        let opened = self.app.opened(String::from(path), Some(&text), None);
        assert_eq!(opened, Opened::Project, "wanted {path} to open as a song, got {opened:?}");
    }

    fn open(&mut self, path: &str) {
        self.begin_open(path);
        self.settle();
    }

    fn key(&mut self, key: Key, shift: bool, command: bool) -> bool {
        self.app.event(Event::Key { key, shift, command, repeat: false })
    }

    fn undo(&mut self) {
        assert!(self.key(Key::Char('z'), false, true), "wanted an undo to take the app back a step, got nothing undone");
    }

    fn redo(&mut self) {
        assert!(self.key(Key::Char('z'), true, true), "wanted a redo to take the app forward a step, got nothing redone");
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
        self.asked();
    }

    fn press_settings_row(&mut self, row: i32) {
        self.click(FILE_CELL_CENTRE);
        self.click(SETTINGS_ITEM_CENTRE);
        self.click((list_left() + 20, LIST_ROWS_TOP + ROW_PITCH * row + ROW_MIDDLE));
    }

    fn browse(&mut self, folder: &str) {
        self.app.dropped(String::from(folder), Kind::Folder, BELOW_THE_TRACKS.0, BELOW_THE_TRACKS.1);
        self.settle();
    }

    fn drop_wave(&mut self, path: &str) {
        self.app.dropped(String::from(path), Kind::Wave, BELOW_THE_TRACKS.0, BELOW_THE_TRACKS.1);
        let asked = self.asked();
        assert_eq!(asked, vec![read(path)], "wanted the drop to ask for {path} alone, got {asked:?}");
    }

    fn rename_track(&mut self, name: &str) {
        let reply = call(&mut self.app, "write", &format!(r#"{{"path":"tracks/0/name","value":"{name}"}}"#));
        assert!(!reply.failed, "wanted the first track renamed {name}, got {}", reply.text);
        self.asked();
    }

    fn pressed_save(&mut self, shift: bool) -> Save {
        self.asked();
        self.saves.clear();
        self.key(Key::Char('s'), shift, true);
        self.asked();
        assert_eq!(self.saves.len(), 1, "wanted exactly one save handed to the shell per press, got {}", self.saves.len());
        self.saves.remove(0)
    }

    fn save(&mut self) -> Save {
        self.pressed_save(false)
    }

    fn put(&mut self, path: &str, text: &str) {
        self.files.retain(|(known, _)| known != path);
        self.files.push((String::from(path), text.as_bytes().to_vec()));
    }

    fn written(&mut self, save: Save, chosen: &str) -> String {
        let path = save.to.clone().unwrap_or_else(|| {
            self.app.assured(save.id);
            String::from(chosen)
        });
        self.put(&path, &save.text);
        self.app.saved(path, save.id);
        self.asked();
        save.text
    }

    fn save_to(&mut self, chosen: &str) -> String {
        let save = self.save();
        self.written(save, chosen)
    }

    fn save_as(&mut self, chosen: &str) -> String {
        let save = self.pressed_save(true);
        assert!(save.to.is_none(), "wanted Save As to ask where to save, got a write to {:?}", save.to);
        self.written(save, chosen)
    }

    fn listened(&mut self) -> Vec<f32> {
        let mut renderer = self.app.audio_started(SAMPLE_RATE, MAX_FRAMES, BLOCK as u32);
        self.key(Key::Space, false, false);
        let (mut left, mut right) = (vec![0.0; LISTENED.1], vec![0.0; LISTENED.1]);
        for (l, r) in left.chunks_mut(BLOCK).zip(right.chunks_mut(BLOCK)) {
            renderer.render(l, r);
        }
        self.key(Key::Space, false, false);
        left.split_off(LISTENED.0)
    }

    fn remembered(&self) -> Option<Project> {
        let session = parsed(self.session.as_deref()?);
        Project::from_json(session.get("project")?)
    }

    fn quit(&mut self) -> String {
        self.app.event(Event::Quit);
        self.asked();
        self.session.clone().expect("wanted the session remembered on quitting")
    }

    fn project(&mut self) -> Project {
        self.quit();
        self.remembered().expect("wanted the remembered session to hold a project that reads")
    }

    fn first_track(&mut self) -> String {
        self.project().tracks[0].name.clone()
    }

    fn clip_waves(&mut self) -> Vec<String> {
        self.project().tracks.iter().flat_map(|track| &track.clips).filter_map(|clip| clip.file.as_ref()).map(|file| file.name.clone()).collect()
    }

    fn relaunched(&mut self) -> Shell {
        let session = self.quit();
        let files: Vec<(&str, Vec<u8>)> = self.files.iter().map(|(path, bytes)| (path.as_str(), bytes.clone())).collect();
        let mut shell = Shell::new(&files);
        shell.stows = self.stows;
        assert!(shell.app.resume(&session), "wanted the remembered session to resume");
        shell
    }

    fn notice_shown(&mut self) -> bool {
        let before = painted(&mut self.app);
        self.key(IDLE_KEY, false, true);
        before != painted(&mut self.app)
    }
}

fn parsed(text: &str) -> Json {
    json::parse(text).expect("wanted the text to parse as JSON")
}

fn read(path: &str) -> Request {
    Request::Read { path: String::from(path) }
}

fn scan(path: &str) -> Request {
    Request::Scan { path: String::from(path) }
}

fn list_left() -> i32 {
    12 + 2 * (font::width("FLATTEN GROUPS AS ONE", 2) + 2 * 10) + 2 * (2 * 8) + 20
}

fn hz_of(tone: &str) -> f64 {
    TONES.iter().find(|(known, _)| *known == tone).map(|(_, hz)| *hz).unwrap_or_else(|| panic!("wanted a pitch for the {tone} tone"))
}

fn tone(named: &str) -> Vec<i16> {
    let hz = hz_of(named);
    (0..frames(TONE_SECONDS)).map(|frame| (TONE_PEAK * (TAU * hz * frame as f64 / SAMPLE_RATE).sin()).round() as i16).collect()
}

fn embedded(name: &str, named: &str) -> Json {
    Wave::sound(String::from(name), SAMPLE_RATE, 1, tone(named)).to_json()
}

fn linked(name: &str, source: &str) -> Json {
    Json::Object(vec![(String::from("name"), Json::String(String::from(name))), (String::from("sourcePath"), Json::String(String::from(source)))])
}

fn song(track: &str, wave: &str, waves: Vec<Json>) -> String {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0] = Track::audio();
    project.tracks[0].name = String::from(track);
    project.tracks[0].clips.push(Clip::audio(0, TICKS_PER_BEAT, String::from(wave), String::from(wave)));
    let mut document = project.to_json();
    if let Json::Object(fields) = &mut document {
        fields.push((String::from("waves"), Json::Array(waves)));
    }
    document.write()
}

fn low_kick_song() -> String {
    song(A_TRACK, KICK, vec![embedded(KICK, LOW)])
}

fn high_kick_song() -> String {
    song(B_TRACK, KICK, vec![embedded(KICK, HIGH)])
}

fn lost_kick_song() -> String {
    song(A_TRACK, KICK, vec![linked(KICK, GONE_FILE)])
}

fn saved_again_as(path: &str, text: &str, renamed: &str) -> String {
    let mut shell = Shell::new(&[(path, text.as_bytes().to_vec())]);
    shell.open(path);
    shell.save_to(path);
    shell.rename_track(renamed);
    let file = shell.save_to(path);
    let held = version_tracks(&parsed(&file));
    assert_eq!(held, vec![A_TRACK, renamed], "wanted the song saved twice to hold both saves as versions, oldest first, got {held:?}");
    file
}

fn saved_twice(path: &str, text: &str) -> String {
    saved_again_as(path, text, A_RESAVED)
}

fn disk(a: String, b: String) -> Vec<(&'static str, Vec<u8>)> {
    vec![(A, a.into_bytes()), (B, b.into_bytes()), (KICK_FILE, wav(&tone(HIGH))), (OTHER_KICK_FILE, wav(&tone(LOW))), (SNARE_FILE, wav(&tone(MID))), (BOUNCE_FILE, wav(&tone(MID)))]
}

fn two_songs(files: &[(&str, Vec<u8>)]) -> Shell {
    let mut shell = Shell::new(files);
    shell.open(A);
    shell.open(B);
    shell
}

fn tones_in(pcm: &[i16]) -> Vec<&'static str> {
    let samples: Vec<f32> = pcm.iter().map(|sample| f32::from(*sample) / f32::from(i16::MAX)).collect();
    TONES.iter().filter(|(_, hz)| amplitude(&samples, *hz) > f64::from(AUDIBLE)).map(|(name, _)| *name).collect()
}

fn saved_waves(file: &Json) -> Vec<(String, Vec<&'static str>)> {
    let entries = file.get("waves").and_then(Json::as_array).unwrap_or(&[]);
    entries.iter().map(|entry| (String::from(entry.get("name").and_then(Json::as_str).unwrap_or("")), tones_in(&Wave::from_json(entry).map_or_else(Vec::new, |wave| wave.pcm)))).collect()
}

fn version_tracks(file: &Json) -> Vec<String> {
    let versions = file.get("versions").and_then(Json::as_array).unwrap_or(&[]);
    let first_track = |version: &Json| version.get("project")?.get("tracks")?.as_array()?.first()?.get("name")?.as_str().map(String::from);
    versions.iter().map(|version| first_track(version).unwrap_or_default()).collect()
}

fn bounce_in(file: &Json) -> Vec<i16> {
    let entries = file.get("waves").and_then(Json::as_array).unwrap_or(&[]);
    entries.iter().find(|entry| entry.get("name").and_then(Json::as_str) == Some(BOUNCE)).and_then(Wave::from_json).map_or_else(Vec::new, |wave| wave.pcm)
}

fn assert_heard(shell: &mut Shell, wanted: &str, when: &str) {
    let window = shell.listened();
    let levels: Vec<(&str, f64)> = TONES.iter().map(|(name, hz)| (*name, amplitude(&window, *hz))).collect();
    let sounding: Vec<&str> = levels.iter().filter(|(_, level)| *level > f64::from(AUDIBLE)).map(|(name, _)| *name).collect();
    assert_eq!(sounding, vec![wanted], "wanted the {wanted} tone alone {when}, heard {levels:?}");
}

fn assert_saved_kick(save: &Save, path: &str, wanted: &str, when: &str) {
    assert_eq!(save.to.as_deref(), Some(path), "wanted the save {when} written to {path}, got {:?}", save.to);
    let waves = saved_waves(&save.file());
    assert_eq!(waves, vec![(String::from(KICK), vec![wanted])], "wanted the save {when} to embed the kick with its own {wanted} tone, got {waves:?}");
}

fn assert_keeps_versions(save: &Save, when: &str) {
    let kept = version_tracks(&save.file());
    assert!(kept.starts_with(&[String::from(A_TRACK), String::from(A_RESAVED)]), "wanted the save {when} to keep the two versions the song's file held, {A_TRACK} then {A_RESAVED}, got the versions {kept:?}");
}

fn synth_song() -> Project {
    let mut patch = Params::DEFAULT;
    patch.amp.sustain = 1.0;
    let note = Note { pitch: 60, start: 0.0, length: HELD_BEATS, velocity: VELOCITY, articulation: Articulation::PLAIN };
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(0.8, patch))];
    project.tracks[0].clips = vec![Clip { start: 0, length: HELD_BEATS as i32 * TICKS_PER_BEAT, label: String::new(), notes: vec![note], file: None }];
    project
}

fn rail_wash(fills: &[Fill]) -> Option<Rect> {
    fills.iter().find(|fill| fill.tone == Tone::Ink25 && fill.rect.y < RAIL && fill.rect.h >= 30).map(|fill| fill.rect)
}

fn buttons(app: &mut App) -> Vec<Rect> {
    let mut found: Vec<Rect> = Vec::new();
    for x in (0..RAIL_W).step_by(RAIL_STEP) {
        app.event(Event::Move { x, y: RAIL / 2 });
        if let Some(rect) = rail_wash(app.draw(RAIL_W, HEIGHT))
            && found.last() != Some(&rect)
        {
            found.push(rect);
        }
    }
    found
}

fn unsaved(files: &[(&str, Vec<u8>)], range: Option<(i32, i32)>) -> Shell {
    let range = range.map_or_else(String::new, |(start, end)| format!(",\"loopStartBeat\":{},\"loopEndBeat\":{}", beats(start), beats(end)));
    let mut shell = Shell::new(files);
    assert!(shell.app.resume(&format!("{{\"project\":{}{range}}}", synth_song().to_json().write())), "wanted the unsaved song resumed");
    painted(&mut shell.app);
    shell.asked();
    shell
}

fn bounce(shell: &mut Shell, before: usize) {
    let usual = buttons(&mut unsaved(&[], None).app);
    let extra = buttons(&mut shell.app).into_iter().filter(|rect| !usual.contains(rect));
    let cell = extra.min_by_key(|rect| rect.x).expect("wanted the BOUNCE cell on the rail once a loop was set");
    shell.click(centre(cell));
    let started = Instant::now();
    while shell.remembered().is_none_or(|project| project.tracks.len() <= before) {
        assert!(started.elapsed() < PATIENCE, "wanted the bounce landed as a track within {PATIENCE:?}");
        std::thread::sleep(Duration::from_millis(1));
        shell.app.tick(TICK);
        shell.asked();
    }
    shell.app.draw(WIDTH, HEIGHT);
}

fn bounced(files: &[(&str, Vec<u8>)]) -> (Shell, Vec<i16>) {
    bounced_stowing(files, None)
}

fn bounced_stowing(files: &[(&str, Vec<u8>)], stows: Option<bool>) -> (Shell, Vec<i16>) {
    let mut shell = unsaved(files, Some(LOOP));
    shell.stows = stows;
    bounce(&mut shell, synth_song().tracks.len());
    let save = shell.save();
    let bounce = bounce_in(&save.file());
    assert!(save.to.is_none() && bounce.iter().any(|sample| sample.unsigned_abs() > LOUD), "wanted an unsaved song holding a bounce that sounds, got a save to {:?} with {} samples of {BOUNCE}", save.to, bounce.len());
    (shell, bounce)
}

fn another_songs_edit_pressed() -> Shell {
    let mut shell = Shell::new(&disk(saved_twice(A, &low_kick_song()), high_kick_song()));
    shell.open(A);
    shell.rename_track(A_EDITED);
    shell.open(B);
    shell.press_settings_row(SECOND_ROW);
    shell.settle();
    let shown = shell.first_track();
    assert_eq!(shown, A_EDITED, "wanted the press on the second row of EDITS to bring back the first song's edit, its track named {A_EDITED}, got {shown}");
    shell
}

#[test]
fn undo_back_to_a_song_plays_its_own_wave_and_redo_plays_the_next_songs_though_both_embed_one_of_that_name() {
    let mut shell = two_songs(&disk(low_kick_song(), high_kick_song()));
    assert_heard(&mut shell, HIGH, "from the second song once opened");
    shell.undo();
    let asked = shell.asked();
    shell.settle();
    assert_heard(&mut shell, LOW, "after undo back to the first song, whose kick it is");
    assert!(asked.is_empty(), "wanted nothing read again on undo back to a song whose wave the app still holds, got {asked:?}");
    shell.redo();
    let asked = shell.asked();
    shell.settle();
    assert_heard(&mut shell, HIGH, "after redo to the second song, whose kick it is");
    assert!(asked.is_empty(), "wanted nothing read again on redo to a song whose wave the app still holds, got {asked:?}");
}

#[test]
fn a_save_after_undo_back_to_a_song_embeds_its_own_wave_and_after_redo_the_next_songs_though_both_embed_one_of_that_name() {
    let mut shell = two_songs(&disk(low_kick_song(), high_kick_song()));
    shell.undo();
    shell.settle();
    assert_saved_kick(&shell.save(), A, LOW, "after undo back to the first song");
    shell.redo();
    shell.settle();
    assert_saved_kick(&shell.save(), B, HIGH, "after redo to the second song");
}

#[test]
fn a_song_that_links_its_wave_plays_the_linked_file_though_the_song_before_embedded_one_of_that_name() {
    let mut shell = two_songs(&disk(low_kick_song(), song(B_TRACK, KICK, vec![linked(KICK, KICK_FILE)])));
    assert_heard(&mut shell, HIGH, "from the second song, which links its kick to a file holding that tone");
    assert!(!shell.notice_shown(), "wanted no notice over the second song, whose linked kick read, but the next key press took one off the screen");
}

#[test]
fn a_save_of_a_song_that_links_its_wave_embeds_the_linked_file_though_the_song_before_embedded_one_of_that_name() {
    let mut shell = two_songs(&disk(low_kick_song(), song(B_TRACK, KICK, vec![linked(KICK, KICK_FILE)])));
    assert_saved_kick(&shell.save(), B, HIGH, "of the second song, which links its kick to a file holding that tone,");
}

#[test]
fn a_save_after_undo_back_to_a_song_keeps_the_versions_its_file_held() {
    let mut shell = two_songs(&disk(saved_twice(A, &low_kick_song()), high_kick_song()));
    shell.undo();
    shell.settle();
    assert_keeps_versions(&shell.save(), "after undo back to the first song");
}

#[test]
fn a_save_after_a_relaunch_and_undo_back_to_a_song_keeps_the_versions_its_file_held() {
    let mut shell = two_songs(&disk(saved_twice(A, &low_kick_song()), high_kick_song())).relaunched();
    shell.settle();
    shell.undo();
    shell.settle();
    assert_keeps_versions(&shell.save(), "after a relaunch on the second song and undo back to the first");
}

#[test]
fn a_relaunch_reads_the_open_songs_file_alone_and_shows_no_notice() {
    let mut shell = two_songs(&disk(low_kick_song(), song(B_TRACK, SNARE, vec![embedded(SNARE, HIGH)]))).relaunched();
    let asked = shell.asked();
    assert_eq!(asked, vec![read(B)], "wanted the relaunch to read the open song's file alone, the earlier song's wave waiting until it is returned to, got {asked:?}");
    shell.settle();
    assert!(!shell.notice_shown(), "wanted no notice over the relaunched song, whose snare read, but the next key press took one off the screen");
    assert_heard(&mut shell, HIGH, "from the relaunched song");
}

#[test]
fn undo_back_to_an_earlier_song_after_a_relaunch_reads_its_file_and_it_plays() {
    let mut shell = two_songs(&disk(low_kick_song(), song(B_TRACK, SNARE, vec![embedded(SNARE, HIGH)]))).relaunched();
    shell.settle();
    shell.undo();
    let asked = shell.asked();
    assert_eq!(asked, vec![read(A)], "wanted undo back to the earlier song to ask for its file, its wave not read at launch, got {asked:?}");
    shell.settle();
    assert!(!shell.notice_shown(), "wanted no notice over the earlier song, whose kick read, but the next key press took one off the screen");
    assert_heard(&mut shell, LOW, "from the earlier song once its file was read");
}

#[test]
fn undo_back_to_a_song_after_a_relaunch_plays_and_saves_its_own_wave_though_the_open_song_embeds_one_of_that_name() {
    let mut shell = two_songs(&disk(low_kick_song(), high_kick_song())).relaunched();
    shell.settle();
    assert_heard(&mut shell, HIGH, "from the relaunched second song");
    shell.undo();
    shell.settle();
    assert_heard(&mut shell, LOW, "after a relaunch and undo back to the first song, whose kick it is");
    assert_saved_kick(&shell.save(), A, LOW, "after a relaunch and undo back to the first song");
}

#[test]
fn an_unsaved_songs_bounce_is_still_its_own_audio_after_a_file_of_its_name_is_dropped_into_a_new_project_and_undone() {
    let (mut shell, bounce) = bounced(&disk(low_kick_song(), high_kick_song()));
    shell.new_project();
    shell.drop_wave(BOUNCE_FILE);
    shell.settle();
    let placed = shell.clip_waves();
    assert_eq!(placed, vec![BOUNCE], "wanted the file dropped into the new project to keep its plain name, the new project holding no wave of the song before, got {placed:?}");
    shell.undo();
    shell.undo();
    shell.settle();
    let placed = shell.clip_waves();
    assert_eq!(placed, vec![BOUNCE], "wanted two undos back on the first song, its bounce the one audio clip, got {placed:?}");
    let save = shell.save();
    assert!(save.to.is_none(), "wanted the first song, never saved, to ask where to save, got a write to {:?}", save.to);
    let kept = bounce_in(&save.file());
    assert!(kept == bounce, "wanted the first song's bounce saved sample for sample as it was bounced, got {} samples sounding {:?} where the bounce held {}", kept.len(), tones_in(&kept), bounce.len());
}

#[test]
fn an_unsaved_songs_bounce_the_shell_kept_is_read_back_at_a_relaunch_and_saves_as_it_was_bounced() {
    let (mut shell, bounce) = bounced_stowing(&[], Some(true));
    let kept: Vec<&String> = shell.files.iter().map(|(path, _)| path).filter(|path| path.starts_with(STOWED)).collect();
    assert_eq!(kept.len(), 1, "wanted the bounce handed to the shell to keep as one file of its own, got {kept:?}");
    let mut shell = shell.relaunched();
    shell.settle();
    assert!(!shell.notice_shown(), "wanted no notice over the relaunched song, whose bounce the shell kept, but the next key press took one off the screen");
    let placed = shell.clip_waves();
    assert_eq!(placed, vec![BOUNCE], "wanted the relaunched song still holding its bounce as the one audio clip, got {placed:?}");
    let read_back = bounce_in(&shell.save().file());
    assert!(read_back == bounce, "wanted the bounce read back at the relaunch saved sample for sample as it was bounced, got {} samples sounding {:?} where the bounce held {}", read_back.len(), tones_in(&read_back), bounce.len());
}

#[test]
fn an_unsaved_songs_bounce_the_shell_could_not_keep_is_gone_at_a_relaunch_with_a_notice_and_keeps_its_name() {
    let (mut shell, _) = bounced_stowing(&[], Some(false));
    let mut shell = shell.relaunched();
    shell.settle();
    assert!(shell.notice_shown(), "wanted a notice over the relaunched song, whose bounce could not be kept and is gone, but no key press took one off the screen");
    bounce(&mut shell, synth_song().tracks.len() + 1);
    let placed = shell.clip_waves();
    assert_eq!(placed, vec![String::from(BOUNCE), format!("{BOUNCE} 2")], "wanted a second bounce to take a name of its own and leave the lost one's clip silent, got {placed:?}");
}

#[test]
fn an_unsaved_songs_bounce_is_still_its_own_audio_after_a_song_embedding_a_wave_of_its_name_is_opened_and_undone() {
    let (mut shell, bounce) = bounced(&disk(song(A_TRACK, BOUNCE, vec![embedded(BOUNCE, MID)]), high_kick_song()));
    shell.open(A);
    shell.undo();
    shell.settle();
    let placed = shell.clip_waves();
    assert_eq!(placed, vec![BOUNCE], "wanted undo back on the unsaved song, its bounce the one audio clip, got {placed:?}");
    let kept = bounce_in(&shell.save().file());
    assert!(kept == bounce, "wanted the unsaved song's bounce saved sample for sample as it was bounced, got {} samples sounding {:?} where the bounce held {}", kept.len(), tones_in(&kept), bounce.len());
}

#[test]
fn a_file_dropped_into_a_new_project_keeps_its_plain_name_though_the_song_before_holds_a_wave_of_that_name() {
    let mut shell = Shell::new(&disk(low_kick_song(), high_kick_song()));
    shell.drop_wave(OTHER_KICK_FILE);
    shell.settle();
    shell.new_project();
    shell.drop_wave(KICK_FILE);
    shell.settle();
    let placed = shell.clip_waves();
    assert_eq!(placed, vec![KICK], "wanted the kick dropped into the new project to keep its plain name, the new project holding no wave of the song before, got {placed:?}");
    assert_heard(&mut shell, HIGH, "from the new project, whose dropped file holds that tone");
    shell.undo();
    shell.undo();
    shell.settle();
    assert_heard(&mut shell, LOW, "after undo back to the first song, whose own dropped file holds that tone");
}

#[test]
fn a_drop_still_waiting_on_its_read_lands_nothing_once_a_saved_version_is_restored() {
    let mut shell = Shell::new(&disk(saved_twice(A, &low_kick_song()), high_kick_song()));
    shell.open(A);
    shell.drop_wave(SNARE_FILE);
    shell.press_settings_row(SECOND_ROW);
    let shown = shell.first_track();
    assert_eq!(shown, A_TRACK, "wanted the press on the second row of SAVES to restore the older version, its track named {A_TRACK}, got {shown}");
    shell.answer_waiting();
    shell.settle();
    let placed = shell.clip_waves();
    assert_eq!(placed, vec![KICK], "wanted the restored version left with its kick alone once the read of the drop made before it was answered, got the clips {placed:?}");
}

#[test]
fn a_wave_still_being_searched_for_is_found_after_a_saved_version_is_restored_whichever_answer_comes_first() {
    for reads_first in [false, true] {
        let mut shell = Shell::new(&disk(saved_twice(A, &lost_kick_song()), high_kick_song()));
        shell.browse(TAKES);
        shell.begin_open(A);
        shell.settle_reads();
        assert_eq!(shell.waiting, vec![scan(TAKES)], "wanted the browser's folder scanned once the song's linked kick failed to read, got {:?}", shell.waiting);
        shell.press_settings_row(SECOND_ROW);
        let shown = shell.first_track();
        assert_eq!(shown, A_TRACK, "wanted the press on the second row of SAVES to restore the older version, its track named {A_TRACK}, got {shown}");
        if reads_first {
            shell.settle_reads();
        }
        shell.settle();
        let when = format!("the scan asked before the restore answered {} the reads asked after it", if reads_first { "after" } else { "before" });
        assert!(!shell.notice_shown(), "wanted no missing notice over the restored version, whose kick lies under the browser's folder, {when}, but the next key press took one off the screen");
        assert_heard(&mut shell, HIGH, &format!("from the restored version, its kick found under the browser's folder, {when}"));
    }
}

#[test]
fn an_old_edit_of_another_song_pressed_in_settings_returns_to_that_songs_file_and_a_save_writes_it_with_its_own_wave() {
    let mut shell = another_songs_edit_pressed();
    assert_saved_kick(&shell.save(), A, LOW, "after an old edit of the first song was pressed over the second");
}

#[test]
fn an_old_edit_of_another_song_pressed_in_settings_plays_that_songs_wave() {
    let mut shell = another_songs_edit_pressed();
    assert_heard(&mut shell, LOW, "after an old edit of the first song, whose kick it is, was pressed over the second");
}

#[test]
fn an_old_edit_of_another_song_pressed_in_settings_brings_back_that_songs_saved_versions() {
    let mut shell = another_songs_edit_pressed();
    assert_keeps_versions(&shell.save(), "after an old edit of the first song was pressed over the second");
}

#[test]
fn an_unsaved_song_has_no_path_after_a_new_project_made_from_it_is_saved_and_undone() {
    let mut shell = Shell::new(&[]);
    shell.rename_track(FIRST);
    shell.new_project();
    shell.rename_track(NEXT);
    shell.save_to(SECOND);
    let save = shell.save();
    assert_eq!(save.to.as_deref(), Some(SECOND), "wanted the new project, once saved, to write to {SECOND}, got {:?}", save.to);
    shell.undo();
    shell.undo();
    let shown = shell.first_track();
    assert_eq!(shown, FIRST, "wanted two undos back on the first song, its track named {FIRST}, got {shown}");
    let save = shell.save();
    assert!(save.to.is_none(), "wanted the first song, never saved, to ask where to save, got a write to {:?}", save.to);
}

#[test]
fn a_song_keeps_its_path_through_undo_after_its_first_save_and_a_relaunch_at_once() {
    let mut shell = Shell::new(&[]);
    shell.rename_track(FIRST);
    shell.save_to(ONE);
    let mut shell = shell.relaunched();
    shell.settle();
    shell.undo();
    shell.settle();
    let save = shell.save();
    assert_eq!(save.to.as_deref(), Some(ONE), "wanted the song still on {ONE} after its first save, a relaunch at once and an undo, got {:?}", save.to);
}

#[test]
fn a_launch_into_the_default_project_leaves_nothing_to_undo() {
    for default in [None, Some(low_kick_song())] {
        let mut shell = Shell::new(&[]);
        shell.app.begin(String::from(DEFAULT), default.as_deref());
        shell.settle();
        let undone = shell.key(Key::Char('z'), false, true);
        assert!(!undone, "wanted nothing to undo on an app that has only begun its default project, {}, but an undo took it back a step", if default.is_some() { "read from its file" } else { "no file holding one" });
    }
}

#[test]
fn undo_back_to_the_first_of_two_opens_of_one_file_plays_and_saves_the_wave_that_open_held_and_redo_the_later_opens() {
    let rewritten = saved_again_as(A, &song(A_TRACK, KICK, vec![embedded(KICK, HIGH)]), A_LATER);
    let mut shell = Shell::new(&disk(saved_twice(A, &low_kick_song()), high_kick_song()));
    shell.open(A);
    shell.rename_track(A_EDITED);
    shell.put(A, &rewritten);
    shell.open(A);
    shell.undo();
    shell.settle();
    let shown = shell.first_track();
    assert_eq!(shown, A_EDITED, "wanted undo back on the first open of the file, its track named {A_EDITED}, got {shown}");
    assert_saved_kick(&shell.save(), A, LOW, "after undo back to the first open");
    assert_heard(&mut shell, LOW, "after undo back to the first open, whose kick it is");
    shell.redo();
    shell.settle();
    assert_saved_kick(&shell.save(), A, HIGH, "after redo to the later open");
    assert_heard(&mut shell, HIGH, "after redo to the later open, whose kick it is");
}

#[test]
fn undo_back_past_a_save_as_to_the_song_before_plays_and_saves_its_own_wave_and_redo_returns_to_the_new_file_with_its_versions() {
    let mut shell = two_songs(&disk(low_kick_song(), high_kick_song()));
    shell.save_as(COPY);
    shell.rename_track(B_EDITED);
    shell.undo();
    shell.undo();
    shell.settle();
    let shown = shell.first_track();
    assert_eq!(shown, A_TRACK, "wanted two undos back past the Save As on the first song, its track named {A_TRACK}, got {shown}");
    assert_heard(&mut shell, LOW, "after undo back past the Save As to the first song, whose kick it is");
    assert_saved_kick(&shell.save(), A, LOW, "after undo back past the Save As to the first song");
    shell.redo();
    shell.redo();
    shell.settle();
    let shown = shell.first_track();
    assert_eq!(shown, B_EDITED, "wanted two redos back on the second song, its track named {B_EDITED}, got {shown}");
    let save = shell.save();
    assert_saved_kick(&save, COPY, HIGH, "after redo to the second song, now on the file Save As gave it,");
    let kept = version_tracks(&save.file());
    assert_eq!(kept, vec![B_TRACK, B_EDITED], "wanted the save after redo to hold the version Save As gave the new file and the save itself, got the versions {kept:?}");
    assert_heard(&mut shell, HIGH, "after redo to the second song, whose kick it is");
}
