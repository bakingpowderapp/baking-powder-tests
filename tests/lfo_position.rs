use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params as Patch;
use bp::core::params::Envelope;
use bp::fx::params::MotionMode;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{AUDIBLE, BLOCK, FRAMES_PER_BEAT, SAMPLE_RATE, core_layer, peak_of, stage, struck, worst_delta};

const A2: u8 = 45;
const NOTE_START: i32 = 3;
const NOTE_BEATS: i32 = 4;
const CLEAR_OF_THE_START: f64 = 0.25;
const SWEPT: f32 = 1e-2;
const ALIKE: f32 = 1e-3;
const MODES: [(MotionMode, &str); 3] = [(MotionMode::Chorus, "the chorus"), (MotionMode::Flanger, "the flanger"), (MotionMode::Phaser, "the phaser")];

fn saw() -> Patch {
    let mut patch = Patch::DEFAULT;
    patch.phase_random = 0.0;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, velocity: 0.0, ..Envelope::DEFAULT };
    patch
}

fn project(mode: Option<MotionMode>) -> Project {
    let notes = vec![Note { pitch: A2, start: 0.0, length: NOTE_BEATS as f32, velocity: VELOCITY, articulation: Articulation::PLAIN }];
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = [struck(Arp::DEFAULT), Row::Core(core_layer(1.0, saw()))].into_iter().chain(mode.map(|mode| Row::Effect(stage(mode)))).collect();
    project.tracks[0].clips.push(Clip { start: NOTE_START * TICKS_PER_BEAT, length: NOTE_BEATS * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn frame(beat: f64) -> usize {
    (beat * FRAMES_PER_BEAT).round() as usize
}

fn note_heard(mode: Option<MotionMode>, from: f64) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(&project(mode), None, &[], &[]);
    audio.play(from);
    let total = frame(f64::from(NOTE_START + NOTE_BEATS) - from);
    let mut out = Vec::with_capacity(total);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    while out.len() < total {
        let n = BLOCK.min(total - out.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    out.split_off(frame(f64::from(NOTE_START) + CLEAR_OF_THE_START - from))
}

#[test]
fn a_sweep_heard_from_the_notes_bar_is_the_sweep_the_whole_song_plays_there() {
    let dry = note_heard(None, 0.0);
    for (mode, what) in MODES {
        let whole = note_heard(Some(mode), 0.0);
        let late = note_heard(Some(mode), f64::from(NOTE_START));
        assert!(peak_of(&whole) > AUDIBLE, "wanted the note heard through {what}, got a peak of {}", peak_of(&whole));
        let swept = worst_delta(&whole, &dry) / peak_of(&dry);
        assert!(swept > SWEPT, "wanted {what} to colour the note before judging its sweep, got it within {swept:.5} of the dry note");
        let apart = worst_delta(&whole, &late) / peak_of(&whole);
        assert!(apart < ALIKE, "wanted {what} begun at beat {NOTE_START} to stand where the whole song has it by then, every sample within {ALIKE} of the peak, got one {apart:.4} of the peak off");
    }
}
