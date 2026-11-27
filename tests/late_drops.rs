use bp::app::{App, Event, Opened, Request};
use bp::browser::Kind;
use bp::grid::BEAT_W;
use bp::json;
use bp::keyboard::Key;
use bp::project::{Project, Track, beats};
use support::{HEIGHT, WIDTH, call, wav};

const KICK: &str = "/late/desk/kick.wav";
const RIFF: &str = "/late/desk/riff.mid";
const NEXT_SONG: &str = "/late/songs/next.json";
const DEFAULT_SONG: &str = "/late/default.json";
const MIDI: &str = "MIDI";
const AUDIO: &str = "AUDIO";
const KEYS_OVER_DRUMS: [(&str, &str); 2] = [("keys", MIDI), ("drums", AUDIO)];
const DRUMS_OVER_KEYS: [(&str, &str); 2] = [("drums", AUDIO), ("keys", MIDI)];
const NEXT: [(&str, &str); 3] = [("lead", MIDI), ("bass", MIDI), ("vocal", AUDIO)];
const KEYS_DRUMS_VOCAL: [(&str, &str); 3] = [("keys", MIDI), ("drums", AUDIO), ("vocal", AUDIO)];
const DRUMS_KEYS_LEAD: [(&str, &str); 3] = [("drums", AUDIO), ("keys", MIDI), ("lead", MIDI)];
const KEYS_BASS_LEAD: [(&str, &str); 3] = [("keys", MIDI), ("bass", MIDI), ("lead", MIDI)];
const UPPER: usize = 0;
const LOWER: usize = 1;
const MIDDLE: usize = 1;
const BEAT: f64 = 8.0;
const RAIL: i32 = 45;
const TILE: i32 = 120;
const BORDER: i32 = 8;
const PITCH: i32 = TILE - BORDER;
const VOL_W: i32 = 28;
const COLUMN: i32 = PITCH + VOL_W + 2 * BORDER;
const REMOVE: (i32, i32) = (20, 100);
const FILE: (i32, i32) = (TILE / 2, RAIL / 2);
const MENU_TOP: i32 = RAIL + BORDER;
const MENU_INSET: i32 = 12;
const ITEM_H: i32 = 32;
const NEW_PROJECT: i32 = 0;
const NEW_DEFAULT_PROJECT: i32 = 1;
const KICK_FRAMES: usize = 4_800;
const KICK_PEAK: i16 = 8_192;
const FORMAT_SINGLE: u16 = 0;
const PPQ: u8 = 96;
const NOTE_ON: u8 = 0x90;
const NOTE_OFF: u8 = 0x80;
const META: u8 = 0xFF;
const END_OF_TRACK: u8 = 0x2F;
const C4: u8 = 60;
const LOUD: u8 = 100;

type Laid<'a> = Vec<(&'a str, &'a str, Vec<(f64, &'a str)>)>;

fn song(tracks: &[(&str, &str)]) -> Project {
    let mut project = Project::new();
    project.tracks.clear();
    for (at, (name, kind)) in tracks.iter().enumerate() {
        let track = if *kind == AUDIO { Track::audio() } else { Track::blank() };
        project.add_track(at, Track { name: String::from(*name), ..track });
    }
    project
}

fn laid(project: &Project) -> Laid<'_> {
    project.tracks.iter().map(|track| (track.name.as_str(), track.kind(), track.clips.iter().map(|clip| (beats(clip.start), clip.label.as_str())).collect())).collect()
}

fn chunk(tag: &[u8], body: &[u8]) -> Vec<u8> {
    let mut out = tag.to_vec();
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(body);
    out
}

fn riff() -> Vec<u8> {
    let header: Vec<u8> = [FORMAT_SINGLE, 1, u16::from(PPQ)].iter().flat_map(|field| field.to_be_bytes()).collect();
    let mut out = chunk(b"MThd", &header);
    out.extend(chunk(b"MTrk", &[0, NOTE_ON, C4, LOUD, PPQ, NOTE_OFF, C4, 0, 0, META, END_OF_TRACK, 0]));
    out
}

