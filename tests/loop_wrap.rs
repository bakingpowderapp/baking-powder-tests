use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{AUDIBLE, BLOCK, FRAMES_PER_BEAT, SAMPLE_RATE, SILENT, core_layer, midi};

const LOOP_BEATS: f64 = 16.0;
const CLIP_START: i32 = 12 * TICKS_PER_BEAT;
const CLIP_LENGTH: i32 = 8 * TICKS_PER_BEAT;
const NOTE_START: f32 = 3.0;
const NOTE_LENGTH: f32 = 3.0;
const PLAY_BEATS: f64 = 17.5;
const SOUNDING: (f64, f64) = (15.25, 15.9);
const RELEASED: (f64, f64) = (16.25, 17.25);
fn project() -> Project {
    let mut patch = Params::DEFAULT;
    patch.amp.attack_ms = 1.0;
    patch.amp.sustain = 1.0;
    patch.amp.release_ms = 1.0;
    let notes = vec![Note { pitch: 60, start: NOTE_START, length: NOTE_LENGTH, velocity: VELOCITY, articulation: Articulation::PLAIN }];
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch))];
    project.tracks[0].clips.push(Clip { start: CLIP_START, length: CLIP_LENGTH, label: String::new(), notes, file: None });
    project
}

fn peak_between(out: &[f32], beats: (f64, f64)) -> f32 {
    let (from, to) = ((beats.0 * FRAMES_PER_BEAT) as usize, (beats.1 * FRAMES_PER_BEAT) as usize);
    out[from..to].iter().fold(0.0f32, |peak, sample| peak.max(sample.abs()))
}

#[test]
fn a_note_across_the_loop_end_is_released_at_the_wrap() {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(&project(), None, &[], &[]);
    audio.set_loop(Some((0.0, LOOP_BEATS)));
    audio.play(0.0);
    let total = (PLAY_BEATS * FRAMES_PER_BEAT) as usize;
    let mut out = Vec::with_capacity(total + BLOCK);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    while out.len() < total {
        renderer.render(&mut l, &mut r);
        out.extend_from_slice(&l);
    }
    assert_eq!(audio.position().wraps, 1, "the loop did not wrap once in {PLAY_BEATS} beats");
    let sounding = peak_between(&out, SOUNDING);
    let released = peak_between(&out, RELEASED);
    assert!(sounding > AUDIBLE, "the note is silent before the loop end, peak {sounding}");
    assert!(released < SILENT, "wanted silence from beat {} after the wrap released the note, got peak {released}", RELEASED.0);
}
