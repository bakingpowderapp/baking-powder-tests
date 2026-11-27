use bp::arp::{Arp, Articulation, Style};
use bp::core::Params;
use bp::core::params::{Envelope, Osc};
use bp::audio;
use bp::division::Division;
use bp::flatten;
use bp::project::{Bus, Clip, DEFAULT_BPM, Note, Project, Row, SongNote, TICKS_PER_BEAT};
use support::{AUDIBLE, FRAMES_PER_BEAT, arp_print, channels_apart, core_layer, frames, midi, mixed, peak_of, sent_notes, song_render};

const BEATS: f64 = 4.0;
const TAIL_SECONDS: f64 = 0.5;
const KEYS: [(u8, f32, f32, u8); 4] = [(57, 0.0, 4.0, 100), (60, 0.0, 4.0, 90), (64, 0.5, 3.5, 110), (67, 2.0, 2.0, 80)];
const LEVEL: f64 = 0.4;

fn above() -> Arp {
    Arp { on: true, style: Style::Up, rate: Division::Eighth, gate: 0.75, ..Arp::DEFAULT }
}

fn below() -> Arp {
    Arp { on: true, style: Style::Down, rate: Division::Sixteenth, gate: 0.5, steps: 2, ..Arp::DEFAULT }
}

fn upper() -> Row {
    let mut patch = Params::DEFAULT;
    patch.osc1 = Osc { levels: [0.0, 0.0, 0.0, 1.0], ..Osc::with_level(0.8) };
    patch.amp = Envelope { attack_ms: 2.0, decay_ms: 60.0, sustain: 0.5, release_ms: 20.0, ..Envelope::DEFAULT };
    Row::Core(core_layer(LEVEL, patch))
}

fn lower() -> Row {
    let mut patch = Params::DEFAULT;
    patch.osc1 = Osc { levels: [1.0, 0.0, 0.0, 0.0], tune: 12.0, ..Osc::with_level(0.6) };
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 40.0, sustain: 0.3, release_ms: 15.0, ..Envelope::DEFAULT };
    Row::Core(core_layer(LEVEL, patch))
}

fn clip() -> Clip {
    let notes = KEYS.iter().map(|&(pitch, start, length, velocity)| Note { pitch, start, length, velocity, articulation: Articulation::PLAIN }).collect();
    Clip { start: 0, length: BEATS as i32 * TICKS_PER_BEAT, label: String::new(), notes, file: None }
}

fn playing(rack: Vec<Row>, clips: Vec<Clip>) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = rack;
    project.tracks[0].clips = clips;
    project
}

fn heard(rack: Vec<Row>) -> [Vec<f32>; 2] {
    song_render(&playing(rack, vec![clip()]), end())
}

fn end() -> usize {
    (BEATS * FRAMES_PER_BEAT).round() as usize + frames(TAIL_SECONDS)
}

fn audible(heard: &[Vec<f32>; 2], what: &str) {
    let peak = peak_of(&heard[0]).max(peak_of(&heard[1]));
    assert!(peak > AUDIBLE, "wanted {what} heard, got a peak of {peak}");
}

#[test]
fn a_second_midi_row_under_a_passing_row_zero_arps_as_a_single_arped_row() {
    let both = heard(vec![midi(Arp::DEFAULT), upper(), midi(below()), lower()]);
    let plain = heard(vec![midi(Arp::DEFAULT), upper()]);
    let arped = heard(vec![midi(below()), lower()]);
    audible(&arped, "the second arp over the lower Core");
    assert_eq!(channels_apart(&both, &mixed(&plain, &arped)), [None, None], "wanted a second MIDI row under a row 0 that passes its notes on to sound as a single arped row, got the left and right channels apart from these frames");
}

#[test]
fn a_second_midi_row_arps_the_row_aboves_output_and_not_the_tracks_own_notes() {
    let rack = vec![midi(above()), upper(), midi(below()), lower()];
    let chained = heard(rack.clone());
    let first = heard(vec![midi(above()), upper()]);
    let alone = heard(vec![midi(below()), lower()]);
    audible(&alone, "the second arp over the lower Core");
    assert_ne!(channels_apart(&chained, &mixed(&first, &alone)), [None, None], "wanted the second MIDI row to hear the arp above it, got the rack bit for bit as each arp over the track's own notes summed");
    let song = playing(rack, vec![clip()]).tracks[0].song_notes();
    let printed = flatten::print(&[clip()], &arp_print(&song, above(), DEFAULT_BPM));
    let over_printed = song_render(&playing(vec![midi(below()), lower()], printed), end());
    audible(&over_printed, "the second arp over the first arp's printed notes");
    assert_eq!(channels_apart(&chained, &mixed(&first, &over_printed)), [None, None], "wanted the second MIDI row to arp row 0's output, the rack bit for bit as row 0's arp over the upper Core beside the second arp over row 0's printed notes, got the left and right channels apart from these frames");
}

fn chained(rack: &[Row], row: usize) -> Vec<SongNote> {
    sent_notes(&playing(rack.to_vec(), vec![clip()]), 0, row)
}

fn cored(rack: &[Row]) -> Vec<SongNote> {
    let project = playing(rack.to_vec(), vec![clip()]);
    audio::sent(&project).cored(&project, 0)
}

#[test]
fn what_a_bused_track_plays_is_what_the_midi_row_in_each_bus_plays_taken_together() {
    let rack = vec![midi(Arp::DEFAULT), midi(above()), upper(), Row::Bus(Bus::DEFAULT), midi(below()), lower()];
    let (first, second) = (chained(&rack, 1), chained(&rack, 4));
    assert_eq!(first, chained(&rack[..3], 1), "wanted bus 1's MIDI row to play what it plays with no second bus");
    assert_eq!(second, chained(&[rack[0], rack[4], rack[5]], 1), "wanted bus 2's MIDI row to play the first MIDI row's notes, as with bus 1 and its arp taken away");
    assert!(!first.is_empty() && !second.is_empty() && first != second, "wanted the two buses to play lines of their own, so the sum proves something");
    let mut wanted: Vec<SongNote> = first.iter().chain(&second).cloned().collect();
    wanted.sort_by(|a, b| a.start.total_cmp(&b.start).then(a.pitch.cmp(&b.pitch)));
    wanted.dedup();
    assert_eq!(cored(&rack), wanted, "wanted what the track plays to be both buses' notes together, in order of start");
    let plain = &rack[..3];
    assert_eq!(cored(plain), chained(plain, 1), "wanted a rack with no bus row to play what its last MIDI row plays");
}
