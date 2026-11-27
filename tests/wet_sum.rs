use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params as CoreParams;
use bp::core::params::Envelope;
use bp::fx::params::{Delay, DelayMode, Reverb};
use bp::fx::{Kind, Params, Slot};
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{AUDIBLE, BLOCK, SAMPLE_RATE, core_layer, frames, midi, peak_of, worst_delta};

const KEY: u8 = 69;
const WET: f64 = 0.5;
const NOTE_SECONDS: f64 = 0.05;
const RENDER_SECONDS: f64 = 1.0;
const SETTLED_SECONDS: f64 = 0.05;
const TOLERANCE: f32 = 1e-4;
const ECHO_MS: f64 = 100.0;
const ECHO_FROM: f64 = 0.1;
const ECHO_TO: f64 = 0.3;
const TRIM_DB: f64 = 12.0;
const TRIM_TOLERANCE_DB: f64 = 0.2;
const SONG_SECONDS: usize = 4;
const SONG_BEATS: i32 = 8;
const NOTE_BEAT: f32 = 4.0;
const NOTE_BEATS: f32 = 2.0;
const REVERB_DECAY_S: f64 = 0.6;
type Stereo = [f32; 2];

fn sine() -> CoreParams {
    let mut patch = CoreParams::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    patch
}

fn reverb(wet: f64, sum: bool) -> Row {
    Row::Effect(Slot { wet, params: Params::Reverb(Reverb { dry: f64::from(u8::from(sum)), ..Reverb::DEFAULT }), ..Slot::new(Kind::Reverb) })
}

fn delay(wet: f64, sum: bool, trim_db: f64) -> Row {
    let params = Delay { mode: DelayMode::Clean, time_l_ms: ECHO_MS, time_r_ms: ECHO_MS, feedback: 0.0, wet_trim_db: trim_db, dry: f64::from(u8::from(sum)), ..Delay::DEFAULT };
    Row::Effect(Slot { wet, params: Params::Delay(params), ..Slot::new(Kind::Delay) })
}

fn rendered(effect: Row) -> Vec<f32> {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, sine())), effect];
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(&project, None, &[], &[]);
    audio.note_on(0, KEY, VELOCITY);
    let mut out = Vec::new();
    let (mut left, mut right) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut lifted = false;
    while out.len() < frames(RENDER_SECONDS) {
        if !lifted && out.len() >= frames(NOTE_SECONDS) {
            audio.note_off(0, KEY);
            lifted = true;
        }
        let n = BLOCK.min(frames(RENDER_SECONDS) - out.len());
        renderer.render(&mut left[..n], &mut right[..n]);
        out.extend_from_slice(&left[..n]);
    }
    out
}

fn expect_sum_adds_the_dry_back(summed: &[f32], faded: &[f32], dry: &[f32], what: &str) {
    let peak = peak_of(dry);
    assert!(peak > AUDIBLE, "wanted the dry {what} render to sound, got a peak of {peak}");
    let from = frames(SETTLED_SECONDS);
    let given_back: Vec<f32> = summed[from..].iter().zip(&faded[from..]).map(|(a, b)| a - b).collect();
    let dry_share: Vec<f32> = dry[from..].iter().map(|sample| sample * WET as f32).collect();
    let worst = worst_delta(&given_back, &dry_share) / peak;
    assert!(worst < TOLERANCE, "wanted SUM on the {what} to add back exactly the {WET} of the dry the crossfade took, got a worst gap of {worst} of the dry peak");
}

fn rms_db(samples: &[f32], from: f64, to: f64) -> f64 {
    let span = &samples[frames(from)..frames(to)];
    let mean_square = span.iter().map(|&x| f64::from(x) * f64::from(x)).sum::<f64>() / span.len() as f64;
    10.0 * mean_square.log10()
}

#[test]
fn the_reverb_with_sum_keeps_the_dry_at_full() {
    let summed = rendered(reverb(WET, true));
    let faded = rendered(reverb(WET, false));
    let dry = rendered(reverb(0.0, false));
    expect_sum_adds_the_dry_back(&summed, &faded, &dry, "reverb");
}

