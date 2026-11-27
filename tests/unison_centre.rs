use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::core::params::Envelope;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{BLOCK, SAMPLE_RATE, amplitude, core_layer, midi};

const MIDI: u8 = 57;
const DETUNE_CENTS: f64 = 400.0;
const SETTLED: usize = 24_000;
const HEARD: usize = 24_000;
const LIFT_DB: f64 = 0.53;
const WITHIN_DB: f64 = 0.1;
const STEP_CENTS: f64 = 2.0;
const OUTER: (f64, f64) = (250.0, 450.0);
const INNER: (f64, f64) = (20.0, 250.0);

fn hz(cents: f64) -> f64 {
    440.0 * ((f64::from(MIDI) - 69.0) / 12.0 + cents / 1200.0).exp2()
}

fn project(copies: usize) -> Project {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.osc1.unison = copies;
    patch.osc1.detune = DETUNE_CENTS;
    patch.osc1.width = 0.0;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(0.25, patch))];
    let notes = vec![Note { pitch: MIDI, start: 0.0, length: 4.0, velocity: VELOCITY, articulation: Articulation::PLAIN }];
    track.clips.push(Clip { start: 0, length: 4 * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn render(copies: usize) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(&project(copies), None, &[], &[]);
    audio.play(0.0);
    let frames = SETTLED + HEARD;
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(frames);
    while out.len() < frames {
        let n = BLOCK.min(frames - out.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    out.split_off(SETTLED)
}

fn loudest(heard: &[f32], between: (f64, f64)) -> (f64, f64) {
    let steps = ((between.1 - between.0) / STEP_CENTS) as usize;
    (0..=steps).map(|step| between.0 + step as f64 * STEP_CENTS).map(|cents| (cents, amplitude(heard, hz(cents)))).fold((0.0, 0.0), |best, found| if found.1 > best.1 { found } else { best })
}

fn mirrored(heard: &[f32], between: (f64, f64)) -> [(f64, f64); 2] {
    [loudest(heard, (-between.1, -between.0)), loudest(heard, between)]
}

fn lift_db(over: f64, under: f64) -> f64 {
    20.0 * (over / under).log10()
}

#[test]
fn the_centre_copy_of_an_odd_stack_sits_half_a_decibel_over_each_side() {
    let heard = render(3);
    let centre = amplitude(&heard, hz(0.0));
    for (cents, side) in mirrored(&heard, OUTER) {
        let lift = lift_db(centre, side);
        assert!((lift - LIFT_DB).abs() < WITHIN_DB, "wanted the centre copy {LIFT_DB} dB over the side at {cents:+.0} cents, got {lift:+.2} dB");
    }
}

#[test]
fn an_even_stack_lifts_its_two_inner_copies_over_the_outer_pair() {
    let heard = render(4);
    let [inner_low, inner_high] = mirrored(&heard, INNER);
    let [outer_low, outer_high] = mirrored(&heard, OUTER);
    for (inner, outer) in [(inner_low, outer_low), (inner_high, outer_high)] {
        let lift = lift_db(inner.1, outer.1);
        assert!((lift - LIFT_DB).abs() < WITHIN_DB, "wanted the inner copy at {:+.0} cents {LIFT_DB} dB over the outer at {:+.0}, got {lift:+.2} dB", inner.0, outer.0);
    }
}
