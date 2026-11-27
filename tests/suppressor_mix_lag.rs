use bp::arp::{Arp, Articulation};
use bp::core::Params;
use bp::core::params::Envelope;
use bp::fx::params::{FreakSlope, Suppressor};
use bp::fx::{Kind, Slot};
use bp::project::{Aim, Clip, Curve, Node, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{beat_frame, core_layer, midi, song_render};

const SUPPRESSOR_ROW: usize = 2;
const STEP_BEAT: i32 = 2;
const BEATS: i32 = 4;
const PITCH: u8 = 57;
const DEEP: f64 = 10.0;
const SHARE: f32 = 0.1;
const TOLERANCE: usize = 150;
const SLOPES: [(FreakSlope, &str); 2] = [(FreakSlope::Linkwitz, "the Linkwitz slope"), (FreakSlope::Linear, "the linear-phase slope")];

fn suppressor(slope: FreakSlope) -> Row {
    let mut params = Suppressor::DEFAULT;
    params.band_count = 2;
    params.slope = slope;
    for band in &mut params.bands {
        band.depth = DEEP;
    }
    Row::Effect(Slot { params: bp::fx::Params::Suppressor(params), ..Slot::new(Kind::Suppressor) })
}

fn core() -> Row {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    Row::Core(core_layer(1.0, patch))
}

fn flat() -> Vec<Node> {
    vec![Node { tick: 0, travel: 0.0, bend: 0.0 }]
}

fn stepped() -> Vec<Node> {
    vec![Node { tick: 0, travel: 0.0, bend: 0.0 }, Node { tick: STEP_BEAT * TICKS_PER_BEAT - 1, travel: 0.0, bend: 0.0 }, Node { tick: STEP_BEAT * TICKS_PER_BEAT, travel: 1.0, bend: 0.0 }]
}

fn project(slope: FreakSlope, from_beat: f32, nodes: Vec<Node>) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), core(), suppressor(slope)];
    let notes = vec![Note { pitch: PITCH, start: from_beat, length: BEATS as f32 - from_beat, velocity: VELOCITY, articulation: Articulation::PLAIN }];
    track.clips.push(Clip { start: 0, length: BEATS * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    track.automation.curves = vec![Curve { aim: Aim::Wet(SUPPRESSOR_ROW), lfo: None, nodes }];
    project
}

fn total() -> usize {
    beat_frame(BEATS as usize)
}

fn arrival(signal: &[f32], what: &str) -> usize {
    let loudest = signal.iter().fold(0.0f32, |loudest, sample| loudest.max(sample.abs()));
    signal.iter().position(|sample| sample.abs() > loudest * SHARE).unwrap_or_else(|| panic!("wanted {what} heard"))
}

#[test]
fn a_mix_step_lands_with_the_audio_of_its_beat_on_every_crossover_slope() {
    for (slope, name) in SLOPES {
        let reference = song_render(&project(slope, STEP_BEAT as f32, flat()), total());
        let onset = arrival(&reference[0], &format!("{name}: the note starting at beat {STEP_BEAT}"));
        let held = song_render(&project(slope, 0.0, flat()), total());
        let mixed_in = song_render(&project(slope, 0.0, stepped()), total());
        let apart: Vec<f32> = mixed_in[0].iter().zip(&held[0]).map(|(a, b)| a - b).collect();
        let step = arrival(&apart, &format!("{name}: the MIX step at beat {STEP_BEAT}"));
        assert!(step.abs_diff(onset) <= TOLERANCE, "{name}: wanted the MIX step at beat {STEP_BEAT} heard where the audio of beat {STEP_BEAT} lands, frame {onset}, within {TOLERANCE} frames where a step read at the row's full latency would land a half window of 431 late, got it heard from frame {step}");
    }
}
