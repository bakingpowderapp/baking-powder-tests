use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::core::params::Envelope;
use bp::project::{Aim, Clip, Curve, Node, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{BLOCK, CORE_ROW, SAMPLE_RATE, core_layer, midi};

const BEATS: usize = 4;
const FRAMES_PER_BEAT: usize = 24_000;
const HALF_WINDOW: usize = 1_200;
const MID: usize = 2 * FRAMES_PER_BEAT;
const LATE: usize = 91_200;

fn project(bend: f32) -> Project {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch))];
    let notes = vec![Note { pitch: 69, start: 0.0, length: BEATS as f32, velocity: VELOCITY, articulation: Articulation::PLAIN }];
    let nodes = vec![Node { tick: 0, travel: 0.0, bend }, Node { tick: BEATS as i32 * TICKS_PER_BEAT, travel: 1.0, bend: 0.0 }];
    let curves = vec![Curve { aim: Aim::knob(CORE_ROW, "OSC 1", "LEVEL"), lfo: None, nodes }];
    track.automation.curves = curves;
    track.clips.push(Clip { start: 0, length: BEATS as i32 * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn midway(bend: f32) -> f32 {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(&project(bend), None, &[], &[]);
    audio.play(0.0);
    let frames = BEATS * FRAMES_PER_BEAT;
    let (mut left, mut right) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(frames);
    while out.len() < frames {
        let n = BLOCK.min(frames - out.len());
        renderer.render(&mut left[..n], &mut right[..n]);
        out.extend_from_slice(&left[..n]);
    }
    let peak = |at: usize| out[at - HALF_WINDOW..at + HALF_WINDOW].iter().fold(0.0f32, |peak, s| s.abs().max(peak));
    peak(MID) / peak(LATE)
}

#[test]
fn a_bend_pulls_the_ramp_early_or_holds_it_late() {
    let straight = midway(0.0);
    let early = midway(1.0);
    let late = midway(-1.0);
    assert!(straight > 0.3 && straight < 0.7, "wanted a straight ramp near half its end level at frame {MID}, got {straight:.3}");
    assert!(early > 0.85, "wanted a bend of 1.0 to reach nearly its end level by frame {MID}, got {early:.3}");
    assert!(late < 0.15, "wanted a bend of -1.0 to still be near silence at frame {MID}, got {late:.3}");
}
