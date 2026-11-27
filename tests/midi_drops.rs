use bp::app::{App, Dropped, Event, Request};
use bp::arp::Articulation;
use bp::browser::{Found, Item, Kind, kind};
use bp::draw::Rect;
use bp::face::{CELL, PITCH};
use bp::json::{self, Json};
use bp::project::{Note, Project, TICKS_PER_BAR, TICKS_PER_BEAT, Track};
use support::{HEIGHT, WIDTH, beat_x, browser_rect, lane_rows, painted};

const ROOT: &str = "/x/Riffs";
const IMAGINARY: &str = "/x/Riffs/Imaginary.mid";
const AUDIO_TRACK: usize = 2;
const PPQ: u16 = 96;
const FINE_PPQ: u16 = 480;
const SMPTE_25_FPS_40_TICKS: u16 = 0xE728;
const FORMAT_SINGLE: u16 = 0;
const FORMAT_PARALLEL: u16 = 1;
const NOTE_ON: u8 = 0x90;
const NOTE_OFF: u8 = 0x80;
const META: u8 = 0xFF;
const TRACK_NAME: u8 = 0x03;
const TEMPO: u8 = 0x51;
const TIME_SIGNATURE: u8 = 0x58;
const END_OF_TRACK: u8 = 0x2F;
const LOUD: u8 = 100;
const SOFT: u8 = 90;
const C4: u8 = 60;
const D4: u8 = 62;
const E4: u8 = 64;
const C2: u8 = 36;
const ONE_BAR: i32 = TICKS_PER_BAR;
const LOOPS: [(u32, i32); 3] = [(8, 8 * TICKS_PER_BEAT), (6, 6 * TICKS_PER_BEAT), (3, ONE_BAR)];

fn quantity(value: u32) -> Vec<u8> {
    let mut out = vec![(value & 0x7F) as u8];
    let mut rest = value >> 7;
    while rest > 0 {
        out.push(0x80 | (rest & 0x7F) as u8);
        rest >>= 7;
    }
    out.reverse();
    out
}

fn chunk(tag: &[u8], body: &[u8]) -> Vec<u8> {
    let mut out = tag.to_vec();
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(body);
    out
}

fn meta(kind: u8, data: &[u8]) -> Vec<u8> {
    let mut out = vec![META, kind];
    out.extend(quantity(data.len() as u32));
    out.extend_from_slice(data);
    out
}

fn named(text: &str) -> Vec<u8> {
    meta(TRACK_NAME, text.as_bytes())
}

fn tempo(micros_per_beat: u32) -> Vec<u8> {
    meta(TEMPO, &micros_per_beat.to_be_bytes()[1..])
}

fn four_four() -> Vec<u8> {
    meta(TIME_SIGNATURE, &[4, 2, 24, 8])
}

fn end() -> Vec<u8> {
    meta(END_OF_TRACK, &[])
}

fn on(channel: u8, pitch: u8, velocity: u8) -> Vec<u8> {
    vec![NOTE_ON | channel, pitch, velocity]
}

fn off(channel: u8, pitch: u8) -> Vec<u8> {
    vec![NOTE_OFF | channel, pitch, 0]
}

fn running(pitch: u8, velocity: u8) -> Vec<u8> {
    vec![pitch, velocity]
}

fn track(events: Vec<(u32, Vec<u8>)>) -> Vec<u8> {
    let mut body = Vec::new();
    for (delta, event) in events {
        body.extend(quantity(delta));
        body.extend(event);
    }
    chunk(b"MTrk", &body)
}

fn file(format: u16, division: u16, tracks: &[Vec<u8>]) -> Vec<u8> {
    let mut header = Vec::new();
    header.extend_from_slice(&format.to_be_bytes());
    header.extend_from_slice(&(tracks.len() as u16).to_be_bytes());
    header.extend_from_slice(&division.to_be_bytes());
    let mut out = chunk(b"MThd", &header);
    for track in tracks {
        out.extend_from_slice(track);
    }
    out
}

fn lead() -> Vec<u8> {
    let track = track(vec![(0, named("Lead")), (0, tempo(500_000)), (0, four_four()), (0, on(0, C4, 127)), (48, off(0, C4)), (48, on(0, E4, LOUD)), (96, off(0, E4)), (0, end())]);
    file(FORMAT_SINGLE, PPQ, &[track])
}

fn lead_notes() -> Vec<Note> {
    vec![Note { pitch: C4, start: 0.0, length: 0.5, velocity: 127, articulation: Articulation::PLAIN }, Note { pitch: E4, start: 1.0, length: 1.0, velocity: LOUD, articulation: Articulation::PLAIN }]
}

