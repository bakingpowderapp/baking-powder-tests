use bp::app::{App, Request};
use bp::arp::Articulation;
use bp::browser::Kind;
use bp::json::{self, Json};
use bp::project::{Note, Project, TICKS_PER_BEAT, TRACKS, Track};
use support::{HEIGHT, WIDTH, beat_x, lane_rows, painted};

const RAIL: i32 = 45;
const TILE: i32 = 120;
const BORDER: i32 = 8;
const PITCH: i32 = TILE - BORDER;
const DROP_BEAT: i32 = 4;
const STROBE: &str = "/x/Riffs/Strobe.mid";
const PPQ: u8 = 96;
const FORMAT_PARALLEL: u16 = 1;
const NOTE_ON: u8 = 0x90;
const NOTE_OFF: u8 = 0x80;
const META: u8 = 0xFF;
const TRACK_NAME: u8 = 0x03;
const END_OF_TRACK: u8 = 0x2F;
const LOUD: u8 = 100;
const LEAD: (&str, u8) = ("Lead", 72);
const CHORDS: (&str, u8) = ("Chords", 60);
const BASS: (&str, u8) = ("Bass", 36);
const THREE: [(&str, u8); 3] = [LEAD, CHORDS, BASS];
const KEYS: usize = 1;

type Laid<'a> = Vec<(&'a str, Vec<(&'a str, Vec<Note>)>)>;

fn chunk(tag: &[u8], body: &[u8]) -> Vec<u8> {
    let mut out = tag.to_vec();
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(body);
    out
}

fn part((name, pitch): (&str, u8)) -> Vec<u8> {
    let mut body = vec![0, META, TRACK_NAME, name.len() as u8];
    body.extend_from_slice(name.as_bytes());
    body.extend([0, NOTE_ON, pitch, LOUD, PPQ, NOTE_OFF, pitch, 0, 0, META, END_OF_TRACK, 0]);
    chunk(b"MTrk", &body)
}

fn score(parts: &[(&str, u8)]) -> Vec<u8> {
    let mut header = FORMAT_PARALLEL.to_be_bytes().to_vec();
    header.extend_from_slice(&(parts.len() as u16).to_be_bytes());
    header.extend_from_slice(&u16::from(PPQ).to_be_bytes());
    let mut out = chunk(b"MThd", &header);
    for &voice in parts {
        out.extend(part(voice));
    }
    out
}

fn quarter(pitch: u8) -> Vec<Note> {
    vec![Note { pitch, start: 0.0, length: 1.0, velocity: LOUD, articulation: Articulation::PLAIN }]
}

fn open(project: &Project, selected: usize) -> App {
    let session = format!("{{\"project\":{},\"keyboard\":false,\"selected\":{selected}}}", project.to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.draw(WIDTH, HEIGHT);
    app.take_requests();
    app
}

fn dropped_on(app: &mut App, slot: usize, bytes: &[u8]) -> Json {
    let x = beat_x(&lane_rows(&painted(app)), DROP_BEAT);
    app.dropped(String::from(STROBE), Kind::Midi, x, RAIL + slot as i32 * PITCH + TILE / 2);
    let mut remembered = None;
    loop {
        let requests = app.take_requests();
        if requests.is_empty() {
            break;
        }
        for request in requests {
            match request {
                Request::Read { path } => app.read(path, Some(bytes.to_vec())),
                Request::Remember { json } => remembered = Some(json),
                _ => {}
            }
        }
    }
    json::parse(&remembered.expect("wanted the drop remembered")).expect("wanted the remembered session to parse")
}

fn project_of(session: &Json) -> Project {
    Project::from_json(session.get("project").expect("wanted the remembered session to hold the project")).expect("wanted the remembered project to read")
}

fn laid(project: &Project) -> Laid<'_> {
    project.tracks.iter().map(|track| (track.name.as_str(), track.clips.iter().map(|clip| (clip.label.as_str(), clip.notes.clone())).collect())).collect()
}

fn picked_tracks(session: &Json) -> Vec<i64> {
    session.get("picked").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(|pick| pick.as_array()?.first()?.as_i64()).collect()
}

fn named(name: &str, inside: bool) -> Track {
    Track { name: String::from(name), inside, ..Track::blank() }
}

#[test]
fn three_parts_dropped_on_a_midi_lane_land_one_there_and_open_a_track_below_for_each_other_part() {
    let mut app = open(&Project::new(), KEYS);
    let session = dropped_on(&mut app, 0, &score(&THREE));
    let project = project_of(&session);
    let wanted = vec![("MIDI 1", vec![("Strobe", quarter(LEAD.1))]), ("MIDI 3", vec![("Chords", quarter(CHORDS.1))]), ("MIDI 4", vec![("Bass", quarter(BASS.1))]), ("MIDI 2", vec![])];
    assert_eq!(laid(&project), wanted, "wanted the lead on MIDI 1 named for the file, a new track for the chords and one for the bass named for their parts, and MIDI 2 pushed below them untouched, got {:?}", laid(&project));
    let starts: Vec<i32> = project.tracks.iter().flat_map(|track| track.clips.iter().map(|clip| clip.start)).collect();
    assert_eq!(starts, vec![DROP_BEAT * TICKS_PER_BEAT; 3], "wanted every part's clip at beat {DROP_BEAT} under the pointer, got {starts:?}");
    assert_eq!(picked_tracks(&session), vec![0, 1, 2], "wanted all three clips picked together after the drop");
    assert_eq!(session.get("selected").and_then(Json::as_i64), Some(3), "wanted the selection to stay on MIDI 2 as the opened tracks pushed it down");
}

#[test]
fn one_part_dropped_on_a_midi_lane_opens_no_track() {
    let mut app = open(&Project::new(), 0);
    let session = dropped_on(&mut app, KEYS, &score(&[LEAD]));
    let project = project_of(&session);
    assert_eq!(laid(&project), vec![("MIDI 1", vec![]), ("MIDI 2", vec![("Strobe", quarter(LEAD.1))])], "wanted the one part on MIDI 2 alone with no track opened, got {:?}", laid(&project));
    assert_eq!(session.get("selected").and_then(Json::as_i64).unwrap_or(0), 0, "wanted the selection left on MIDI 1 above the drop");
}

#[test]
fn parts_dropped_on_a_member_of_a_group_open_their_tracks_inside_it() {
    let mut project = Project::new();
    project.tracks = vec![Track { name: String::from("Band"), ..Track::group() }, named("Keys", true), named("Outside", false)];
    let mut app = open(&project, KEYS);
    let project = project_of(&dropped_on(&mut app, KEYS, &score(&THREE)));
    let tracks: Vec<(&str, bool)> = project.tracks.iter().map(|track| (track.name.as_str(), track.inside)).collect();
    assert_eq!(tracks, vec![("Band", false), ("Keys", true), ("MIDI 1", true), ("MIDI 2", true), ("Outside", false)], "wanted the chords and bass tracks opened under Keys inside Band, Outside left out, got {tracks:?}");
}

#[test]
fn parts_past_the_track_cap_are_left_out() {
    let mut project = Project::new();
    while project.tracks.len() < TRACKS - 1 {
        assert!(project.add_midi_track(project.tracks.len()), "wanted room for track {}", project.tracks.len() + 1);
    }
    let mut app = open(&project, 0);
    let project = project_of(&dropped_on(&mut app, 0, &score(&THREE)));
    assert_eq!(project.tracks.len(), TRACKS, "wanted the drop to fill the last free track and stop at the cap");
    let clips: Vec<(&str, &str)> = project.tracks.iter().flat_map(|track| track.clips.iter().map(|clip| (track.name.as_str(), clip.label.as_str()))).collect();
    assert_eq!(clips, vec![("MIDI 1", "Strobe"), ("MIDI 64", "Chords")], "wanted the lead on MIDI 1 and the chords on the one track the cap allowed, the bass left out, got {clips:?}");
}
