use bp::app::{App, Dropped, Event, Opened, Request};
use bp::browser::Kind;
use bp::draw::Tone;
use bp::fx;
use bp::json::{self, Json};
use bp::keyboard::Key;
use bp::project::{Clip, Preset, Project, TICKS_PER_BEAT, Track};
use std::f64::consts::TAU;
use support::{AUDIBLE, BLOCK, HEIGHT, SAMPLE_RATE, WIDTH, amplitude, effect, frames, painted, wav};

const FIRST: &str = "/holds/songs/first.json";
const NEXT: &str = "/holds/songs/next.json";
const KICK: &str = "/holds/takes/kick.wav";
const SNARE: &str = "/holds/desk/snare.wav";
const RIFF: &str = "/holds/desk/riff.mid";
const SLAP: &str = "/holds/desk/slap.json";
const LIBRARY: &str = "/holds/library";
const EXPORT: &str = "/holds/out/first.wav";
const KEYS: &str = "keys";
const DRUMS: &str = "drums";
const BASS: &str = "bass";
const LEAD: &str = "lead";
const KICK_CLIP: &str = "kick";
const SNARE_CLIP: &str = "snare";
const RIFF_CLIP: &str = "riff";
const BOUNCE: &str = "Master Bounce";
const AS_OPENED: &Layout = &[(KEYS, &[]), (DRUMS, &[KICK_CLIP])];
const FLATTENED: &Layout = &[(KEYS, &[]), (DRUMS, &[DRUMS])];
const BOUNCED: &Layout = &[(KEYS, &[]), (DRUMS, &[KICK_CLIP]), (BOUNCE, &[BOUNCE])];
const NEXT_OPENED: &Layout = &[(BASS, &[]), (LEAD, &[])];
const LOOP_BEATS: (f64, f64) = (0.0, 1.0);
const KICK_HZ: f64 = 300.0;
const SNARE_HZ: f64 = 700.0;
const TONE_SECONDS: f64 = 0.25;
const TONE_PEAK: f64 = 8_192.0;
const LISTENED: (usize, usize) = (2_400, 9_600);
const MAX_FRAMES: usize = 4_096;
const RIFF_BEAT: u8 = 96;
const RIFF_PITCH: u8 = 60;
const RIFF_VELOCITY: u8 = 100;
const NOTE_ON: u8 = 0x90;
const NOTE_OFF: u8 = 0x80;
const END_OF_TRACK: [u8; 3] = [0xFF, 0x2F, 0];
const TRACK_PITCH: i32 = 112;
const KEYS_LANE: (i32, i32) = (600, 105);
const DRUMS_TILE: (i32, i32) = (60, 66 + TRACK_PITCH);
const FLATTEN_CELL: (i32, i32) = (1_130, 26);
const BOUNCE_CELL: (i32, i32) = (1_130, 26);
const BAR_W: i32 = 400;
const TICK: f32 = 1.0 / 60.0;
const MOST_TICKS: usize = 1_000_000;
const MOST_ANSWERS: usize = 12;
const IDLE_KEY: Key = Key::Char('x');

