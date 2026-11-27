use bp::arp::Arp;
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::division::Division;
use bp::project::{Project, Row};
use support::{BLOCK, FRAMES_PER_BEAT, Onsets, SAMPLE_RATE, WITHIN, beat_frame, core_layer, midi};

const BEATS: usize = 240;
fn play(project: &Project, held: Option<u8>) -> Vec<usize> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, None, &[], &[]);
    if let Some(midi) = held {
        audio.note_on(0, midi, 127);
    }
    audio.play(0.0);
    let mut onsets = Onsets::default();
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let total = ((BEATS as f64 + 0.5) * FRAMES_PER_BEAT) as usize;
    while onsets.frame < total {
        renderer.render(&mut l, &mut r);
        onsets.feed(&l);
    }
    onsets.at
}

fn check(onsets: &[usize], expected: impl Fn(usize) -> usize) {
    let missed: Vec<usize> = (0..=BEATS).filter(|&beat| !onsets.iter().any(|&at| at.abs_diff(expected(beat)) <= WITHIN)).collect();
    let stray: Vec<f64> = onsets.iter().filter(|&&at| !(0..=BEATS).any(|beat| at.abs_diff(expected(beat)) <= WITHIN)).map(|&at| at as f64 / FRAMES_PER_BEAT).collect();
    assert!(missed.is_empty() && stray.is_empty(), "missed beats {missed:?}, stray onsets at beats {stray:?}");
}

#[test]
fn metronome_on_beat_at_two_minutes() {
    let mut project = Project::new();
    project.metronome = true;
    check(&play(&project, None), beat_frame);
}

#[test]
fn quarter_arp_on_beat_at_two_minutes() {
    let mut patch = Params::DEFAULT;
    patch.amp.attack_ms = 1.0;
    patch.amp.release_ms = 1.0;
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp { on: true, rate: Division::Quarter, gate: Arp::MIN_GATE, ..Arp::DEFAULT }), Row::Core(core_layer(1.0, patch))];
    check(&play(&project, Some(60)), beat_frame);
}
