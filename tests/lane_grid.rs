use bp::arp::Arp;
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::core::params::Envelope;
use bp::fx::{Kind, Slot};
use bp::project::{Aim, Clip, Curve, LaneLfo, Node, Project, Row, TICKS_PER_BAR};
use support::{CORE_ROW, SAMPLE_RATE, core_layer, midi};

const SECONDS: usize = 4;
const LADDER: [usize; 2] = [256, 4096];
const SMALLEST: usize = 64;

fn ramp(aim: Aim, from: f32, to: f32) -> Curve {
    Curve { aim, lfo: None, nodes: vec![Node { tick: 0, travel: from, bend: 0.0 }, Node { tick: TICKS_PER_BAR, travel: to, bend: 0.0 }] }
}

fn project() -> Project {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.osc1.level = 0.5;
    patch.filter.on = true;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch)), Row::Effect(Slot::new(Kind::Delay))];
    track.automation.lfos.push(LaneLfo { amount: 0.6, ..LaneLfo::blank(Aim::knob(CORE_ROW, "FILTER", "CUTOFF")) });
    track.automation.curves = vec![ramp(Aim::knob(CORE_ROW, "OSC 1", "LEVEL"), 0.2, 1.0), ramp(Aim::Wet(CORE_ROW + 1), 1.0, 0.2), ramp(Aim::Gain, 0.75, 0.375)];
    track.clips.push(Clip::chord(0, String::new(), &[60, 64, 67]));
    project
}

fn render(project: &Project, block: usize) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, block, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(project, None, &[], &[]);
    audio.play(0.0);
    let frames = SECONDS * SAMPLE_RATE as usize;
    let (mut left, mut right) = (vec![0.0; block], vec![0.0; block]);
    let mut out = Vec::with_capacity(frames);
    while out.len() < frames {
        let n = block.min(frames - out.len());
        renderer.render(&mut left[..n], &mut right[..n]);
        out.extend_from_slice(&left[..n]);
    }
    out
}

#[test]
fn every_lane_renders_the_same_bytes_at_every_buffer() {
    let project = project();
    let reference = render(&project, SMALLEST);
    for block in LADDER {
        let other = render(&project, block);
        if let Some(frame) = reference.iter().zip(&other).position(|(a, b)| a.to_bits() != b.to_bits()) {
            panic!("wanted the same bytes at {block} frames as at {SMALLEST} with a level curve, a wet curve, a volume lane and an LFO riding, got a difference at frame {frame}");
        }
    }
}