fn bass_and_keys() -> Vec<u8> {
    let song = track(vec![(0, named("Song")), (0, tempo(400_000)), (0, end())]);
    let bass = track(vec![(0, named("Bass")), (0, on(0, C2, SOFT)), (480, off(0, C2)), (0, end())]);
    let keys = track(vec![(0, named("Keys")), (960, on(1, C4, LOUD)), (0, on(1, E4, LOUD)), (480, off(1, C4)), (0, off(1, E4)), (0, end())]);
    file(FORMAT_PARALLEL, FINE_PPQ, &[song, bass, keys])
}

fn open(project: &Project, browsing: bool) -> App {
    let folders = if browsing { format!(",\"aside\":\"browser\",\"folders\":[\"{ROOT}\"],\"unfolded\":[\"{ROOT}\"]") } else { String::new() };
    let session = format!("{{\"project\":{},\"keyboard\":false{folders}}}", project.to_json().write());
    let mut app = App::new();
    app.resume(&session);
    app.take_requests();
    app
}

fn serve(app: &mut App, bytes: Option<&[u8]>) -> Option<Json> {
    let mut remembered = None;
    loop {
        let requests = app.take_requests();
        if requests.is_empty() {
            break;
        }
        for request in requests {
            match request {
                Request::Read { path } => app.read(path, bytes.map(<[u8]>::to_vec)),
                Request::Remember { json } => remembered = Some(json),
                _ => {}
            }
        }
    }
    Some(json::parse(&remembered?).expect("wanted the remembered session to parse"))
}

fn project_of(session: &Json) -> Project {
    Project::from_json(session.get("project").expect("wanted the remembered session to hold the project")).expect("wanted the remembered project to read")
}

fn landed(app: &mut App, bytes: &[u8]) -> Project {
    project_of(&serve(app, Some(bytes)).expect("wanted the drop remembered"))
}

fn track_rows(app: &mut App, count: usize) -> Vec<Rect> {
    let rows = lane_rows(&painted(app));
    assert_eq!(rows.len(), count, "wanted {count} track lanes painted, got {}", rows.len());
    rows
}

fn middle(row: Rect) -> i32 {
    row.y + row.h / 2
}

fn kinds(project: &Project) -> Vec<(&str, bool, usize)> {
    project.tracks.iter().map(|track| (track.name.as_str(), track.audio, track.clips.len())).collect()
}

fn placed(project: &Project, track: usize) -> Vec<(i32, i32, &str, Vec<Note>)> {
    project.tracks[track].clips.iter().map(|clip| (clip.start, clip.length, clip.label.as_str(), clip.notes.clone())).collect()
}

fn name_cell(browser: Rect, row: i32) -> Rect {
    let mark = Rect::new(browser.x + PITCH, browser.y + row * PITCH, CELL, CELL);
    Rect::new(mark.x + PITCH, mark.y, browser.x + browser.w - mark.x - PITCH, CELL)
}

fn drag(app: &mut App, from: Rect, to: (i32, i32)) {
    let (x, y) = (from.x + from.w / 2, from.y + from.h / 2);
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Move { x: to.0, y: to.1 });
    app.event(Event::Up { x: to.0, y: to.1 });
}

fn double_press(app: &mut App, rect: Rect) {
    let (x, y) = (rect.x + rect.w / 2, rect.y + rect.h / 2);
    app.event(Event::Move { x, y });
    for clicks in 1..=2 {
        app.event(Event::Down { x, y, clicks });
        app.event(Event::Up { x, y });
    }
}

#[test]
fn a_midi_file_is_known_by_its_extension_in_any_case() {
    for path in ["/x/riff.mid", "/x/RIFF.MID", "/x/riff.midi", "/x/Riff.Midi"] {
        assert_eq!(kind(path, false), Some(Kind::Midi), "wanted {path} sorted as a MIDI file");
    }
    for path in ["/x/riff.mid.bak", "/x/mid", "/x/.riff.mid"] {
        assert_eq!(kind(path, false), None, "wanted {path} passed over");
    }
}

#[test]
fn a_midi_file_dropped_on_a_midi_lane_lands_its_notes_as_a_clip_at_the_beat_under_the_pointer() {
    let mut app = open(&Project::new(), false);
    let rows = track_rows(&mut app, 2);
    let taken = app.dropped(String::from(IMAGINARY), Kind::Midi, beat_x(&rows, 8), middle(rows[1]));
    assert_eq!(taken, Dropped::Read, "wanted the file read once and not bookmarked");
    let project = landed(&mut app, &lead());
    assert_eq!(kinds(&project), vec![("MIDI 1", false, 0), ("MIDI 2", false, 1)], "wanted the clip on MIDI 2 alone, got {:?}", kinds(&project));
    assert_eq!(placed(&project, 1), vec![(8 * TICKS_PER_BEAT, ONE_BAR, "Imaginary", lead_notes())], "wanted a one-bar clip at beat 8 named for the file holding both notes, got {:?}", placed(&project, 1));
}

