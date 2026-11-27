use bp::app::{App, Event};
use bp::arp::{Arp, Articulation, Style};
use bp::core::Params;
use bp::core::params::{Envelope, Osc};
use bp::division::Division;
use bp::face::{CELL, PITCH, READOUT_ROWS};
use bp::flatten;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT};
use support::{AUDIBLE, FRAMES_PER_BEAT, channels_apart, core_layer, face_rect, frames, midi, painted, peak_of, remembered, sent_notes, song_render, struck, unseeded, worst_delta};

const FLAT: (i32, i32) = (5, 12);
const MIDDLE: usize = 2;
const BEATS: f64 = 4.0;
const TAIL_SECONDS: f64 = 0.5;
const KEYS: [(u8, f32, f32, u8); 3] = [(60, 0.0, 4.0, 100), (64, 0.0, 4.0, 90), (67, 1.0, 3.0, 110)];
const LEVEL: f64 = 0.3;
const ALIKE: f32 = 1e-5;

fn above() -> Arp {
    Arp { on: true, style: Style::Up, rate: Division::Eighth, gate: 0.75, ..Arp::DEFAULT }
}

fn below() -> Arp {
    Arp { on: true, style: Style::Down, rate: Division::Sixteenth, gate: 0.5, steps: 2, ..Arp::DEFAULT }
}

fn third() -> Arp {
    Arp { on: true, style: Style::UpDown, rate: Division::Twelfth, gate: 0.6, ..Arp::DEFAULT }
}

fn flattened(arp: Arp) -> Row {
    struck(Arp { on: false, ..arp })
}

fn layer(saw: f64, sine: f64, decay_ms: f64) -> Row {
    let mut patch = Params::DEFAULT;
    patch.osc1 = Osc { levels: [saw, 0.0, 0.0, sine], ..Osc::with_level(0.8) };
    patch.amp = Envelope { attack_ms: 2.0, decay_ms, sustain: 0.4, release_ms: 20.0, ..Envelope::DEFAULT };
    Row::Core(core_layer(LEVEL, patch))
}

fn playing(rack: Vec<Row>) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = rack;
    let notes = KEYS.iter().map(|&(pitch, start, length, velocity)| Note { pitch, start, length, velocity, articulation: Articulation::PLAIN }).collect();
    project.tracks[0].clips.push(Clip { start: 0, length: BEATS as i32 * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn end() -> usize {
    (BEATS * FRAMES_PER_BEAT).round() as usize + frames(TAIL_SECONDS)
}

fn open(project: &Project, open: usize) -> App {
    let session = format!("{{\"project\":{},\"open\":{open},\"keyboard\":false,\"palette\":false}}", project.to_json().write());
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

fn heard(project: &Project, what: &str) -> [Vec<f32>; 2] {
    let got = song_render(project, end());
    let peak = peak_of(&got[0]).max(peak_of(&got[1]));
    assert!(peak > AUDIBLE, "wanted {what} heard after FLAT, got a peak of {peak}");
    got
}

#[test]
fn flat_on_a_middle_row_takes_the_rows_below_it_and_leaves_the_row_above_its_notes() {
    let rack = vec![midi(above()), layer(0.0, 1.0, 200.0), midi(below()), layer(1.0, 0.0, 90.0), midi(third()), layer(0.5, 0.5, 150.0)];
    let project = playing(rack.clone());
    let printed = flatten::print(&project.tracks[0].clips, &sent_notes(&project, 0, MIDDLE));
    let mut app = open(&project, MIDDLE);
    press_flat(&mut app);
    let flat = remembered(&mut app);
    assert_eq!(flat.tracks.len(), 2, "wanted the flattened row and the rows below it split onto a track of their own, got {} tracks", flat.tracks.len());
    assert_eq!(unseeded(&flat.tracks[0].rack), rack[..MIDDLE].to_vec(), "wanted the first arp and its Core left on the first track");
    assert_eq!(flat.tracks[0].clips, project.tracks[0].clips, "wanted the first track to keep the notes its own arp reads");
    assert_eq!(unseeded(&flat.tracks[1].rack), vec![flattened(below()), layer(1.0, 0.0, 90.0), midi(third()), layer(0.5, 0.5, 150.0)], "wanted the flattened row with its arp off and its VEL at 127, its Core, and the MIDI row that heard it with its own Core on the new track");
    assert_eq!(flat.tracks[1].clips, printed, "wanted the new track's clips to carry the flattened row's output");
    let (got, wanted) = (heard(&flat, "the two tracks"), song_render(&project, end()));
    let worst = worst_delta(&got[0], &wanted[0]).max(worst_delta(&got[1], &wanted[1]));
    assert!(worst < ALIKE, "wanted the two tracks to sound as the rack did before FLAT within {ALIKE}, the Cores now summed across two tracks, got a worst delta of {worst}");
}

#[test]
fn flat_on_row_zero_prints_in_place_and_keeps_every_row_below_it_on_the_track() {
    let rack = vec![midi(above()), layer(0.0, 1.0, 200.0), midi(third()), layer(0.5, 0.5, 150.0), midi(below()), layer(1.0, 0.0, 90.0)];
    let project = playing(rack.clone());
    let printed = flatten::print(&project.tracks[0].clips, &sent_notes(&project, 0, 0));
    let mut app = open(&project, 0);
    press_flat(&mut app);
    let flat = remembered(&mut app);
    assert_eq!(flat.tracks.len(), 1, "wanted FLAT on row 0 to keep one track, every MIDI row below it hearing it still, got {} tracks", flat.tracks.len());
    let kept: Vec<Row> = std::iter::once(flattened(above())).chain(rack[1..].iter().copied()).collect();
    assert_eq!(unseeded(&flat.tracks[0].rack), kept, "wanted row 0 with its arp off and its VEL at 127 and every row below it left as it was");
    assert_eq!(flat.tracks[0].clips, printed, "wanted the track's clips to carry row 0's output");
    assert_eq!(channels_apart(&heard(&flat, "the track"), &song_render(&project, end())), [None, None], "wanted the track to sound bit for bit as the rack did before FLAT, the rows below row 0 arping its printed notes, got the left and right channels apart from these frames");
}
