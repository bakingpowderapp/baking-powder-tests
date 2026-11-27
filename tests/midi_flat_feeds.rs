use bp::app::{App, Event};
use bp::arp::{Arp, Articulation, Style};
use bp::core::Params;
use bp::core::params::{Envelope, Osc};
use bp::division::Division;
use bp::face::{CELL, PITCH, READOUT_ROWS};
use bp::flatten;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, Track};
use support::{AUDIBLE, FRAMES_PER_BEAT, channels_apart, core_layer, face_rect, frames, midi, painted, peak_of, remembered, sent_notes, song_render, struck, unseeded};

const FLAT: (i32, i32) = (5, 12);
const SOURCE: usize = 0;
const FOLLOWER: usize = 1;
const MIDI_ROW: usize = 0;
const BEATS: f64 = 4.0;
const TAIL_SECONDS: f64 = 0.5;
const KEYS: [(u8, f32, f32, u8); 3] = [(60, 0.0, 4.0, 100), (64, 0.0, 4.0, 90), (67, 1.0, 3.0, 110)];
const LEVEL: f64 = 0.4;

fn below() -> Arp {
    Arp { on: true, style: Style::Down, rate: Division::Sixteenth, gate: 0.5, steps: 2, ..Arp::DEFAULT }
}

fn layer(sine: f64, decay_ms: f64) -> Row {
    let mut patch = Params::DEFAULT;
    patch.osc1 = Osc { levels: [1.0 - sine, 0.0, 0.0, sine], ..Osc::with_level(0.8) };
    patch.amp = Envelope { attack_ms: 2.0, decay_ms, sustain: 0.4, release_ms: 20.0, ..Envelope::DEFAULT };
    Row::Core(core_layer(LEVEL, patch))
}

fn fed() -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[SOURCE].rack = vec![midi(Arp::DEFAULT), layer(1.0, 200.0)];
    let notes = KEYS.iter().map(|&(pitch, start, length, velocity)| Note { pitch, start, length, velocity, articulation: Articulation::PLAIN }).collect();
    project.tracks[SOURCE].clips.push(Clip { start: 0, length: BEATS as i32 * TICKS_PER_BEAT, label: String::from("I"), notes, file: None });
    project.tracks.push(Track { follows: Some(SOURCE), rack: vec![midi(below()), layer(0.0, 90.0)], ..Track::blank() });
    project
}

fn end() -> usize {
    (BEATS * FRAMES_PER_BEAT).round() as usize + frames(TAIL_SECONDS)
}

fn open(project: &Project) -> App {
    let session = format!("{{\"project\":{},\"selected\":{FOLLOWER},\"open\":{MIDI_ROW},\"keyboard\":false,\"palette\":false}}", project.to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    painted(&mut app);
    app.take_requests();
    app
}

fn press_flat(app: &mut App) {
    let face = face_rect(&painted(app));
    let (x, y) = (face.x + FLAT.1 * PITCH + CELL / 2, face.y + (READOUT_ROWS + FLAT.0) * PITCH + CELL / 2);
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
}

#[test]
fn flat_on_a_follower_without_clips_prints_the_fed_notes_into_clips_of_its_own() {
    let project = fed();
    let printed = flatten::print(&project.tracks[SOURCE].clips, &sent_notes(&project, FOLLOWER, MIDI_ROW));
    assert!(!printed.iter().all(|clip| clip.notes.is_empty()), "wanted the follower's arp to play the source's notes");
    let mut app = open(&project);
    press_flat(&mut app);
    let flat = remembered(&mut app);
    assert_eq!(flat.tracks.len(), 2, "wanted FLAT on the follower's only MIDI row to keep two tracks, got {}", flat.tracks.len());
    assert_eq!(flat.tracks[FOLLOWER].follows, None, "wanted the follower to stop following once its arp was printed");
    assert_eq!(flat.tracks[FOLLOWER].clips, printed, "wanted the fed notes as the arp played them printed into clips spanning the source's clips");
    assert_eq!(unseeded(&flat.tracks[FOLLOWER].rack), vec![struck(Arp { on: false, ..below() }), layer(0.0, 90.0)], "wanted the follower's arp off, its VEL at 127 now the notes carry it, and its Core kept");
    assert_eq!(flat.tracks[SOURCE].clips, project.tracks[SOURCE].clips, "wanted the source's clips left as they were");
    let (got, wanted) = (song_render(&flat, end()), song_render(&project, end()));
    let peak = peak_of(&got[0]).max(peak_of(&got[1]));
    assert!(peak > AUDIBLE, "wanted the printed follower heard after FLAT, got a peak of {peak}");
    assert_eq!(channels_apart(&got, &wanted), [None, None], "wanted the printed follower to sound bit for bit as it did fed by the source, got the left and right channels apart from these frames");
}
