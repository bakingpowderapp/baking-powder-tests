use bp::arp::{Arp, Articulation, Style};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::division::Division;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BAR, VELOCITY};
use support::{CHORD, SAMPLE_RATE, core_layer, midi};

const MAX_FRAMES: usize = 1024;
const RAGGED: [usize; 12] = [1, 7, 64, 3, 512, 128, 256, 1, 33, 900, 17, 256];
const UNIFORM: usize = 256;
const BPM: f64 = 143.7;
const RATE: Division = Division::ThirtySecondDotted;
const STEP_A_THIRD_OF_A_FRAME_SHORT_OF_A_BLOCK: f64 = 18.0 * UNIFORM as f64 - 1.0 / 3.0;
const SECONDS: f64 = 30.0;
const GENERATOR_SECONDS: f64 = 12.0;
const NOTE_BEATS: [f32; 4] = [0.0, 1.0, 1.75, 2.5];
const NOTE_LENGTH: f32 = 0.75;
const BARS: i32 = 8;

fn metronome() -> Project {
    let mut project = Project::new();
    project.bpm = BPM;
    project.metronome = true;
    project
}

fn generator() -> Project {
    let mut project = Project::new();
    project.bpm = SAMPLE_RATE * 60.0 * RATE.beats() / STEP_A_THIRD_OF_A_FRAME_SHORT_OF_A_BLOCK;
    project.tracks[0].rack = vec![midi(Arp { on: true, style: Style::UpAndDown, rate: RATE, gate: 0.383, ..Arp::DEFAULT }), Row::Core(core_layer(1.0, Params::DEFAULT))];
    for bar in 0..BARS {
        let notes = NOTE_BEATS.iter().map(|&beat| Note { pitch: 48 + bar as u8, start: beat, length: NOTE_LENGTH, velocity: VELOCITY, articulation: Articulation::PLAIN }).collect();
        project.tracks[1].clips.push(Clip { start: bar * TICKS_PER_BAR, length: TICKS_PER_BAR, label: String::new(), notes, file: None });
    }
    project
}

fn render(project: &Project, held: &[u8], playing: bool, seconds: f64, sizes: impl Iterator<Item = usize>) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, MAX_FRAMES, Seer::Inline);
    audio.sync(project, None, &[], &[]);
    for &midi in held {
        audio.note_on(0, midi, VELOCITY);
    }
    if playing {
        audio.play(0.0);
    }
    let total = (SAMPLE_RATE * seconds) as usize;
    let mut out = Vec::with_capacity(total + MAX_FRAMES);
    let (mut l, mut r) = (vec![0.0; MAX_FRAMES], vec![0.0; MAX_FRAMES]);
    for n in sizes {
        if out.len() >= total {
            break;
        }
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    out.truncate(total);
    out
}

fn same_ragged_or_uniform(project: &Project, held: &[u8], playing: bool, seconds: f64, what: &str) {
    let ragged = render(project, held, playing, seconds, RAGGED.iter().copied().cycle());
    let uniform = render(project, held, playing, seconds, std::iter::repeat(UNIFORM));
    assert!(ragged.iter().any(|sample| *sample != 0.0), "{what} is silent");
    let divergence = ragged.iter().zip(&uniform).position(|(a, b)| a.to_bits() != b.to_bits());
    let worst = ragged.iter().zip(&uniform).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
    assert!(divergence.is_none(), "{what}: ragged and uniform blocks diverge at frame {divergence:?}, worst delta {worst}");
}

#[test]
fn block_size_never_changes_the_metronome() {
    same_ragged_or_uniform(&metronome(), &[], true, SECONDS, "the metronome");
}

#[test]
fn block_size_never_changes_a_held_arp() {
    same_ragged_or_uniform(&generator(), &CHORD, false, GENERATOR_SECONDS, "the held arp");
}

#[test]
fn block_size_never_changes_a_playing_song() {
    same_ragged_or_uniform(&generator(), &CHORD, true, GENERATOR_SECONDS, "the playing song");
}
