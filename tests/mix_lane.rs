use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::core::params::Envelope;
use bp::fx::{Kind, Slot};
use bp::project::{Aim, Clip, Curve, Node, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{BLOCK, CORE_ROW, SAMPLE_RATE, core_layer, midi, worst_delta};

const SECONDS: usize = 2;
const SETTLE: usize = 24_000;
const TOLERANCE: f32 = 1e-3;
const WET: f32 = 0.4;

fn project(delay: Option<f32>, lane: Option<f32>) -> Project {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.osc1.level = 0.5;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch))];
    if let Some(wet) = delay {
        track.rack.push(Row::Effect(Slot { wet: f64::from(wet), ..Slot::new(Kind::Delay) }));
    }
    let notes = vec![Note { pitch: 69, start: 0.0, length: 8.0, velocity: VELOCITY, articulation: Articulation::PLAIN }];
    let curves = lane.map_or(Vec::new(), |travel| {
        vec![Curve { aim: Aim::Wet(CORE_ROW + 1), lfo: None, nodes: vec![Node { tick: 0, travel, bend: 0.0 }] }]
    });
    track.automation.curves = curves;
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

#[test]
fn a_mix_lane_rides_the_rows_wet_knob() {
    let knob = render(&project(Some(WET), None));
    let lane = render(&project(Some(1.0), Some(WET)));
    let worst = worst_delta(&knob[SETTLE..], &lane[SETTLE..]);
    assert!(worst < TOLERANCE, "a flat MIX lane at {WET} over a wet knob at 1.0 should match the knob at {WET} within {TOLERANCE} after a beat, worst delta {worst}");
    let bare = render(&project(None, None));
    let dried = render(&project(Some(1.0), Some(0.0)));
    let dry_worst = worst_delta(&bare[SETTLE..], &dried[SETTLE..]);
    assert!(dry_worst < TOLERANCE, "a MIX lane holding zero should leave the delay unheard within {TOLERANCE} after a beat, worst delta {dry_worst}");
}
