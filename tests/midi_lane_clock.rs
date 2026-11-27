use bp::arp::{Arp, Articulation, Style};
use bp::core::Params;
use bp::core::params::{Envelope, Osc};
use bp::division::Division;
use bp::flatten;
use bp::project::{Aim, Clip, DEFAULT_BPM, LaneLfo, LfoClock, LfoShape, Note, Project, Row, TICKS_PER_BEAT};
use support::{AUDIBLE, CORE_ROW, FRAMES_PER_BEAT, arp_print, channels_apart, core_layer, frames, midi, mixed, peak_of, song_render};

const BEATS: f64 = 4.0;
const TAIL_SECONDS: f64 = 0.5;
const KEYS: [(u8, f32, f32, u8); 3] = [(60, 0.0, 4.0, 100), (64, 0.0, 4.0, 90), (67, 0.0, 4.0, 110)];
const LOWER_CORE: usize = 3;
const SWEEP_HZ: f64 = 1.5;
const LEVEL: f64 = 0.4;
const CLOCKS: [LfoClock; 1] = [LfoClock::Note];

fn above() -> Arp {
    Arp { on: true, style: Style::Up, rate: Division::Eighth, gate: 0.9, ..Arp::DEFAULT }
}

fn below() -> Arp {
    Arp { on: true, style: Style::Down, rate: Division::Sixteenth, gate: 0.9, ..Arp::DEFAULT }
}

fn layer(saw: f64, sine: f64) -> Row {
    let mut patch = Params::DEFAULT;
    patch.osc1 = Osc { levels: [saw, 0.0, 0.0, sine], ..Osc::with_level(0.8) };
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 5.0, ..Envelope::DEFAULT };
    Row::Core(core_layer(LEVEL, patch))
}

fn swept(row: usize, clock: LfoClock) -> LaneLfo {
    LaneLfo { amount: 1.0, shape: LfoShape::Saw, sync: false, rate_hz: SWEEP_HZ, clock, ..LaneLfo::blank(Aim::knob(row, "OSC 1", "LEVEL")) }
}

fn clip() -> Clip {
    let notes = KEYS.iter().map(|&(pitch, start, length, velocity)| Note { pitch, start, length, velocity, articulation: Articulation::PLAIN }).collect();
    Clip { start: 0, length: BEATS as i32 * TICKS_PER_BEAT, label: String::new(), notes, file: None }
}

fn playing(rack: Vec<Row>, lfos: Vec<LaneLfo>, clips: Vec<Clip>) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = rack;
    project.tracks[0].automation.lfos = lfos;
    project.tracks[0].clips = clips;
    project
}

fn heard_over(rack: Vec<Row>, lfos: Vec<LaneLfo>, clips: Vec<Clip>) -> [Vec<f32>; 2] {
    song_render(&playing(rack, lfos, clips), (BEATS * FRAMES_PER_BEAT).round() as usize + frames(TAIL_SECONDS))
}

fn heard(rack: Vec<Row>, lfos: Vec<LaneLfo>) -> [Vec<f32>; 2] {
    heard_over(rack, lfos, vec![clip()])
}

#[test]
fn a_lane_on_a_core_follows_the_midi_row_nearest_above_it() {
    let song = playing(Vec::new(), Vec::new(), vec![clip()]).tracks[0].song_notes();
    let printed = flatten::print(&[clip()], &arp_print(&song, above(), DEFAULT_BPM));
    for clock in CLOCKS {
        let both = heard(vec![midi(above()), layer(1.0, 0.0), midi(below()), layer(0.0, 1.0)], vec![swept(CORE_ROW, clock), swept(LOWER_CORE, clock)]);
        let upper = heard(vec![midi(above()), layer(1.0, 0.0)], vec![swept(CORE_ROW, clock)]);
        let lower = heard_over(vec![midi(below()), layer(0.0, 1.0)], vec![swept(CORE_ROW, clock)], printed.clone());
        let free = heard_over(vec![midi(below()), layer(0.0, 1.0)], vec![swept(CORE_ROW, LfoClock::Song)], printed.clone());
        let peak = peak_of(&lower[0]).max(peak_of(&lower[1]));
        assert!(peak > AUDIBLE, "{clock:?}: wanted the lower arp heard under its lane, got a peak of {peak}");
        assert_ne!(channels_apart(&lower, &free), [None, None], "{clock:?}: wanted the lane to follow its MIDI row's clock rather than the song's, got the two renders bit for bit alike");
        assert_eq!(channels_apart(&both, &mixed(&upper, &lower)), [None, None], "{clock:?}: wanted the lane on each Core clocked by the MIDI row nearest above it, the rack bit for bit as row 0's arp over the upper Core beside the second arp over row 0's printed notes, each under its own lane, got the left and right channels apart from these frames");
    }
}

#[test]
fn a_lane_under_a_midi_row_with_its_arp_off_follows_the_arp_above() {
    for clock in CLOCKS {
        let both = heard(vec![midi(above()), layer(1.0, 0.0), midi(Arp::DEFAULT), layer(0.0, 1.0)], vec![swept(CORE_ROW, clock), swept(LOWER_CORE, clock)]);
        let upper = heard(vec![midi(above()), layer(1.0, 0.0)], vec![swept(CORE_ROW, clock)]);
        let lower = heard(vec![midi(above()), layer(0.0, 1.0)], vec![swept(CORE_ROW, clock)]);
        assert_eq!(channels_apart(&both, &mixed(&upper, &lower)), [None, None], "{clock:?}: wanted a second MIDI row with its arp off to hand the arp's clock down to the lane on the Core below, the rack bit for bit as the arp over each Core alone summed, got the left and right channels apart from these frames");
    }
}
