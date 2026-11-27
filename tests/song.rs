use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BAR, VELOCITY};
use support::{BLOCK, FRAMES_PER_BEAT, Onsets, SAMPLE_RATE, WITHIN, beat_frame, core_layer, midi};

const LOOP_BEATS: usize = 16;
const BEATS: usize = 240;
const NOTE_BEATS: [usize; 2] = [0, 2];
fn project(notes: bool, metronome: bool) -> Project {
    let mut patch = Params::DEFAULT;
    patch.amp.attack_ms = 1.0;
    patch.amp.release_ms = 1.0;
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch))];
    project.metronome = metronome;
    if notes {
        for bar in 0..LOOP_BEATS as i32 / 4 {
            let notes = NOTE_BEATS.iter().map(|&beat| Note { pitch: 60, start: beat as f32, length: 1.0, velocity: VELOCITY, articulation: Articulation::PLAIN }).collect();
            project.tracks[0].clips.push(Clip { start: bar * TICKS_PER_BAR, length: TICKS_PER_BAR, label: String::new(), notes, file: None });
        }
    }
    project
}

fn play(project: &Project) -> (Vec<usize>, u16) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, None, &[], &[]);
    audio.set_loop(Some((0.0, LOOP_BEATS as f64)));
    audio.play(0.0);
    let mut onsets = Onsets::default();
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let total = ((BEATS as f64 + 0.5) * FRAMES_PER_BEAT) as usize;
    while onsets.frame < total {
        renderer.render(&mut l, &mut r);
        onsets.feed(&l);
    }
    (onsets.at, audio.position().wraps)
}

fn check(onsets: &[usize], expected: &[usize]) {
    let near = |at: usize, frame: usize| at.abs_diff(frame) <= WITHIN;
    let missed: Vec<f64> = expected.iter().filter(|&&frame| !onsets.iter().any(|&at| near(at, frame))).map(|&frame| frame as f64 / FRAMES_PER_BEAT).collect();
    let stray: Vec<f64> = onsets.iter().filter(|&&at| !expected.iter().any(|&frame| near(at, frame))).map(|&at| at as f64 / FRAMES_PER_BEAT).collect();
    assert!(missed.is_empty() && stray.is_empty(), "missed beats {missed:?}, stray onsets at beats {stray:?}");
}

#[test]
fn song_notes_land_on_their_beat_through_loop_wraps() {
    let (onsets, wraps) = play(&project(true, false));
    let expected: Vec<usize> = (0..=BEATS / LOOP_BEATS)
        .flat_map(|wrap| (0..LOOP_BEATS / 4).flat_map(move |bar| NOTE_BEATS.iter().map(move |&beat| wrap * LOOP_BEATS + bar * 4 + beat)))
        .filter(|&beat| beat <= BEATS)
        .map(beat_frame)
        .collect();
    check(&onsets, &expected);
    assert_eq!(wraps, (BEATS / LOOP_BEATS) as u16);
}

#[test]
fn metronome_stays_on_the_sample_through_loop_wraps() {
    let (onsets, wraps) = play(&project(false, true));
    let expected: Vec<usize> = (0..=BEATS).map(beat_frame).collect();
    check(&onsets, &expected);
    assert_eq!(wraps, (BEATS / LOOP_BEATS) as u16);
}
