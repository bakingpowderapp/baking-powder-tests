use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::core::params::Envelope;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{BLOCK, SAMPLE_RATE, amplitude, core_layer, midi};

const MIDI: u8 = 57;
const COPIES: usize = 3;
const DETUNE_CENTS: f64 = 400.0;
const SETTLED: usize = 24_000;
const HEARD: usize = 24_000;
const CENTRE_LIFT_DB: f64 = 0.53;
const WITHIN_DB: f64 = 0.1;
const HALF: f64 = 0.5;
const SILENT_SHARE: f64 = 0.01;

fn hz(cents: f64) -> f64 {
    440.0 * ((f64::from(MIDI) - 69.0) / 12.0 + cents / 1200.0).exp2()
}

fn db(ratio: f64) -> f64 {
    20.0 * ratio.log10()
}

fn project(blend: f64) -> Project {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.osc1.unison = COPIES;
    patch.osc1.detune = DETUNE_CENTS;
    patch.osc1.width = 0.0;
    patch.osc1.blend = blend;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(0.25, patch))];
    let notes = vec![Note { pitch: MIDI, start: 0.0, length: 4.0, velocity: VELOCITY, articulation: Articulation::PLAIN }];
    track.clips.push(Clip { start: 0, length: 4 * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn render(blend: f64) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(&project(blend), None, &[], &[]);
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

fn partials(heard: &[f32]) -> (f64, [(f64, f64); 2]) {
    (amplitude(heard, hz(0.0)), [-DETUNE_CENTS, DETUNE_CENTS].map(|cents| (cents, amplitude(heard, hz(cents)))))
}

fn power(heard: &[f32]) -> f64 {
    let (centre, sides) = partials(heard);
    centre * centre + sides.iter().map(|(_, side)| side * side).sum::<f64>()
}

#[test]
fn the_blend_sets_the_detuned_copies_level_beside_the_centre() {
    let (centre, sides) = partials(&render(HALF));
    let wanted = CENTRE_LIFT_DB + db(1.0 / HALF);
    for (cents, side) in sides {
        let lift = db(centre / side);
        assert!((lift - wanted).abs() < WITHIN_DB, "wanted the centre copy {wanted:.2} dB over the side at {cents:+.0} cents with the blend at {HALF}, got {lift:+.2} dB");
    }
}

#[test]
fn the_blend_moves_the_tone_and_keeps_the_level() {
    let (equal, blended) = (power(&render(1.0)), power(&render(HALF)));
    let change = 10.0 * (blended / equal).log10();
    assert!(change.abs() < WITHIN_DB, "wanted the stack's power unchanged by the blend at {HALF}, got {change:+.2} dB");
}

#[test]
fn at_zero_the_centre_copy_sounds_alone() {
    let (centre, sides) = partials(&render(0.0));
    for (cents, side) in sides {
        assert!(side < centre * SILENT_SHARE, "wanted the side copy at {cents:+.0} cents silent at blend 0, got {:.3} of the centre's amplitude", side / centre);
    }
}