#[test]
fn a_midi_file_dropped_on_an_audio_lane_makes_a_midi_track_per_part_after_it_and_off_the_lanes_at_the_end() {
    let mut project = Project::new();
    project.add_track(AUDIO_TRACK, Track::audio());
    let mut app = open(&project, false);
    let rows = track_rows(&mut app, 3);
    app.dropped(String::from(IMAGINARY), Kind::Midi, beat_x(&rows, 4), middle(rows[AUDIO_TRACK]));
    let session = serve(&mut app, Some(&bass_and_keys())).expect("wanted the drop remembered");
    let project = project_of(&session);
    assert_eq!(kinds(&project), vec![("MIDI 1", false, 0), ("MIDI 2", false, 0), ("AUDIO 3", true, 0), ("MIDI 4", false, 1), ("MIDI 5", false, 1)], "wanted a MIDI track for the bass and one for the keys after the audio track, the song's empty track skipped, got {:?}", kinds(&project));
    assert_eq!(placed(&project, 3), vec![(4 * TICKS_PER_BEAT, ONE_BAR, "Bass", vec![Note { pitch: C2, start: 0.0, length: 1.0, velocity: SOFT, articulation: Articulation::PLAIN }])], "wanted the bass clip at beat 4 named for its track, got {:?}", placed(&project, 3));
    assert_eq!(placed(&project, 4), vec![(4 * TICKS_PER_BEAT, ONE_BAR, "Keys", vec![Note { pitch: C4, start: 2.0, length: 1.0, velocity: LOUD, articulation: Articulation::PLAIN }, Note { pitch: E4, start: 2.0, length: 1.0, velocity: LOUD, articulation: Articulation::PLAIN }])], "wanted the keys clip at beat 4 with its chord two beats in, got {:?}", placed(&project, 4));
    assert_eq!(session.get("selected").and_then(Json::as_i64), Some(3), "wanted the first new track selected");
    app.dropped(String::from(IMAGINARY), Kind::Midi, WIDTH - 20, HEIGHT - 20);
    let project = landed(&mut app, &bass_and_keys());
    assert_eq!(kinds(&project), vec![("MIDI 1", false, 0), ("MIDI 2", false, 0), ("AUDIO 3", true, 0), ("MIDI 4", false, 1), ("MIDI 5", false, 1), ("MIDI 6", false, 1), ("MIDI 7", false, 1)], "wanted two more tracks at the end for a drop off the lanes, got {:?}", kinds(&project));
    let starts: Vec<i32> = [5, 6].iter().flat_map(|&track| project.tracks[track].clips.iter().map(|clip| clip.start)).collect();
    assert_eq!(starts, vec![0, 0], "wanted the clips off the lanes at the playhead, got {starts:?}");
}

#[test]
fn a_dropped_midi_file_runs_to_its_end_past_the_bar_its_last_note_ends_in_and_to_that_bar_short_of_it() {
    let ticks_per_beat = u32::from(PPQ);
    for (beats, wanted) in LOOPS {
        let mut app = open(&Project::new(), false);
        let rows = track_rows(&mut app, 2);
        app.dropped(String::from(IMAGINARY), Kind::Midi, beat_x(&rows, 0), middle(rows[0]));
        let ending = track(vec![(0, on(0, C4, LOUD)), (ticks_per_beat / 2, off(0, C4)), (beats * ticks_per_beat - ticks_per_beat / 2, end())]);
        let project = landed(&mut app, &file(FORMAT_SINGLE, PPQ, &[ending]));
        let lengths: Vec<i32> = project.tracks[0].clips.iter().map(|clip| clip.length).collect();
        assert_eq!(lengths, [wanted], "wanted a file ending {beats} beats in, its one note in the first beat, to land as one clip of {} beats", wanted / TICKS_PER_BEAT);
    }
}

