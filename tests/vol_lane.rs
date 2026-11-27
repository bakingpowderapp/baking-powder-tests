use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::core::params::Envelope;
use bp::project::{Aim, Clip, Curve, Node, Note, Project, Row, TICKS_PER_BAR, TICKS_PER_BEAT, VELOCITY, gain_to_position};
use support::{AUDIBLE, BLOCK, HALF, SAMPLE_RATE, core_layer, midi, peak_of};

const BEATS: usize = 3;
const FRAMES_PER_BEAT: usize = 24_000;
fn curve(nodes: Vec<Node>) -> Curve {
    Curve { aim: Aim::Gain, lfo: None, nodes }
}

fn project(gain: f32, nodes: Vec<Node>) -> Project {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.osc1.level = 0.5;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch))];
    track.gain = gain;
    let notes = vec![Note { pitch: 69, start: 0.0, length: BEATS as f32, velocity: VELOCITY, articulation: Articulation::PLAIN }];
    let curves = if nodes.is_empty() { Vec::new() } else { vec![curve(nodes)] };
    track.automation.curves = curves;
    track.clips.push(Clip { start: 0, length: TICKS_PER_BAR, label: String::new(), notes, file: None });
    project
}

fn render(project: &Project) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(project, None, &[], &[]);
    audio.play(0.0);
    let frames = BEATS * FRAMES_PER_BEAT;
    let (mut left, mut right) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(frames);
    while out.len() < frames {
        let n = BLOCK.min(frames - out.len());
        renderer.render(&mut left[..n], &mut right[..n]);
        out.extend_from_slice(&left[..n]);
    }
    out
}

#[test]
fn the_volume_lane_is_the_fader() {
    let fader = render(&project(HALF, Vec::new()));
    let lane = render(&project(1.0, vec![Node { tick: 0, travel: gain_to_position(HALF), bend: 0.0 }]));
    assert!(peak_of(&fader) > AUDIBLE, "the note is silent");
    if let Some(frame) = fader.iter().zip(&lane).position(|(a, b)| a.to_bits() != b.to_bits()) {
        panic!("a flat volume lane at the {HALF} fader position differs from the fader at {HALF}, first at frame {frame}");
    }
    let nodes = vec![Node { tick: 0, travel: gain_to_position(1.0), bend: 0.0 }, Node { tick: 2 * TICKS_PER_BEAT, travel: 0.0, bend: 0.0 }];
    let faded = render(&project(1.0, nodes));
    let early = peak_of(&faded[4_800..24_000]);
    let late = peak_of(&faded[2 * FRAMES_PER_BEAT + 4_800..]);
    assert!(early > AUDIBLE, "the ramp's first beat is silent, peak {early}");
    assert_eq!(late, 0.0, "past the ramp's last node the volume lane holds zero, still heard peak {late}");
}