fn on_disk(path: &str) -> Vec<u8> {
    if path == RIFF { riff() } else { wav(&[KICK_PEAK; KICK_FRAMES]) }
}

struct Shell {
    app: App,
    waiting: Vec<String>,
    session: Option<String>,
}

impl Shell {
    fn resumed(project: &Project) -> Shell {
        let mut shell = Shell { app: App::new(), waiting: Vec::new(), session: None };
        shell.resume(project);
        shell.app.draw(WIDTH, HEIGHT);
        shell
    }

    fn resume(&mut self, project: &Project) {
        let session = format!("{{\"project\":{}}}", project.to_json().write());
        assert!(self.app.resume(&session), "wanted the session to resume");
    }

    fn asked(&mut self) -> Vec<Request> {
        let mut asked = Vec::new();
        for request in self.app.take_requests() {
            match request {
                Request::Remember { json } => self.session = Some(json),
                Request::Read { path } => self.waiting.push(path),
                other => asked.push(other),
            }
        }
        asked
    }

    fn drop_on(&mut self, slot: usize, path: &str, kind: Kind) {
        self.app.dropped(String::from(path), kind, COLUMN + (BEAT * BEAT_W) as i32, RAIL + slot as i32 * PITCH + TILE / 2);
        self.asked();
        assert!(self.waiting.iter().any(|waiting| waiting == path), "wanted the shell asked to read {path} once it was dropped, got {:?}", self.waiting);
    }

    fn drop_both(&mut self) {
        self.drop_on(UPPER, RIFF, Kind::Midi);
        self.drop_on(LOWER, KICK, Kind::Wave);
    }

    fn answer(&mut self) {
        for path in std::mem::take(&mut self.waiting) {
            let bytes = on_disk(&path);
            self.app.read(path, Some(bytes));
        }
    }

    fn press(&mut self, (x, y): (i32, i32)) {
        self.app.event(Event::Move { x, y });
        self.app.event(Event::Down { x, y, clicks: 1 });
        self.app.event(Event::Up { x, y });
    }

    fn choose(&mut self, item: i32) -> Vec<Request> {
        self.press(FILE);
        self.press((MENU_INSET, MENU_TOP + item * ITEM_H + ITEM_H / 2));
        self.asked()
    }

    fn open(&mut self, path: &str, project: &Project) {
        let opened = self.app.opened(String::from(path), Some(&project.to_json().write()), None);
        assert_eq!(opened, Opened::Project, "wanted {path} to open as a song, got {opened:?}");
    }

    fn new_default_project(&mut self, default: &Project) {
        let asked = self.choose(NEW_DEFAULT_PROJECT);
        assert!(asked.contains(&Request::Fresh), "wanted the shell asked for the default project, got {asked:?}");
        self.app.begin(String::from(DEFAULT_SONG), Some(&default.to_json().write()));
    }

    fn remove_track(&mut self, slot: usize) {
        self.press((REMOVE.0, RAIL + slot as i32 * PITCH + REMOVE.1));
    }

