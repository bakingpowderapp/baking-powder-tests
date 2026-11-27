use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params as CoreParams;
use bp::core::params::Envelope;
use bp::fx::params::{Delay, DelayMode, Reverb};
use bp::fx::{Kind, Params, Slot};
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{BLOCK, SAMPLE_RATE, core_layer, midi};

const HEARD: f32 = 1e-6;
const SECONDS: f64 = 3.0;

fn delay(forward: f64, reverse: f64) -> Row {
    Row::Effect(Slot { params: Params::Delay(Delay { mode: DelayMode::Clean, time_l_ms: 500.0, time_r_ms: 500.0, feedback: 0.0, forward, reverse, ..Delay::DEFAULT }), ..Slot::new(Kind::Delay) })
}

fn reverb(forward: f64, reverse: f64) -> Row {
    Row::Effect(Slot { params: Params::Reverb(Reverb { decay_s: 2.0, mod_depth: 0.0, forward, reverse, ..Reverb::DEFAULT }), ..Slot::new(Kind::Reverb) })
}

fn chained(ahead: Row) -> Project {
    let mut patch = CoreParams::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    let mut project = Project::new();
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(0.1, patch)), ahead, delay(0.0, 1.0)];
    track.clips = vec![Clip { start: 0, length: 8 * TICKS_PER_BEAT, label: String::new(), notes: vec![Note { pitch: 69, start: 4.0, length: 0.5, velocity: VELOCITY, articulation: Articulation::PLAIN }], file: None }];
    project
}

fn play_through(project: &Project) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, None, &[], &[]);
    audio.play(0.0);
    let frames = (SECONDS * SAMPLE_RATE) as usize;
    let mut out = Vec::with_capacity(frames);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    while out.len() < frames {
        let n = BLOCK.min(frames - out.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    out
}

fn energy(out: &[f32], from_seconds: f64, to_seconds: f64) -> f32 {
    out[(from_seconds * SAMPLE_RATE) as usize..(to_seconds * SAMPLE_RATE) as usize].iter().map(|x| x * x).sum()
}

#[test]
fn a_forward_tail_ahead_in_the_rack_reaches_the_reversed_delay() {
    let heard = play_through(&chained(reverb(1.0, 0.0)));
    let swell = energy(&heard, 1.8, 2.0);
    assert!(swell > HEARD, "the reverb tail after the note should be swelled in ahead of it by the reversed delay below it at 1.8s..2.0s; got energy {swell}");
}