#[test]
fn running_status_silent_note_ons_restruck_pitches_and_channels_read_as_a_player_would() {
    let events = vec![
        (0, on(0, C4, LOUD)),
        (0, on(1, E4, SOFT)),
        (24, on(0, D4, LOUD)),
        (24, running(C4, 0)),
        (0, running(C4, LOUD)),
        (0, on(2, E4, SOFT)),
        (24, on(0, C4, LOUD)),
        (24, running(D4, 0)),
        (0, off(1, E4)),
        (0, off(2, E4)),
        (48, end()),
    ];
    let mut app = open(&Project::new(), false);
    let rows = track_rows(&mut app, 2);
    app.dropped(String::from(IMAGINARY), Kind::Midi, beat_x(&rows, 0), middle(rows[0]));
    let project = landed(&mut app, &file(FORMAT_SINGLE, PPQ, &[track(events)]));
    let wanted = vec![
        Note { pitch: C4, start: 0.0, length: 0.5, velocity: LOUD, articulation: Articulation::PLAIN },
        Note { pitch: E4, start: 0.0, length: 0.5, velocity: SOFT, articulation: Articulation::PLAIN },
        Note { pitch: D4, start: 0.25, length: 0.75, velocity: LOUD, articulation: Articulation::PLAIN },
        Note { pitch: C4, start: 0.5, length: 0.25, velocity: LOUD, articulation: Articulation::PLAIN },
        Note { pitch: E4, start: 0.5, length: 0.5, velocity: SOFT, articulation: Articulation::PLAIN },
        Note { pitch: C4, start: 0.75, length: 0.75, velocity: LOUD, articulation: Articulation::PLAIN },
    ];
    assert_eq!(placed(&project, 0), vec![(0, ONE_BAR, "Imaginary", wanted)], "wanted the silent note-on to end C4, the restrike to cut it, the two E4s to share the pitch and the last C4 to end with the track, got {:?}", placed(&project, 0));
}

#[test]
fn smpte_timing_is_read_through_the_projects_tempo() {
    let mut app = open(&Project::new(), false);
    let rows = track_rows(&mut app, 2);
    app.dropped(String::from(IMAGINARY), Kind::Midi, beat_x(&rows, 0), middle(rows[0]));
    let project = landed(&mut app, &file(FORMAT_SINGLE, SMPTE_25_FPS_40_TICKS, &[track(vec![(500, on(0, C4, LOUD)), (250, off(0, C4)), (0, end())])]));
    assert_eq!(placed(&project, 0), vec![(0, ONE_BAR, "Imaginary", vec![Note { pitch: C4, start: 1.0, length: 0.5, velocity: LOUD, articulation: Articulation::PLAIN }])], "wanted a thousand ticks a second to put the half-second note a beat in at 120 BPM, got {:?}", placed(&project, 0));
}

#[test]
fn a_file_without_notes_an_unreadable_one_and_a_file_that_is_not_midi_place_nothing() {
    let mut app = open(&Project::new(), false);
    let rows = track_rows(&mut app, 2);
    let silent = file(FORMAT_SINGLE, PPQ, &[track(vec![(0, named("Hush")), (0, end())])]);
    for bytes in [Some(silent.as_slice()), None, Some(b"RIFF....WAVE".as_slice()), Some(&lead()[..lead().len() / 2])] {
        app.dropped(String::from(IMAGINARY), Kind::Midi, beat_x(&rows, 0), middle(rows[0]));
        let session = serve(&mut app, bytes);
        assert!(session.as_ref().is_none_or(|session| kinds(&project_of(session)) == kinds(&Project::new())), "wanted nothing placed for {bytes:?}, got {:?}", session.map(|session| kinds(&project_of(&session)).into_iter().map(|(name, audio, clips)| (name.to_string(), audio, clips)).collect::<Vec<(String, bool, usize)>>()));
    }
}

fn browsing() -> App {
    let mut app = open(&Project::new(), true);
    app.listed(ROOT, vec![Found { path: String::from(IMAGINARY), name: String::from("Imaginary.mid"), item: Item::Midi(u64::MAX) }]);
    app.take_requests();
    app
}

#[test]
fn a_midi_file_in_the_browser_dragged_onto_a_lane_lands_there() {
    let mut app = browsing();
    let rows = track_rows(&mut app, 2);
    let from = name_cell(browser_rect(&painted(&mut app)), 1);
    drag(&mut app, from, (beat_x(&rows, 4), middle(rows[1])));
    let project = landed(&mut app, &lead());
    assert_eq!(kinds(&project), vec![("MIDI 1", false, 0), ("MIDI 2", false, 1)], "wanted the browser's file on MIDI 2, got {:?}", kinds(&project));
    assert_eq!(placed(&project, 1), vec![(4 * TICKS_PER_BEAT, ONE_BAR, "Imaginary", lead_notes())], "wanted the clip at beat 4 under the pointer, got {:?}", placed(&project, 1));
}

#[test]
fn a_midi_file_in_the_browser_double_pressed_makes_a_track_at_the_playhead() {
    let mut app = browsing();
    let from = name_cell(browser_rect(&painted(&mut app)), 1);
    double_press(&mut app, from);
    let project = landed(&mut app, &lead());
    assert_eq!(kinds(&project), vec![("MIDI 1", false, 0), ("MIDI 2", false, 0), ("MIDI 3", false, 1)], "wanted a new MIDI track at the end for the double press, got {:?}", kinds(&project));
    assert_eq!(placed(&project, 2), vec![(0, ONE_BAR, "Lead", lead_notes())], "wanted the clip at the playhead named for its part, got {:?}", placed(&project, 2));
}
