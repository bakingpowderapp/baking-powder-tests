use bp::arp::Arp;
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::params::Envelope;
use bp::core::{FilterMode, FilterShape, Params as Patch, Slope};
use bp::project::{Project, Row, VELOCITY};
use support::{AUDIBLE, BLOCK, SAMPLE_RATE, core_layer, frames, peak_of, struck};

const A2: u8 = 45;
const HELD_SECONDS: f64 = 2.0;
const LAST_SECONDS: f64 = 0.25;
const CUTOFF_HZ: f64 = 1_000.0;
const RESONANCE: f64 = 0.5;
const FEEDBACK: f64 = 0.95;
const LEVEL: f64 = 0.25;
const ROOM: f32 = 6.0;
const SHAPES: [(FilterShape, &str); 2] = [(FilterShape::BandPass, "band-pass"), (FilterShape::HighPass, "high-pass")];

fn patch(shape: FilterShape, feedback: f64) -> Patch {
    let mut patch = Patch::DEFAULT;
    patch.phase_random = 0.0;
    patch.filter.on = true;
    patch.filter.mode = FilterMode::Svf;
    patch.filter.shape = shape;
    patch.filter.slope = Slope::Db12;
    patch.filter.cutoff_hz = CUTOFF_HZ;
    patch.filter.resonance = RESONANCE;
    patch.filter.feedback = feedback;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, velocity: 0.0, ..Envelope::DEFAULT };
    patch
}

fn held(shape: FilterShape, feedback: f64) -> Vec<f32> {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![struck(Arp::DEFAULT), Row::Core(core_layer(LEVEL, patch(shape, feedback)))];
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(&project, None, &[], &[]);
    audio.note_on(0, A2, VELOCITY);
    let total = frames(HELD_SECONDS);
    let mut out = Vec::with_capacity(total);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    while out.len() < total {
        let n = BLOCK.min(total - out.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    out
}

#[test]
fn full_feedback_colours_a_held_note_without_running_away() {
    for (shape, name) in SHAPES {
        let plain = held(shape, 0.0);
        let fed = held(shape, FEEDBACK);
        let last = frames(HELD_SECONDS - LAST_SECONDS);
        let (plain_peak, fed_peak) = (peak_of(&plain), peak_of(&fed));
        assert!(plain_peak > AUDIBLE, "wanted the {name} note heard without feedback, got a peak of {plain_peak}");
        assert!(fed_peak < ROOM * plain_peak, "wanted FEEDBACK {FEEDBACK} on the {name} filter to lift the note by under {ROOM} times its peak of {plain_peak} without it, got a peak of {fed_peak}");
        let (plain_end, fed_end) = (peak_of(&plain[last..]), peak_of(&fed[last..]));
        assert!(fed_end > plain_end / ROOM && fed_end < ROOM * plain_end, "wanted the {name} note still sounding near its level after {HELD_SECONDS} s of FEEDBACK {FEEDBACK}, {plain_end} without it, got {fed_end}");
    }
}