    fn turn(&mut self, slot: usize, kind: &str) {
        let reply = call(&mut self.app, "write", &format!(r#"{{"path":"tracks/{slot}/kind","value":"{}"}}"#, kind.to_lowercase()));
        assert!(!reply.failed, "wanted track {slot} turned into {kind}, got {}", reply.text);
    }

    fn undo(&mut self) {
        self.app.event(Event::Key { key: Key::Char('z'), shift: false, command: true, repeat: false });
    }

    fn redo(&mut self) {
        self.app.event(Event::Key { key: Key::Char('z'), shift: true, command: true, repeat: false });
    }

    fn remembered(&mut self) -> Project {
        self.app.event(Event::Quit);
        self.asked();
        let session = json::parse(self.session.as_deref().expect("wanted the session remembered on quitting")).expect("wanted the remembered session to parse");
        Project::from_json(session.get("project").expect("wanted the remembered session to hold the project")).expect("wanted the remembered project to read")
    }
}

fn assert_laid(shell: &mut Shell, wanted: Laid, what: &str) {
    let song = shell.remembered();
    let got = laid(&song);
    assert_eq!(got, wanted, "wanted {what}, got {got:?}");
}

fn assert_nothing_lands(shell: &mut Shell, wanted: &Project, song: &str) {
    shell.answer();
    assert_laid(shell, laid(wanted), &format!("nothing landed in {song}, the drops' reads answered only after it came"));
}

#[test]
fn a_drop_answered_in_the_usual_order_lands_where_it_was_dropped() {
    let mut shell = Shell::resumed(&song(&KEYS_OVER_DRUMS));
    shell.drop_both();
    shell.answer();
    assert_laid(&mut shell, vec![("keys", MIDI, vec![(BEAT, "riff")]), ("drums", AUDIO, vec![(BEAT, "kick")])], "the MIDI file on the MIDI track and the wave on the audio track, each at the beat it was dropped on");
}

#[test]
fn a_drop_waiting_on_its_read_lands_nowhere_in_a_song_opened_meanwhile() {
    let mut shell = Shell::resumed(&song(&KEYS_OVER_DRUMS));
    shell.drop_both();
    shell.open(NEXT_SONG, &song(&NEXT));
    assert_nothing_lands(&mut shell, &song(&NEXT), "the opened song");
}

#[test]
fn a_drop_waiting_on_its_read_lands_nowhere_in_a_new_project_begun_meanwhile() {
    let mut shell = Shell::resumed(&song(&KEYS_OVER_DRUMS));
    shell.drop_both();
    shell.choose(NEW_PROJECT);
    assert_nothing_lands(&mut shell, &Project::new(), "the new project");
}

#[test]
fn a_drop_waiting_on_its_read_lands_nowhere_in_a_default_project_begun_meanwhile() {
    let mut shell = Shell::resumed(&song(&KEYS_OVER_DRUMS));
    shell.drop_both();
    shell.new_default_project(&song(&NEXT));
    assert_nothing_lands(&mut shell, &song(&NEXT), "the default project");
}

#[test]
fn a_drop_waiting_on_its_read_lands_nowhere_in_a_session_resumed_meanwhile() {
    let mut shell = Shell::resumed(&song(&KEYS_OVER_DRUMS));
    shell.drop_both();
    shell.resume(&song(&NEXT));
    assert_nothing_lands(&mut shell, &song(&NEXT), "the resumed session");
}

#[test]
fn a_drop_waiting_on_its_read_lands_nowhere_in_the_song_an_undo_goes_back_to_across_a_load() {
    let mut shell = Shell::resumed(&song(&KEYS_OVER_DRUMS));
    shell.open(NEXT_SONG, &song(&NEXT));
    shell.drop_both();
    shell.undo();
    assert_nothing_lands(&mut shell, &song(&KEYS_OVER_DRUMS), "the song the undo went back to");
}

#[test]
fn a_drop_waiting_on_its_read_lands_nowhere_in_the_song_a_redo_returns_to_across_a_load() {
    let mut shell = Shell::resumed(&song(&KEYS_OVER_DRUMS));
    shell.open(NEXT_SONG, &song(&NEXT));
    shell.undo();
    shell.drop_both();
    shell.redo();
    assert_nothing_lands(&mut shell, &song(&NEXT), "the song the redo returned to");
}

#[test]
fn an_undo_inside_the_same_song_leaves_a_waiting_drop_to_land() {
    let mut shell = Shell::resumed(&song(&KEYS_OVER_DRUMS));
    shell.drop_on(UPPER, RIFF, Kind::Midi);
    shell.answer();
    shell.drop_on(LOWER, KICK, Kind::Wave);
    shell.undo();
    shell.answer();
    assert_laid(&mut shell, vec![("keys", MIDI, vec![]), ("drums", AUDIO, vec![(BEAT, "kick")])], "the MIDI clip undone and the wave, read after the undo, on the audio track it was dropped on");
}

#[test]
fn a_wave_whose_track_was_removed_before_its_read_lands_on_a_new_track_at_the_bottom_at_its_beat() {
    let mut shell = Shell::resumed(&song(&KEYS_OVER_DRUMS));
    shell.drop_on(LOWER, KICK, Kind::Wave);
    shell.remove_track(LOWER);
    shell.answer();
    assert_laid(&mut shell, vec![("keys", MIDI, vec![]), ("AUDIO 1", AUDIO, vec![(BEAT, "kick")])], "a new audio track under the one left, holding the wave at the beat it was dropped on");
}

#[test]
fn a_midi_file_whose_track_was_removed_before_its_read_lands_on_a_new_track_at_the_bottom_at_its_beat() {
    let mut shell = Shell::resumed(&song(&DRUMS_OVER_KEYS));
    shell.drop_on(LOWER, RIFF, Kind::Midi);
    shell.remove_track(LOWER);
    shell.answer();
    assert_laid(&mut shell, vec![("drums", AUDIO, vec![]), ("MIDI 1", MIDI, vec![(BEAT, "riff")])], "a new MIDI track under the one left, holding the clip at the beat it was dropped on");
}

#[test]
fn a_wave_whose_track_was_turned_into_a_midi_track_before_its_read_lands_on_a_new_track_at_the_bottom_at_its_beat() {
    let mut shell = Shell::resumed(&song(&DRUMS_OVER_KEYS));
    shell.drop_on(UPPER, KICK, Kind::Wave);
    shell.turn(UPPER, MIDI);
    shell.answer();
    assert_laid(&mut shell, vec![("drums", MIDI, vec![]), ("keys", MIDI, vec![]), ("AUDIO 1", AUDIO, vec![(BEAT, "kick")])], "a new audio track under both, holding the wave at the beat it was dropped on, and the track turned to MIDI left empty");
}

#[test]
fn a_midi_file_whose_track_was_turned_into_an_audio_track_before_its_read_lands_on_a_new_track_at_the_bottom_at_its_beat() {
    let mut shell = Shell::resumed(&song(&KEYS_OVER_DRUMS));
    shell.drop_on(UPPER, RIFF, Kind::Midi);
    shell.turn(UPPER, AUDIO);
    shell.answer();
    assert_laid(&mut shell, vec![("keys", AUDIO, vec![]), ("drums", AUDIO, vec![]), ("MIDI 1", MIDI, vec![(BEAT, "riff")])], "a new MIDI track under both, holding the clip at the beat it was dropped on, and the track turned to audio left empty");
}

#[test]
fn a_wave_whose_track_moved_up_when_the_one_above_was_removed_lands_on_its_own_track() {
    let mut shell = Shell::resumed(&song(&KEYS_DRUMS_VOCAL));
    shell.drop_on(MIDDLE, KICK, Kind::Wave);
    shell.remove_track(UPPER);
    shell.answer();
    assert_laid(&mut shell, vec![("drums", AUDIO, vec![(BEAT, "kick")]), ("vocal", AUDIO, vec![])], "the wave on the audio track it was dropped on, now one slot up, and the audio track under it left empty");
}

#[test]
fn a_midi_file_whose_track_moved_up_when_the_one_above_was_removed_lands_on_its_own_track() {
    let mut shell = Shell::resumed(&song(&DRUMS_KEYS_LEAD));
    shell.drop_on(MIDDLE, RIFF, Kind::Midi);
    shell.remove_track(UPPER);
    shell.answer();
    assert_laid(&mut shell, vec![("keys", MIDI, vec![(BEAT, "riff")]), ("lead", MIDI, vec![])], "the clip on the MIDI track it was dropped on, now one slot up, and the MIDI track under it left empty");
}

#[test]
fn a_wave_dropped_on_a_midi_track_still_opens_its_track_under_that_one_when_a_track_above_was_removed() {
    let mut shell = Shell::resumed(&song(&KEYS_BASS_LEAD));
    shell.drop_on(MIDDLE, KICK, Kind::Wave);
    shell.remove_track(UPPER);
    shell.answer();
    assert_laid(&mut shell, vec![("bass", MIDI, vec![]), ("AUDIO 1", AUDIO, vec![(BEAT, "kick")]), ("lead", MIDI, vec![])], "a new audio track right under the MIDI track the wave was dropped on, holding it at its beat");
}
