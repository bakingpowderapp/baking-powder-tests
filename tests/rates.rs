use bp::arp::{Arp, Articulation, Style};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::division::Division;
use bp::project::{Clip, DEFAULT_BPM, Note, Project, Row, TICKS_PER_BAR, VELOCITY};
use support::{CHORD, GAP, SILENT, WITHIN, core_layer, midi};

const RATES: [f64; 2] = [44_100.0, 96_000.0];
const MAX_FRAMES: usize = 1024;
const RAGGED: [usize; 12] = [1, 7, 64, 3, 512, 128, 256, 1, 33, 900, 17, 256];
const UNIFORM: usize = 256;
const BPM: f64 = 143.7;
const RATE: Division = Division::ThirtySecondDotted;
const STEP_A_THIRD_OF_A_FRAME_SHORT_OF_A_BLOCK: f64 = 18.0 * UNIFORM as f64 - 1.0 / 3.0;
const SECONDS: f64 = 12.0;
const NOTE_BEATS: [f32; 4] = [0.0, 1.0, 1.75, 2.5];
const NOTE_LENGTH: f32 = 0.75;
const BARS: i32 = 8;
const BEATS: usize = 60;
fn metronome(bpm: f64) -> Project {
    let mut project = Project::new();
    project.bpm = bpm;
    project.metronome = true;
    project
}

fn generator(sample_rate: f64) -> Project {
    let mut project = Project::new();
    project.bpm = sample_rate * 60.0 * RATE.beats() / STEP_A_THIRD_OF_A_FRAME_SHORT_OF_A_BLOCK;
    project.tracks[0].rack = vec![midi(Arp { on: true, style: Style::UpAndDown, rate: RATE, gate: 0.383, ..Arp::DEFAULT }), Row::Core(core_layer(1.0, Params::DEFAULT))];
    for bar in 0..BARS {
        let notes = NOTE_BEATS.iter().map(|&beat| Note { pitch: 48 + bar as u8, start: beat, length: NOTE_LENGTH, velocity: VELOCITY, articulation: Articulation::PLAIN }).collect();
        project.tracks[1].clips.push(Clip { start: bar * TICKS_PER_BAR, length: TICKS_PER_BAR, label: String::new(), notes, file: None });
    }
    project
}

fn render(project: &Project, sample_rate: f64, held: &[u8], playing: bool, seconds: f64, sizes: impl Iterator<Item = usize>) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(sample_rate, MAX_FRAMES, Seer::Inline);
    audio.sync(project, None, &[], &[]);
    for &midi in held {
        audio.note_on(0, midi, VELOCITY);
    }
    if playing {
        audio.play(0.0);
    }
    let total = (sample_rate * seconds) as usize;
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

fn same_ragged_or_uniform(project: &Project, sample_rate: f64, held: &[u8], playing: bool, what: &str) {
    let ragged = render(project, sample_rate, held, playing, SECONDS, RAGGED.iter().copied().cycle());
    let uniform = render(project, sample_rate, held, playing, SECONDS, std::iter::repeat(UNIFORM));
    assert!(ragged.iter().any(|sample| *sample != 0.0), "{what} at {sample_rate} Hz is silent");
    let divergence = ragged.iter().zip(&uniform).position(|(a, b)| a.to_bits() != b.to_bits());
    let worst = ragged.iter().zip(&uniform).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
    assert!(divergence.is_none(), "{what} at {sample_rate} Hz: ragged and uniform blocks diverge at frame {divergence:?}, worst delta {worst}");
}

fn onsets(project: &Project, sample_rate: f64, frames: usize) -> Vec<usize> {
    let out = render(project, sample_rate, &[], true, frames as f64 / sample_rate, std::iter::repeat(UNIFORM));
    let (mut quiet, mut at) = (GAP, Vec::new());
    for (frame, sample) in out.iter().enumerate() {
        if sample.abs() < SILENT {
            quiet += 1;
        } else {
            if quiet >= GAP {
                at.push(frame);
            }
            quiet = 0;
        }
    }
    at
}

#[test]
fn block_size_never_changes_the_metronome_at_other_sample_rates() {
    for rate in RATES {
        same_ragged_or_uniform(&metronome(BPM), rate, &[], true, "the metronome");
    }
}

#[test]
fn block_size_never_changes_a_playing_song_at_other_sample_rates() {
    for rate in RATES {
        same_ragged_or_uniform(&generator(rate), rate, &CHORD, true, "the playing song");
    }
}

#[test]
fn the_metronome_lands_on_the_beat_at_other_sample_rates() {
    for rate in RATES {
        let frames_per_beat = rate * 60.0 / DEFAULT_BPM;
        let beat_frame = |beat: usize| (beat as f64 * frames_per_beat).round() as usize;
        let onsets = onsets(&metronome(DEFAULT_BPM), rate, beat_frame(BEATS) + UNIFORM);
        let missed: Vec<usize> = (0..=BEATS).filter(|&beat| !onsets.iter().any(|&at| at.abs_diff(beat_frame(beat)) <= WITHIN)).collect();
        let stray: Vec<f64> = onsets.iter().filter(|&&at| !(0..=BEATS).any(|beat| at.abs_diff(beat_frame(beat)) <= WITHIN)).map(|&at| at as f64 / frames_per_beat).collect();
        assert!(missed.is_empty() && stray.is_empty(), "at {rate} Hz: missed beats {missed:?}, stray onsets at beats {stray:?}");
    }
}
