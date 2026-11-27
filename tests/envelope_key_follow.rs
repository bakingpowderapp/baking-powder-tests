use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::core::params::Envelope;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{BLOCK, SAMPLE_RATE, core_layer, frames, midi, peak_of};

const C4: u8 = 60;
const C5: u8 = 72;
const DECAY_MS: f64 = 400.0;
const SECONDS: f64 = 1.5;
const FADED: f32 = 0.01;
const OCTAVE: f64 = 2.0;
const WITHIN: f64 = 0.05;

fn project(pitch: u8, key_follow: f64) -> Project {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: DECAY_MS, sustain: 0.0, release_ms: 1.0, key_follow, ..Envelope::DEFAULT };
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(0.5, patch))];
    let notes = vec![Note { pitch, start: 0.0, length: 4.0, velocity: VELOCITY, articulation: Articulation::PLAIN }];
    track.clips.push(Clip { start: 0, length: 4 * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn render(project: &Project) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(project, None, &[], &[]);
    audio.play(0.0);
    let frames = frames(SECONDS);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(frames);
    while out.len() < frames {
        let n = BLOCK.min(frames - out.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    out
}

fn decay_seconds(midi: u8, key_follow: f64) -> f64 {
    let heard = render(&project(midi, key_follow));
    let floor = peak_of(&heard) * FADED;
    let faded = heard.iter().rposition(|sample| sample.abs() > floor).expect("wanted the note heard");
    faded as f64 / SAMPLE_RATE
}

#[test]
fn key_follow_halves_the_decay_an_octave_up() {
    let (low, high) = (decay_seconds(C4, 1.0), decay_seconds(C5, 1.0));
    let ratio = low / high;
    assert!((ratio - OCTAVE).abs() < OCTAVE * WITHIN, "wanted C5's decay half of C4's with key follow at 1, got {high:.3} s against {low:.3} s");
}

#[test]
fn key_follow_turned_down_doubles_the_decay_an_octave_up() {
    let (low, high) = (decay_seconds(C4, -1.0), decay_seconds(C5, -1.0));
    let ratio = high / low;
    assert!((ratio - OCTAVE).abs() < OCTAVE * WITHIN, "wanted C5's decay twice C4's with key follow at -1, got {high:.3} s against {low:.3} s");
}

#[test]
fn without_key_follow_the_decay_is_the_same_on_every_key() {
    let (low, high) = (decay_seconds(C4, 0.0), decay_seconds(C5, 0.0));
    let ratio = low / high;
    assert!((ratio - 1.0).abs() < WITHIN, "wanted the same decay on C4 and C5 without key follow, got {low:.3} s against {high:.3} s");
}
