use bp::arp::{Arp, Articulation};
use bp::audio::{Audio, Renderer};
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::core::params::Envelope;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{AUDIBLE, BLOCK, SAMPLE_RATE, SILENT, WINDOW, beat_frame, core_layer, midi, peak_of};

const LOW: u8 = 69;
const HIGH: u8 = 81;
const LOW_HZ: f64 = 440.0;
const HIGH_HZ: f64 = 880.0;
const LONG: f32 = 8.0;
const SHORT: f32 = 2.0;
const LENGTHENED: f32 = 4.0;
const CLIP_BEATS: i32 = 8;
const CLEAR: f64 = 10.0;
const CUT: usize = 4 * BLOCK;

fn project(notes: &[(u8, f32)]) -> Project {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch))];
    let notes = notes.iter().map(|&(pitch, length)| Note { pitch, start: 0.0, length, velocity: VELOCITY, articulation: Articulation::PLAIN }).collect();
    track.clips.push(Clip { start: 0, length: CLIP_BEATS * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn render(renderer: &mut Renderer, frames: usize) -> Vec<f32> {
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(frames);
    while out.len() < frames {
        let n = BLOCK.min(frames - out.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    out
}

fn play(first: &[(u8, f32)], then: &[(u8, f32)]) -> (Vec<f32>, Vec<f32>) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(&project(first), None, &[], &[]);
    audio.play(0.0);
    let before = render(&mut renderer, beat_frame(1));
    audio.sync(&project(then), None, &[], &[]);
    let after = render(&mut renderer, beat_frame(2));
    (before, after)
}

fn amplitude_at(samples: &[f32], hz: f64) -> f64 {
    let (mut sines, mut cosines) = (0.0, 0.0);
    for (i, &sample) in samples.iter().enumerate() {
        let angle = std::f64::consts::TAU * hz * i as f64 / SAMPLE_RATE;
        sines += f64::from(sample) * angle.sin();
        cosines += f64::from(sample) * angle.cos();
    }
    2.0 * (sines * sines + cosines * cosines).sqrt() / samples.len() as f64
}

fn later_half_beat(samples: &[f32]) -> &[f32] {
    &samples[beat_frame(1) / 2..beat_frame(1)]
}

#[test]
fn a_playing_note_moved_to_a_new_pitch_sounds_the_new_pitch_at_once() {
    let (before, after) = play(&[(LOW, LONG)], &[(HIGH, LONG)]);
    let (low_before, high_before) = (amplitude_at(later_half_beat(&before), LOW_HZ), amplitude_at(later_half_beat(&before), HIGH_HZ));
    let (low_after, high_after) = (amplitude_at(later_half_beat(&after), LOW_HZ), amplitude_at(later_half_beat(&after), HIGH_HZ));
    assert!(low_before > high_before * CLEAR, "wanted {LOW_HZ} Hz alone before the move, got {low_before} at {LOW_HZ} Hz against {high_before} at {HIGH_HZ} Hz");
    assert!(high_after > low_after * CLEAR, "wanted {HIGH_HZ} Hz alone half a beat after the move, got {high_after} at {HIGH_HZ} Hz against {low_after} at {LOW_HZ} Hz");
}

#[test]
fn a_note_taken_out_of_the_song_while_it_plays_stops_at_once() {
    let (before, after) = play(&[(LOW, LONG)], &[]);
    let heard = peak_of(&before);
    assert!(heard > AUDIBLE, "wanted the note sounding through its first beat, got a peak of {heard}");
    let last = after.iter().rposition(|sample| sample.abs() > SILENT);
    assert!(last.is_none_or(|last| last < CUT), "wanted the note silent within {CUT} frames of leaving the song, got sound until frame {last:?}");
}

#[test]
fn a_playing_note_lengthened_sounds_past_its_old_end() {
    let at = beat_frame(1) + beat_frame(1) / 2;
    let (_, alone) = play(&[(LOW, SHORT)], &[(LOW, SHORT)]);
    let (_, lengthened) = play(&[(LOW, SHORT)], &[(LOW, LENGTHENED)]);
    let (ended, held) = (peak_of(&alone[at..at + WINDOW]), peak_of(&lengthened[at..at + WINDOW]));
    assert!(ended <= SILENT, "wanted a {SHORT} beat note silent at beat 2.5, got a peak of {ended}");
    assert!(held > AUDIBLE, "wanted the note still sounding at beat 2.5 once lengthened to {LENGTHENED} beats, got a peak of {held}");
}