type Layout = [(&'static str, &'static [&'static str])];

#[derive(Clone, Copy, PartialEq)]
enum Render {
    Flatten,
    Bounce,
    Export,
}

impl Render {
    fn named(self) -> &'static str {
        match self {
            Render::Flatten => "a flatten",
            Render::Bounce => "a bounce",
            Render::Export => "an export to a file",
        }
    }

    fn needs_a_loop(self) -> bool {
        self == Render::Bounce
    }
}

struct Shell {
    app: App,
    render: Render,
    waiting: Vec<String>,
    read: Vec<String>,
    handed: Vec<String>,
    session: Option<String>,
}

impl Shell {
    fn opened(render: Render) -> Shell {
        let mut app = App::new();
        app.draw(WIDTH, HEIGHT);
        app.take_requests();
        let mut shell = Shell { app, render, waiting: Vec::new(), read: Vec::new(), handed: Vec::new(), session: None };
        let opened = shell.open(FIRST, &first_song(render.needs_a_loop()));
        assert_eq!(opened, Opened::Project, "wanted {FIRST} to open as a song, got {opened:?}");
        shell
    }

    fn about_to(render: Render) -> Shell {
        let mut shell = Shell::opened(render);
        shell.settle();
        shell.press(DRUMS_TILE);
        shell
    }

    fn rendering(render: Render) -> Shell {
        let mut shell = Shell::about_to(render);
        shell.begin();
        shell
    }

    fn open(&mut self, path: &str, document: &str) -> Opened {
        self.app.opened(String::from(path), Some(document), None)
    }

    fn drop(&mut self, path: &str, kind: Kind) -> Dropped {
        self.app.dropped(String::from(path), kind, KEYS_LANE.0, KEYS_LANE.1)
    }

    fn press(&mut self, (x, y): (i32, i32)) {
        self.app.event(Event::Move { x, y });
        self.app.event(Event::Down { x, y, clicks: 1 });
        self.app.event(Event::Up { x, y });
    }

    fn collect(&mut self) {
        for request in self.app.take_requests() {
            match request {
                Request::Remember { json } => self.session = Some(json),
                Request::Read { path } => {
                    self.read.push(path.clone());
                    self.waiting.push(path);
                }
                Request::Exported { path, .. } => {
                    self.app.exported(path.clone(), true);
                    self.handed.push(path);
                }
                _ => {}
            }
        }
    }

    fn settle(&mut self) {
        for answered in 0.. {
            self.collect();
            if self.waiting.is_empty() {
                return;
            }
            assert!(answered < MOST_ANSWERS, "wanted the app to stop asking within {MOST_ANSWERS} answers, but after them it still waits on {:?}", self.waiting);
            let path = self.waiting.remove(0);
            let bytes = file(&path);
            self.app.read(path, bytes);
        }
    }

    fn begin(&mut self) {
        match self.render {
            Render::Flatten => self.press(FLATTEN_CELL),
            Render::Bounce => self.press(BOUNCE_CELL),
            Render::Export => self.app.export_to(String::from(EXPORT)),
        }
        assert!(self.bar_shown(), "wanted the bar of {} painted once it began", self.render.named());
    }

    fn bar_shown(&mut self) -> bool {
        painted(&mut self.app).iter().any(|fill| fill.tone == Tone::Ground && fill.rect.w == BAR_W && fill.rect.x == (WIDTH - BAR_W) / 2)
    }

    fn finish(&mut self) {
        for ticked in 0.. {
            if !self.bar_shown() {
                self.collect();
                return;
            }
            assert!(ticked < MOST_TICKS, "wanted {} landed within {MOST_TICKS} ticks, but its bar is still painted", self.render.named());
            self.app.tick(TICK);
        }
    }

    fn quit(&mut self) -> Json {
        self.app.event(Event::Quit);
        self.collect();
        json::parse(self.session.as_deref().expect("wanted the session remembered")).expect("wanted the remembered session to parse")
    }

    fn heard(&mut self, hz: f64) -> f64 {
        let mut renderer = self.app.audio_started(SAMPLE_RATE, MAX_FRAMES, BLOCK as u32);
        self.app.event(Event::Key { key: Key::Space, shift: false, command: false, repeat: false });
        let (mut left, mut right) = (vec![0.0; LISTENED.1], vec![0.0; LISTENED.1]);
        for (l, r) in left.chunks_mut(BLOCK).zip(right.chunks_mut(BLOCK)) {
            renderer.render(l, r);
        }
        amplitude(&left[LISTENED.0..], hz)
    }
}

fn first_song(looped: bool) -> String {
    let mut project = Project::new();
    project.tracks[0].name = String::from(KEYS);
    project.tracks[1] = Track { name: String::from(DRUMS), clips: vec![Clip::audio(0, TICKS_PER_BEAT, String::from(KICK_CLIP), String::from(KICK_CLIP))], ..Track::audio() };
    let mut document = project.to_json();
    let text = |value: &str| Json::String(String::from(value));
    if let Json::Object(fields) = &mut document {
        fields.push((String::from("waves"), Json::Array(vec![Json::Object(vec![(String::from("name"), text(KICK_CLIP)), (String::from("sourcePath"), text(KICK))])])));
        if looped {
            fields.push((String::from("loopStartBeat"), Json::Number(LOOP_BEATS.0)));
            fields.push((String::from("loopEndBeat"), Json::Number(LOOP_BEATS.1)));
        }
    }
    document.write()
}

fn next_song() -> String {
    let mut project = Project::new();
    project.tracks[0].name = String::from(BASS);
    project.tracks[1].name = String::from(LEAD);
    project.to_json().write()
}

fn slap() -> String {
    Preset::Row(effect(fx::Kind::Delay), Vec::new()).to_json().write()
}

fn tone(hz: f64) -> Vec<i16> {
    (0..frames(TONE_SECONDS)).map(|frame| (TONE_PEAK * (TAU * hz * frame as f64 / SAMPLE_RATE).sin()).round() as i16).collect()
}

fn riff() -> Vec<u8> {
    let header = [0, 1, u16::from(RIFF_BEAT)].map(u16::to_be_bytes).concat();
    let note = [0, NOTE_ON, RIFF_PITCH, RIFF_VELOCITY, RIFF_BEAT, NOTE_OFF, RIFF_PITCH, 0, 0];
    let track = [note.as_slice(), &END_OF_TRACK].concat();
    [b"MThd".as_slice(), &(header.len() as u32).to_be_bytes(), &header, b"MTrk", &(track.len() as u32).to_be_bytes(), &track].concat()
}

fn file(path: &str) -> Option<Vec<u8>> {
    match path {
        KICK => Some(wav(&tone(KICK_HZ))),
        SNARE => Some(wav(&tone(SNARE_HZ))),
        RIFF => Some(riff()),
        _ => None,
    }
}

fn project(session: &Json) -> Project {
    session.get("project").and_then(Project::from_json).expect("wanted the remembered session to hold the song")
}

fn song(session: &Json) -> (Option<String>, Vec<(String, Vec<String>)>) {
    let tracks = project(session).tracks.iter().map(|track| (track.name.clone(), track.clips.iter().map(|clip| clip.label.clone()).collect())).collect();
    (session.get("path").and_then(Json::as_str).map(String::from), tracks)
}

fn effects(session: &Json) -> Vec<(String, &'static str)> {
    project(session).tracks.iter().flat_map(|track| track.rack.iter().filter(|row| row.is_effect()).map(|row| (track.name.clone(), row.label()))).collect()
}

fn folders(session: &Json) -> Vec<&str> {
    session.get("folders").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(Json::as_str).collect()
}

fn placed(session: &Json, labels: &[&'static str]) -> Vec<&'static str> {
    let (_, tracks) = song(session);
    labels.iter().copied().filter(|label| tracks.iter().any(|(_, clips)| clips.iter().any(|clip| clip == label))).collect()
}

fn assert_remains(shell: &mut Shell, path: &str, layout: &Layout, after: &str) {
    let wanted: Vec<(String, Vec<String>)> = layout.iter().map(|(track, clips)| (String::from(*track), clips.iter().map(|clip| String::from(*clip)).collect())).collect();
    let (left, tracks) = song(&shell.quit());
    assert!(left.as_deref() == Some(path) && tracks == wanted, "wanted {path} to remain, holding {layout:?}, after {after}, got {left:?} holding {tracks:?}");
}

#[test]
fn a_song_opened_during_a_flatten_is_refused_and_the_flatten_lands_in_the_song_it_began_in() {
    let mut shell = Shell::rendering(Render::Flatten);
    let opened = shell.open(NEXT, &next_song());
    shell.finish();
    assert_remains(&mut shell, FIRST, FLATTENED, "a flatten a song was opened during");
    assert_eq!(opened, Opened::Refused, "wanted the song opened during the flatten refused, got {opened:?}");
}

#[test]
fn a_song_opened_during_a_bounce_is_refused_and_the_bounce_lands_in_the_song_it_began_in() {
    let mut shell = Shell::rendering(Render::Bounce);
    let opened = shell.open(NEXT, &next_song());
    shell.finish();
    assert_remains(&mut shell, FIRST, BOUNCED, "a bounce a song was opened during");
    assert_eq!(opened, Opened::Refused, "wanted the song opened during the bounce refused, got {opened:?}");
}

#[test]
fn a_song_opened_during_an_export_to_a_file_is_refused_quietly_and_the_file_is_handed_over() {
    let mut shell = Shell::rendering(Render::Export);
    let opened = shell.open(NEXT, &next_song());
    let noticed = shell.app.event(Event::Key { key: IDLE_KEY, shift: false, command: false, repeat: false });
    shell.finish();
    assert_remains(&mut shell, FIRST, AS_OPENED, "an export to a file a song was opened during");
    assert_eq!(opened, Opened::Refused, "wanted the song opened during the export refused, got {opened:?}");
    assert!(!noticed, "wanted the song refused quietly, as other input is ignored during an export, but the next key press took a notice off the screen");
    assert_eq!(shell.handed, [EXPORT], "wanted the export handed over at {EXPORT} all the same, got {:?}", shell.handed);
}

#[test]
fn a_preset_opened_during_a_bounce_is_refused_and_lands_on_no_track() {
    let mut shell = Shell::rendering(Render::Bounce);
    let opened = shell.open(SLAP, &slap());
    shell.finish();
    let effects = effects(&shell.quit());
    assert!(effects.is_empty(), "wanted no effect on any track, the delay preset opened during the bounce refused, got {effects:?}");
    assert_eq!(opened, Opened::Refused, "wanted the preset opened during the bounce refused, got {opened:?}");
}

#[test]
fn a_wave_dropped_during_a_flatten_is_not_placed_and_the_flatten_lands_on_the_track_it_was_pressed_for() {
    let mut shell = Shell::rendering(Render::Flatten);
    let dropped = shell.drop(SNARE, Kind::Wave);
    shell.settle();
    shell.finish();
    assert_remains(&mut shell, FIRST, FLATTENED, "a flatten a wave was dropped during");
    assert_eq!(dropped, Dropped::Read, "wanted the wave dropped during the flatten answered as one the shell keeps nothing of, got {dropped:?}");
    assert_eq!(shell.read, [KICK], "wanted nothing read but the song's own kick, the wave dropped during the flatten left alone, got {:?}", shell.read);
}

#[test]
fn a_midi_file_dropped_during_a_bounce_is_not_placed() {
    let mut shell = Shell::rendering(Render::Bounce);
    let dropped = shell.drop(RIFF, Kind::Midi);
    shell.settle();
    shell.finish();
    assert_remains(&mut shell, FIRST, BOUNCED, "a bounce a MIDI file was dropped during");
    assert_eq!(dropped, Dropped::Read, "wanted the MIDI file dropped during the bounce answered as one the shell keeps nothing of, got {dropped:?}");
    assert_eq!(shell.read, [KICK], "wanted nothing read but the song's own kick, the MIDI file dropped during the bounce left alone, got {:?}", shell.read);
}

#[test]
fn a_wave_dropped_before_an_export_lands_nowhere_once_its_read_is_answered_during_it() {
    let mut shell = Shell::about_to(Render::Export);
    shell.drop(SNARE, Kind::Wave);
    shell.begin();
    shell.settle();
    shell.finish();
    assert_eq!(shell.read, [KICK, SNARE], "wanted the wave dropped before the export asked for once and answered during it, got {:?}", shell.read);
    assert_remains(&mut shell, FIRST, AS_OPENED, "an export during which the read of a wave dropped before it was answered");
}

#[test]
fn a_midi_file_dropped_before_a_flatten_lands_nowhere_once_its_read_is_answered_during_it() {
    let mut shell = Shell::about_to(Render::Flatten);
    shell.drop(RIFF, Kind::Midi);
    shell.begin();
    shell.settle();
    shell.finish();
    assert_eq!(shell.read, [KICK, RIFF], "wanted the MIDI file dropped before the flatten asked for once and answered during it, got {:?}", shell.read);
    assert_remains(&mut shell, FIRST, FLATTENED, "a flatten during which the read of a MIDI file dropped before it was answered");
}

#[test]
fn a_folder_dropped_during_a_render_is_still_added_to_the_browser_and_a_song_dropped_is_still_handed_on() {
    let mut shell = Shell::rendering(Render::Export);
    let folder = shell.drop(LIBRARY, Kind::Folder);
    let document = shell.drop(NEXT, Kind::Document);
    shell.finish();
    let session = shell.quit();
    assert_eq!(folders(&session), [LIBRARY], "wanted the folder dropped during the export added to the browser, got {:?}", folders(&session));
    assert_eq!((folder, document), (Dropped::Kept, Dropped::Document), "wanted the folder kept and the song handed on for the open to refuse, as outside a render, got {:?}", (folder, document));
}

#[test]
fn a_wave_its_song_waits_for_still_loads_when_its_read_is_answered_during_a_render() {
    let mut shell = Shell::opened(Render::Export);
    shell.begin();
    shell.collect();
    assert_eq!(shell.waiting, [KICK], "wanted the song's kick still waited for as the export began, got {:?}", shell.waiting);
    shell.settle();
    shell.finish();
    let level = shell.heard(KICK_HZ);
    assert!(level > f64::from(AUDIBLE), "wanted the kick heard when the song plays, its read answered during the export, got a level of {level}");
}

#[test]
fn once_a_flatten_has_landed_a_dropped_wave_and_a_dropped_midi_file_are_placed_as_usual() {
    let mut shell = Shell::rendering(Render::Flatten);
    shell.finish();
    let wave = shell.drop(SNARE, Kind::Wave);
    shell.settle();
    let midi = shell.drop(RIFF, Kind::Midi);
    shell.settle();
    let session = shell.quit();
    assert_eq!(placed(&session, &[SNARE_CLIP, RIFF_CLIP]), [SNARE_CLIP, RIFF_CLIP], "wanted the wave and the MIDI file dropped after the flatten placed in the song, got {:?}", song(&session).1);
    assert_eq!((wave, midi), (Dropped::Kept, Dropped::Read), "wanted the wave kept and the MIDI file read as usual after the flatten, got {:?}", (wave, midi));
}

#[test]
fn once_a_flatten_has_landed_a_song_opens_as_usual() {
    let mut shell = Shell::rendering(Render::Flatten);
    shell.finish();
    let opened = shell.open(NEXT, &next_song());
    assert_eq!(opened, Opened::Project, "wanted the song opened after the flatten taken, got {opened:?}");
    assert_remains(&mut shell, NEXT, NEXT_OPENED, "it was opened once the flatten had landed");
}
