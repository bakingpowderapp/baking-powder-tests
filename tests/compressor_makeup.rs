use bp::arp::{Arp, Articulation};
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::core::Params as Patch;
use bp::core::params::Envelope;
use bp::fx::params::Compressor;
use bp::fx::{Kind, Params, Slot};
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{AUDIBLE, BLOCK, SAMPLE_RATE, core_layer, frames, peak_of, struck, worst_delta};

const A3: u8 = 57;
const HELD_BEATS: i32 = 6;
const PASS_SECONDS: f64 = 4.0;
const THRESHOLD_DB: f64 = -30.0;
const RATIO: f64 = 4.0;
const EARLY: (f64, f64) = (0.5, 1.0);
const SETTLED: (f64, f64) = (2.5, 3.0);
const SETTLED_WITHIN_DB: f64 = 1.0;
const SAME_WITHIN: f32 = 1e-3;

fn sine() -> Patch {
    let mut patch = Patch::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.phase_random = 0.0;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, velocity: 0.0, ..Envelope::DEFAULT };
    patch
}

fn project() -> Project {
    let compressor = Slot { params: Params::Compressor(Compressor { threshold_db: THRESHOLD_DB, ratio: RATIO, ..Compressor::DEFAULT }), ..Slot::new(Kind::Compressor) };
    let notes = vec![Note { pitch: A3, start: 0.0, length: HELD_BEATS as f32, velocity: VELOCITY, articulation: Articulation::PLAIN }];
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![struck(Arp::DEFAULT), Row::Core(core_layer(1.0, sine())), Row::Effect(compressor)];
    project.tracks[0].clips.push(Clip { start: 0, length: HELD_BEATS * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn open() -> (Audio, Renderer) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(&project(), None, &[], &[]);
    (audio, renderer)
}

fn pass(audio: &mut Audio, renderer: &mut Renderer) -> Vec<f32> {
    audio.play(0.0);
    let total = frames(PASS_SECONDS);
    let mut out = Vec::with_capacity(total);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    while out.len() < total {
        let n = BLOCK.min(total - out.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    out
}

fn level_db(out: &[f32], (from, to): (f64, f64)) -> f64 {
    20.0 * f64::from(peak_of(&out[frames(from)..frames(to)])).log10()
}

#[test]
fn auto_makeup_is_settled_within_the_first_second() {
    let (mut audio, mut renderer) = open();
    let out = pass(&mut audio, &mut renderer);
    assert!(peak_of(&out) > AUDIBLE, "wanted the compressed tone heard, got a peak of {}", peak_of(&out));
    let (early, settled) = (level_db(&out, EARLY), level_db(&out, SETTLED));
    assert!((settled - early).abs() < SETTLED_WITHIN_DB, "wanted the compressed tone between {} and {} s within {SETTLED_WITHIN_DB} dB of its level between {} and {} s, the AUTO makeup already settled, got {early:.2} dB against {settled:.2} dB", EARLY.0, EARLY.1, SETTLED.0, SETTLED.1);
}

#[test]
fn a_second_play_starts_as_the_first_did() {
    let (mut audio, mut renderer) = open();
    let first = pass(&mut audio, &mut renderer);
    let second = pass(&mut audio, &mut renderer);
    let heard = frames(SETTLED.1);
    let worst = worst_delta(&first[..heard], &second[..heard]) / peak_of(&first);
    assert!(worst < SAME_WITHIN, "wanted the second play of the compressed tone to render as the first did, as an export of it would, every sample within {SAME_WITHIN} of the peak, got one {worst:.4} of the peak off");
}
