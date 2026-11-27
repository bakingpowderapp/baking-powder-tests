use bp::arp::Arp;
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params as Patch;
use bp::fx::params::Utility;
use bp::fx::{Kind, Params, Slot};
use bp::project::{Project, Row, VELOCITY};
use support::{AUDIBLE, CHORD, SAMPLE_RATE, core_layer, struck};

const MAX_FRAMES: usize = 4096;
const RAGGED: [usize; 12] = [1, 7, 64, 3, 512, 128, 4096, 1, 33, 900, 17, 256];
const UNIFORM: usize = 256;
const SECONDS: f64 = 4.0;
const WIDTH: f64 = 1.6;
const COPIES: usize = 3;
const DETUNE_CENTS: f64 = 12.0;

fn spread() -> Patch {
    let mut patch = Patch::DEFAULT;
    patch.osc1.unison = COPIES;
    patch.osc1.detune = DETUNE_CENTS;
    patch.osc1.alternate = true;
    patch.phase_random = 0.0;
    patch
}

fn project() -> Project {
    let widened = Slot { params: Params::Utility(Utility { width: WIDTH, ..Utility::DEFAULT }), ..Slot::new(Kind::Utility) };
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![struck(Arp::DEFAULT), Row::Core(core_layer(0.5, spread())), Row::Effect(widened)];
    project
}

fn held(sizes: impl Iterator<Item = usize>) -> [Vec<f32>; 2] {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, MAX_FRAMES, Seer::Inline);
    audio.sync(&project(), None, &[], &[]);
    for midi in CHORD {
        audio.note_on(0, midi, VELOCITY);
    }
    let total = (SAMPLE_RATE * SECONDS) as usize;
    let (mut left, mut right) = (Vec::with_capacity(total + MAX_FRAMES), Vec::with_capacity(total + MAX_FRAMES));
    let (mut l, mut r) = (vec![0.0; MAX_FRAMES], vec![0.0; MAX_FRAMES]);
    for n in sizes {
        if left.len() >= total {
            break;
        }
        renderer.render(&mut l[..n], &mut r[..n]);
        left.extend_from_slice(&l[..n]);
        right.extend_from_slice(&r[..n]);
    }
    left.truncate(total);
    right.truncate(total);
    [left, right]
}

#[test]
fn block_size_never_changes_a_widened_chord() {
    let ragged = held(RAGGED.iter().copied().cycle());
    let uniform = held(std::iter::repeat(UNIFORM));
    let side = ragged[0].iter().zip(&ragged[1]).map(|(l, r)| (l - r).abs()).fold(0.0f32, f32::max);
    assert!(side > AUDIBLE, "wanted the chord to differ between its channels before WIDTH {WIDTH} widens it, got a side of {side}");
    for (channel, name) in ["left", "right"].into_iter().enumerate() {
        let divergence = ragged[channel].iter().zip(&uniform[channel]).position(|(a, b)| a.to_bits() != b.to_bits());
        let worst = ragged[channel].iter().zip(&uniform[channel]).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        assert!(divergence.is_none(), "the widened chord's {name} channel: ragged and uniform blocks diverge at frame {divergence:?}, worst delta {worst}");
    }
}
