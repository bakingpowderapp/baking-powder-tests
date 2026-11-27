use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::core::params::Envelope;
use bp::project::{Aim, Clip, LaneLfo, LfoShape, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{CORE_ROW, SAMPLE_RATE, WINDOW, core_layer, midi};

const BLOCK: usize = 256;
const SECONDS: usize = 4;
const RATE_HZ: f64 = 4.0;
const SMOOTH: f32 = 0.06;
const JUMP: f32 = 0.3;
const RANGE: f32 = 0.3;

fn project(shape: LfoShape) -> Project {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.osc1.level = 0.5;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch))];
    track.automation.lfos.push(LaneLfo { amount: 1.0, shape, sync: false, rate_hz: RATE_HZ, ..LaneLfo::blank(Aim::knob(CORE_ROW, "OSC 1", "LEVEL")) });
    let notes = vec![Note { pitch: 69, start: 0.0, length: 8.0, velocity: VELOCITY, articulation: Articulation::PLAIN }];
    track.clips.push(Clip { start: 0, length: 8 * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn render(project: &Project) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(project, None, &[], &[]);
    audio.play(0.0);
    let frames = SECONDS * SAMPLE_RATE as usize;
    let (mut left, mut right) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(frames);
    while out.len() < frames {
        let n = BLOCK.min(frames - out.len());
        renderer.render(&mut left[..n], &mut right[..n]);
        out.extend_from_slice(&left[..n]);
    }
    out
}

fn envelope(samples: &[f32]) -> Vec<f32> {
    let peaks: Vec<f32> = samples.as_chunks::<WINDOW>().0.iter().map(|window| window.iter().fold(0.0, |peak, s| s.abs().max(peak))).collect();
    let loudest = peaks.iter().copied().fold(0.0, f32::max);
    peaks.iter().map(|peak| peak / loudest).collect()
}

fn largest_step(envelope: &[f32]) -> (usize, f32) {
    envelope.windows(2).enumerate().map(|(at, pair)| (at, (pair[1] - pair[0]).abs())).fold((0, 0.0), |worst, (at, step)| if step > worst.1 { (at, step) } else { worst })
}

#[test]
fn a_walk_wanders_without_ever_jumping() {
    let envelope = envelope(&render(&project(LfoShape::Walk)));
    let (lowest, highest) = envelope.iter().skip(1).fold((1.0f32, 0.0f32), |(lo, hi), level| (lo.min(*level), hi.max(*level)));
    assert!(highest - lowest > RANGE, "wanted the walk to move the level more than {RANGE} over {SECONDS} seconds, got {lowest:.3} to {highest:.3}");
    let (at, step) = largest_step(&envelope[1..]);
    assert!(step < SMOOTH, "wanted the walk to move under {SMOOTH} between neighbouring windows, got {step:.3} at frame {}", (at + 1) * WINDOW);
}

#[test]
fn random_jumps_where_walk_eases() {
    let envelope = envelope(&render(&project(LfoShape::Random)));
    let (at, step) = largest_step(&envelope[1..]);
    assert!(step > JUMP, "wanted random to jump more than {JUMP} between neighbouring windows somewhere in {SECONDS} seconds, got {step:.3} at most, at frame {}", (at + 1) * WINDOW);
}

#[test]
fn a_walk_renders_the_same_bytes_twice() {
    let project = project(LfoShape::Walk);
    let (first, second) = (render(&project), render(&project));
    let frame = first.iter().zip(&second).position(|(a, b)| a.to_bits() != b.to_bits());
    assert_eq!(frame, None, "wanted a walk to draw the same levels on every render, the two part at frame {frame:?}");
}
