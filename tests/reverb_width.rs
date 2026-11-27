use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::fx::{self, Kind, Slot};
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{AUDIBLE, BLOCK, FRAMES_PER_BEAT, SAMPLE_RATE, core_layer, midi};

const BEATS: f64 = 3.0;
const NOTE_BEATS: f32 = 0.25;
const MIDDLE_HELD: f32 = 1e-5;
const SIDE_DB: f64 = 0.01;

fn project(width: f64) -> Project {
    let mut reverb = Slot::new(Kind::Reverb);
    if let fx::Params::Reverb(p) = &mut reverb.params {
        p.width = width;
    }
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(0.25, Params::DEFAULT)), Row::Effect(reverb)];
    let notes = vec![Note { pitch: 60, start: 0.0, length: NOTE_BEATS, velocity: VELOCITY, articulation: Articulation::PLAIN }];
    track.clips.push(Clip { start: 0, length: 4 * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn render(project: &Project) -> (Vec<f32>, Vec<f32>) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(project, None, &[], &[]);
    audio.play(0.0);
    let frames = (BEATS * FRAMES_PER_BEAT) as usize;
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let (mut left, mut right) = (Vec::with_capacity(frames), Vec::with_capacity(frames));
    while left.len() < frames {
        let n = BLOCK.min(frames - left.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        left.extend_from_slice(&l[..n]);
        right.extend_from_slice(&r[..n]);
    }
    (left, right)
}

fn mid_side((left, right): &(Vec<f32>, Vec<f32>)) -> (Vec<f64>, Vec<f64>) {
    left.iter().zip(right).map(|(l, r)| ((f64::from(*l) + f64::from(*r)) / 2.0, (f64::from(*l) - f64::from(*r)) / 2.0)).unzip()
}

fn rms(samples: &[f64]) -> f64 {
    (samples.iter().map(|sample| sample * sample).sum::<f64>() / samples.len() as f64).sqrt()
}

#[test]
fn at_width_zero_the_tail_is_mono() {
    let (left, right) = render(&project(0.0));
    assert!(left.iter().any(|sample| sample.abs() > AUDIBLE), "the reverb is silent");
    let frame = left.iter().zip(&right).position(|(l, r)| l.to_bits() != r.to_bits());
    assert_eq!(frame, None, "wanted the left and right of a tail at width 0 to be the same samples, they part at frame {frame:?}");
}

#[test]
fn past_one_the_side_grows_with_the_width_and_the_middle_holds() {
    let (middle, side) = mid_side(&render(&project(1.0)));
    assert!(rms(&side) > 1e-3, "the tail at width 1 has no side to widen, rms {:.6}", rms(&side));
    for width in [1.5, 2.0, 4.0] {
        let (wider_middle, wider_side) = mid_side(&render(&project(width)));
        let (frame, drift) = middle.iter().zip(&wider_middle).map(|(a, b)| (a - b).abs()).enumerate().fold((0, 0.0), |worst, (frame, drift)| if drift > worst.1 { (frame, drift) } else { worst });
        assert!(drift < f64::from(MIDDLE_HELD), "wanted the middle at width {width} to match width 1, worst at frame {frame}, off by {drift:.2e}");
        let gain = 20.0 * (rms(&wider_side) / rms(&side)).log10();
        let wanted = 20.0 * width.log10();
        assert!((gain - wanted).abs() < SIDE_DB, "wanted the side at width {width} {wanted:.2} dB over width 1, got {gain:.4} dB");
    }
}
