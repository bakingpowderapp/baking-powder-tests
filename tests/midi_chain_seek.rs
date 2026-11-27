use bp::arp::{Arp, Articulation, Style};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::core::params::{Envelope, Osc};
use bp::division::Division;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT};
use support::{AUDIBLE, BLOCK, FRAMES_PER_BEAT, SAMPLE_RATE, channels_apart, core_layer, frames, midi, mixed, peak_of};

const BEATS: f64 = 8.0;
const FROM_BEAT: f64 = 2.6;
const TAIL_SECONDS: f64 = 0.5;
const KEYS: [(u8, u8); 3] = [(57, 100), (60, 90), (64, 110)];
const LEVEL: f64 = 0.4;

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

fn sustained(rack: Vec<Row>) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = rack;
    let notes = KEYS.iter().map(|&(pitch, velocity)| Note { pitch, start: 0.0, length: BEATS as f32, velocity, articulation: Articulation::PLAIN }).collect();
    project.tracks[0].clips.push(Clip { start: 0, length: BEATS as i32 * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn heard_from(rack: Vec<Row>, from: f64) -> [Vec<f32>; 2] {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(&sustained(rack), None, &[], &[]);
    audio.play(from);
    let total = ((BEATS - from) * FRAMES_PER_BEAT).round() as usize + frames(TAIL_SECONDS);
    let (mut left, mut right) = (Vec::with_capacity(total), Vec::with_capacity(total));
    let (mut l, mut r) = ([0.0; BLOCK], [0.0; BLOCK]);
    while left.len() < total {
        let n = BLOCK.min(total - left.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        left.extend_from_slice(&l[..n]);
        right.extend_from_slice(&r[..n]);
    }
    [left, right]
}

#[test]
fn a_second_midi_row_entered_inside_a_sustained_chord_arps_as_the_single_arped_row_does() {
    let both = heard_from(vec![midi(Arp::DEFAULT), upper(), midi(below()), lower()], FROM_BEAT);
    let plain = heard_from(vec![midi(Arp::DEFAULT), upper()], FROM_BEAT);
    let arped = heard_from(vec![midi(below()), lower()], FROM_BEAT);
    let peak = peak_of(&arped[0]).max(peak_of(&arped[1]));
    assert!(peak > AUDIBLE, "wanted the arp heard from beat {FROM_BEAT}, got a peak of {peak}");
    assert_eq!(channels_apart(&both, &mixed(&plain, &arped)), [None, None], "wanted play from beat {FROM_BEAT} inside the chord to reach the second MIDI row with the chord's true start and no burst of missed steps, the rack bit for bit as the passing row over its Core beside the same arp on row 0 over its own, got the left and right channels apart from these frames");
}
