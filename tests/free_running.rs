use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::core::params::Envelope;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{AUDIBLE, BLOCK, FRAMES_PER_BEAT, SAMPLE_RATE, core_layer, midi};

const MIDI: u8 = 70;
const BEATS: f64 = 2.5;
const AGAIN_BEAT: f64 = 1.5;
const SETTLE: usize = 480;
const SAME: f32 = 1e-4;
const APART: f32 = 0.05;

fn project(free: bool, notes: &[(f32, f32)]) -> Project {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.osc1.free = free;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(0.5, patch))];
    let notes = notes.iter().map(|&(start, length)| Note { pitch: MIDI, start, length, velocity: VELOCITY, articulation: Articulation::PLAIN }).collect();
    track.clips.push(Clip { start: 0, length: 4 * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn render(project: &Project) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(project, None, &[], &[]);
    audio.play(0.0);
    let frames = (BEATS * FRAMES_PER_BEAT) as usize;
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(frames);
    while out.len() < frames {
        let n = BLOCK.min(frames - out.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    out
}

fn again(samples: &[f32]) -> &[f32] {
    let from = (AGAIN_BEAT * FRAMES_PER_BEAT) as usize + SETTLE;
    &samples[from..samples.len() - SETTLE]
}

fn worst(a: &[f32], b: &[f32]) -> (usize, f32) {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).enumerate().fold((0, 0.0), |worst, (frame, gap)| if gap > worst.1 { (frame, gap) } else { worst })
}

fn unbroken() -> Vec<f32> {
    render(&project(false, &[(0.0, BEATS as f32)]))
}

#[test]
fn a_repeated_note_running_free_picks_up_the_unbroken_wave() {
    let unbroken = unbroken();
    let repeated = render(&project(true, &[(0.0, 1.0), (AGAIN_BEAT as f32, 1.0)]));
    assert!(again(&repeated).iter().any(|sample| sample.abs() > AUDIBLE), "the repeated note is silent");
    let (frame, gap) = worst(again(&unbroken), again(&repeated));
    let at = (AGAIN_BEAT * FRAMES_PER_BEAT) as usize + SETTLE + frame;
    assert!(gap < SAME, "wanted the free-running repeat to follow the unbroken note's wave, worst at frame {at}, off by {gap:.2e}");
}

#[test]
fn a_repeated_note_starts_its_wave_afresh_unless_free() {
    let unbroken = unbroken();
    let repeated = render(&project(false, &[(0.0, 1.0), (AGAIN_BEAT as f32, 1.0)]));
    let (frame, gap) = worst(again(&unbroken), again(&repeated));
    let at = (AGAIN_BEAT * FRAMES_PER_BEAT) as usize + SETTLE + frame;
    assert!(gap > APART, "wanted a repeat that is not free to start its own wave apart from the unbroken note's, closest worst gap {gap:.2e} at frame {at}");
}
