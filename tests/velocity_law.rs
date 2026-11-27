use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::core::params::Envelope;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT};
use support::{AUDIBLE, BLOCK, LEVEL, SAMPLE_RATE, amplitude, core_layer, peak_of, struck, worst_delta};

const A4: u8 = 69;
const TOP: u8 = 127;
const SOFT: u8 = 64;
const FULL_AMOUNT: f64 = 1.0;
const HALF_AMOUNT: f64 = 0.5;
const NO_AMOUNT: f64 = 0.0;
const TOLERANCE: f64 = 1e-3;
const SETTLED: usize = 24_000;
const HEARD: usize = 96_000;
const BEATS: i32 = 8;
const CLIP: &str = "a clip through Audio and Renderer";

fn midi_hz(midi: u8) -> f64 {
    440.0 * ((f64::from(midi) - 69.0) / 12.0).exp2()
}

fn law(velocity: u8, amount: f64) -> f64 {
    let v = f64::from(velocity) / f64::from(TOP);
    1.0 + amount * (v - 1.0)
}

fn sine(amount: f64) -> Params {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, velocity: amount, ..Envelope::DEFAULT };
    patch
}

fn held(mut block: impl FnMut(&mut [f32], &mut [f32])) -> Vec<f32> {
    let frames = SETTLED + HEARD;
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(frames);
    while out.len() < frames {
        let n = BLOCK.min(frames - out.len());
        block(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    out.split_off(SETTLED)
}

fn project(amount: f64, velocity: u8) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![struck(Arp::DEFAULT), Row::Core(core_layer(LEVEL, sine(amount)))];
    let notes = vec![Note { pitch: A4, start: 0.0, length: BEATS as f32, velocity, articulation: Articulation::PLAIN }];
    track.clips.push(Clip { start: 0, length: BEATS * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn played(amount: f64, velocity: u8) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(&project(amount, velocity), None, &[], &[]);
    audio.play(0.0);
    held(|l, r| renderer.render(l, r))
}

fn assert_law(path: &str, amount: f64, render: impl Fn(f64, u8) -> Vec<f32>) {
    let (loud, soft) = (render(amount, TOP), render(amount, SOFT));
    let loud_peak = f64::from(peak_of(&loud));
    assert!(loud_peak > f64::from(AUDIBLE), "wanted note {A4} at velocity {TOP} from {path} to sound at velocityAmount {amount}, got a peak of {loud_peak:.6}");
    let ratio = law(SOFT, amount) / law(TOP, amount);
    let scaled: Vec<f32> = loud.iter().map(|sample| sample * ratio as f32).collect();
    let worst = f64::from(worst_delta(&soft, &scaled)) / (ratio * loud_peak);
    let heard = amplitude(&soft, midi_hz(A4)) / amplitude(&loud, midi_hz(A4));
    assert!(worst < TOLERANCE, "wanted note {A4} at velocity {SOFT} from {path} to render as the velocity {TOP} render scaled by the law 1 + velocityAmount * (velocity / 127 - 1), {ratio:.4} at velocityAmount {amount}, every sample within {TOLERANCE} of its peak, got {heard:.4} of the velocity {TOP} note's level and a sample {worst:.1e} of its peak off");
}

#[test]
fn a_clip_note_through_audio_keeps_the_law_at_every_amount() {
    for amount in [FULL_AMOUNT, HALF_AMOUNT, NO_AMOUNT] {
        assert_law(CLIP, amount, played);
    }
}