#[test]
fn the_delay_with_sum_keeps_the_dry_at_full() {
    let summed = rendered(delay(WET, true, 0.0));
    let faded = rendered(delay(WET, false, 0.0));
    let dry = rendered(delay(0.0, false, 0.0));
    expect_sum_adds_the_dry_back(&summed, &faded, &dry, "delay");
}

#[test]
fn the_delay_trim_reaches_twelve_decibels() {
    let plain = rendered(delay(1.0, false, 0.0));
    let trimmed = rendered(delay(1.0, false, TRIM_DB));
    let echo = rms_db(&plain, ECHO_FROM, ECHO_TO);
    assert!(echo > -60.0, "wanted an echo between {ECHO_FROM} and {ECHO_TO} s, got {echo:.1} dB");
    let lift = rms_db(&trimmed, ECHO_FROM, ECHO_TO) - echo;
    assert!((lift - TRIM_DB).abs() < TRIM_TOLERANCE_DB, "wanted a trim of {TRIM_DB} dB to lift the echoes by {TRIM_DB} dB, got {lift:+.2} dB");
}

fn reversing(kind: Kind, wet: f64, sum: bool) -> Row {
    let params = match kind {
        Kind::Delay => Params::Delay(Delay { mode: DelayMode::Clean, time_l_ms: ECHO_MS, time_r_ms: ECHO_MS, feedback: 0.0, forward: 0.0, reverse: 1.0, dry: f64::from(u8::from(sum)), ..Delay::DEFAULT }),
        Kind::Reverb => Params::Reverb(Reverb { decay_s: REVERB_DECAY_S, mod_depth: 0.0, forward: 0.0, reverse: 1.0, dry: f64::from(u8::from(sum)), ..Reverb::DEFAULT }),
        _ => unreachable!(),
    };
    Row::Effect(Slot { wet, params, ..Slot::new(kind) })
}

fn played(effect: Row) -> Vec<Stereo> {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, sine())), effect];
    let notes = vec![Note { pitch: KEY, start: NOTE_BEAT, length: NOTE_BEATS, velocity: VELOCITY, articulation: Articulation::PLAIN }];
    project.tracks[0].clips = vec![Clip { start: 0, length: SONG_BEATS * TICKS_PER_BEAT, label: String::new(), notes, file: None }];
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(&project, None, &[], &[]);
    audio.play(0.0);
    let total = SONG_SECONDS * SAMPLE_RATE as usize;
    let (mut left, mut right) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(total);
    while out.len() < total {
        let n = BLOCK.min(total - out.len());
        renderer.render(&mut left[..n], &mut right[..n]);
        out.extend(left[..n].iter().zip(&right[..n]).map(|(&l, &r)| [l, r]));
    }
    out
}

fn expect_reverse_adds_the_dry_once(kind: Kind) {
    let summed = played(reversing(kind, 1.0, true));
    let alone = played(reversing(kind, 1.0, false));
    let dry = played(reversing(kind, 0.0, false));
    let peak = dry.iter().flatten().fold(0.0f32, |peak, sample| peak.max(sample.abs()));
    assert!(peak > AUDIBLE, "wanted the dry {kind:?} render to sound, got a peak of {peak}");
    let worst = summed.iter().zip(&alone).zip(&dry).flat_map(|((s, a), d)| (0..2).map(move |channel| ((s[channel] - a[channel]) - d[channel]).abs())).fold(0.0f32, f32::max) / peak;
    assert!(worst < TOLERANCE, "wanted SUM on a {kind:?} playing in reverse to add the dry once over its reversed tail, got a worst gap of {worst} of the dry peak");
}

#[test]
fn the_delay_with_sum_and_reverse_adds_the_dry_once() {
    expect_reverse_adds_the_dry_once(Kind::Delay);
}

#[test]
fn the_reverb_with_sum_and_reverse_adds_the_dry_once() {
    expect_reverse_adds_the_dry_once(Kind::Reverb);
}
