use bp::arp::Arp;
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::core::Params;
use bp::core::params::Envelope;
use bp::project::{Aim, Clip, Curve, Node, Project, Row, TICKS_PER_BAR};
use support::{AUDIBLE, CORE_ROW, SAMPLE_RATE, core_layer, struck};

const MAX_FRAMES: usize = 1024;
const UNIFORM: usize = 256;
const RAGGED: [usize; 12] = [1, 7, 64, 3, 512, 128, 256, 1, 33, 900, 17, 256];
const BAR_FRAMES: usize = 96_000;
const PAST_THE_BAR: usize = 24_037;
const REST_LEVEL: f64 = 0.2;
const UNITY: f32 = 0.75;

fn there_and_back(aim: Aim, rest: f32, far: f32) -> Curve {
    let nodes = vec![Node { tick: 0, travel: rest, bend: 0.0 }, Node { tick: TICKS_PER_BAR / 2, travel: far, bend: 0.0 }, Node { tick: TICKS_PER_BAR, travel: rest, bend: 0.0 }];
    Curve { aim, lfo: None, nodes }
}

fn project() -> Project {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.osc1.level = REST_LEVEL;
    patch.phase_random = 0.0;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, velocity: 0.0, ..Envelope::DEFAULT };
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![struck(Arp::DEFAULT), Row::Core(core_layer(1.0, patch))];
    track.automation.curves = vec![there_and_back(Aim::knob(CORE_ROW, "OSC 1", "LEVEL"), REST_LEVEL as f32, 1.0), there_and_back(Aim::Gain, UNITY, UNITY / 2.0)];
    track.clips.push(Clip::chord(0, String::new(), &[60, 64, 67]));
    project
}

fn open() -> (Audio, Renderer) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, MAX_FRAMES, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(&project(), None, &[], &[]);
    (audio, renderer)
}

fn pass(audio: &mut Audio, renderer: &mut Renderer, total: usize, sizes: impl Iterator<Item = usize>) -> Vec<f32> {
    audio.play(0.0);
    let mut out = Vec::with_capacity(total + MAX_FRAMES);
    let (mut l, mut r) = (vec![0.0; MAX_FRAMES], vec![0.0; MAX_FRAMES]);
    for size in sizes {
        if out.len() >= total {
            break;
        }
        let n = size.min(total - out.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    out
}

fn apart(got: &[f32], wanted: &[f32]) -> Option<usize> {
    got.iter().zip(wanted).position(|(a, b)| a.to_bits() != b.to_bits())
}

#[test]
fn a_second_play_steps_its_lanes_where_the_first_did() {
    let (mut audio, mut renderer) = open();
    let first = pass(&mut audio, &mut renderer, BAR_FRAMES + PAST_THE_BAR, std::iter::repeat(UNIFORM));
    let second = pass(&mut audio, &mut renderer, BAR_FRAMES, std::iter::repeat(UNIFORM));
    assert!(first.iter().any(|sample| sample.abs() > AUDIBLE), "wanted the chord heard under its level and volume lanes");
    let frame = apart(&second, &first[..BAR_FRAMES]);
    assert!(frame.is_none(), "wanted a second play, begun {} frames into a lane step, to render the bar as the first play did, as an export from the bar's start does, got a difference at frame {frame:?}", (BAR_FRAMES + PAST_THE_BAR) % 64);
}

#[test]
fn ragged_blocks_step_the_lanes_where_uniform_ones_do() {
    let (mut audio, mut renderer) = open();
    let uniform = pass(&mut audio, &mut renderer, BAR_FRAMES, std::iter::repeat(UNIFORM));
    let (mut audio, mut renderer) = open();
    let ragged = pass(&mut audio, &mut renderer, BAR_FRAMES, RAGGED.iter().copied().cycle());
    let frame = apart(&ragged, &uniform);
    assert!(frame.is_none(), "wanted a level lane and a volume lane to render the same bytes in ragged blocks as in blocks of {UNIFORM}, got a difference at frame {frame:?}");
}
