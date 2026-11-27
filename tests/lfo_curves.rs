use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::core::params::Envelope;
use bp::project::{Aim, Clip, Curve, LaneLfo, LfoField, Node, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{BLOCK, CORE_ROW, SAMPLE_RATE, WINDOW, core_layer, midi, worst_delta};

const SECONDS: usize = 4;
const SETTLE: usize = 24_000;
const TOLERANCE: f32 = 1e-4;

fn flat(field: LfoField, travel: f32) -> Curve {
    Curve { aim: Aim::knob(CORE_ROW, "OSC 1", "LEVEL"), lfo: Some(field), nodes: vec![Node { tick: 0, travel, bend: 0.0 }] }
}

fn project(level: f64, lfos: Vec<LaneLfo>, curves: Vec<Curve>) -> Project {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.osc1.level = level;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch))];
    track.automation.lfos = lfos;
    let notes = vec![Note { pitch: 69, start: 0.0, length: 8.0, velocity: VELOCITY, articulation: Articulation::PLAIN }];
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

fn envelope(samples: &[f32]) -> Vec<f32> {
    let peaks: Vec<f32> = samples.as_chunks::<WINDOW>().0.iter().map(|window| window.iter().fold(0.0, |peak, s| s.abs().max(peak))).collect();
    let loudest = peaks.iter().copied().fold(0.0, f32::max);
    peaks.iter().map(|peak| peak / loudest).collect()
}

#[test]
fn an_amount_curve_wakes_a_silent_lfo() {
    let steady = envelope(&render(&project(0.5, Vec::new(), Vec::new())));
    let woken = envelope(&render(&project(0.5, Vec::new(), vec![flat(LfoField::Amount, 1.0)])));
    let from = SETTLE / 2 / WINDOW;
    let held = steady[from..].iter().copied().fold(1.0, f32::min);
    let trough = woken[from..].iter().copied().fold(1.0, f32::min);
    assert!(held > 0.85, "without a curve the level should hold steady, sank to {held:.3} of its peak");
    assert!(trough < 0.06, "an AMOUNT curve at full on a bare lane should swing the level to silence, only reached {trough:.3} of the peak");
}

#[test]
fn a_centre_curve_parks_the_level_where_the_knob_would() {
    let knob = render(&project(0.6, Vec::new(), Vec::new()));
    let lane = vec![LaneLfo { amount: 0.0, ..LaneLfo::blank(Aim::knob(CORE_ROW, "OSC 1", "LEVEL")) }];
    let parked = render(&project(0.9, lane, vec![flat(LfoField::Center, 0.6)]));
    let worst = worst_delta(&knob[SETTLE..], &parked[SETTLE..]);
    assert!(worst < TOLERANCE, "a CENTER curve at 0.6 over a knob at 0.9 should match the knob at 0.6 within {TOLERANCE} after a beat, worst delta {worst}");
}
