use bp::arp::Arp;
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::project::{Project, Row, VELOCITY};
use support::{BLOCK, GAP, SAMPLE_RATE, SILENT, WITHIN, core_layer, midi};

const STEPS: usize = 240;
const STEP_FRAMES: usize = 12_000;
fn project(bpm: f64) -> Project {
    let mut patch = Params::DEFAULT;
    patch.amp.attack_ms = 1.0;
    patch.amp.release_ms = 1.0;
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp { on: true, sync: false, time_ms: 250.0, gate: Arp::MIN_GATE, ..Arp::DEFAULT }), Row::Core(core_layer(1.0, patch))];
    project.bpm = bpm;
    project
}

fn onsets(project: &Project) -> Vec<usize> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, None, &[], &[]);
    audio.note_on(0, 60, VELOCITY);
    audio.play(0.0);
    let total = (STEPS as f64 + 0.5) as usize * STEP_FRAMES;
    let (mut at, mut frame, mut quiet) = (Vec::new(), 0usize, GAP);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    while frame < total {
        renderer.render(&mut l, &mut r);
        for &sample in &l {
            if sample.abs() < SILENT {
                quiet += 1;
            } else {
                if quiet >= GAP {
                    at.push(frame);
                }
                quiet = 0;
            }
            frame += 1;
        }
    }
    at
}

fn check(onsets: &[usize], bpm: f64) {
    let expected = |step: usize| step * STEP_FRAMES;
    let missed: Vec<usize> = (0..STEPS).filter(|&step| !onsets.iter().any(|&at| at.abs_diff(expected(step)) <= WITHIN)).collect();
    let stray: Vec<usize> = onsets.iter().copied().filter(|&at| !(0..STEPS).any(|step| at.abs_diff(expected(step)) <= WITHIN)).collect();
    assert!(missed.is_empty() && stray.is_empty(), "at {bpm} bpm the free arp missed steps {missed:?} and strayed to frames {stray:?}");
}

#[test]
fn a_free_arp_holds_its_milliseconds_at_any_tempo() {
    let slow = onsets(&project(89.0));
    let fast = onsets(&project(140.0));
    check(&slow, 89.0);
    check(&fast, 140.0);
    assert_eq!(slow, fast, "the same free arp landed on different frames at 89 and 140 bpm");
}
