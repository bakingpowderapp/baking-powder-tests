use bp::arp::Arp;
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::core::params::Envelope;
use bp::project::{Project, Row, VELOCITY};
use support::{BLOCK, SAMPLE_RATE, core_layer, frames, midi};

const PITCHES: [(&str, u8); 3] = [("A2", 45), ("A4", 69), ("A6", 93)];
const SETTLED_SECONDS: f64 = 0.5;
const HELD_SECONDS: f64 = 2.0;
const CLEAN_DB: f64 = -50.0;
const MASTER_SEMITONES: f64 = -7.0;
const ALIKE_DB: f64 = 0.5;
const FM_AMOUNT: f64 = 0.1;
const MODULATOR_SEMITONES: f64 = 5.0;

fn saw(bright: bool, synced: bool) -> Params {
    let mut patch = Params::DEFAULT;
    patch.osc1.bright = bright;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    if synced {
        patch.osc1.sync = true;
        patch.osc2.tune = MASTER_SEMITONES;
        patch.osc2.level = 0.0;
    }
    patch
}

fn project(patch: Params) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch))];
    project
}

fn held(patch: Params, midi: u8) -> Vec<f32> {
    held_then(patch, None, midi)
}

fn held_then(patch: Params, change: Option<Params>, midi: u8) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(&project(patch), None, &[], &[]);
    audio.note_on(0, midi, VELOCITY);
    let mut out = Vec::new();
    let (mut left, mut right) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut changed = false;
    while out.len() < frames(HELD_SECONDS) {
        if let Some(changed_to) = change.filter(|_| !changed && out.len() >= frames(SETTLED_SECONDS)) {
            audio.sync(&project(changed_to), None, &[], &[]);
            changed = true;
        }
        let n = BLOCK.min(frames(HELD_SECONDS) - out.len());
        renderer.render(&mut left[..n], &mut right[..n]);
        out.extend_from_slice(&left[..n]);
    }
    out.split_off(frames(if change.is_some() { 2.0 * SETTLED_SECONDS } else { SETTLED_SECONDS }))
}

fn mean_and_rms(samples: &[f32]) -> (f64, f64) {
    let n = samples.len() as f64;
    let mean = samples.iter().map(|&x| f64::from(x)).sum::<f64>() / n;
    let rms = (samples.iter().map(|&x| f64::from(x) * f64::from(x)).sum::<f64>() / n).sqrt();
    (mean, rms)
}

fn dc_db(samples: &[f32]) -> f64 {
    let (mean, rms) = mean_and_rms(samples);
    20.0 * (mean.abs() / rms).log10()
}

fn expect_clean(patch: Params, what: &str) {
    for (name, midi) in PITCHES {
        let got = dc_db(&held(patch, midi));
        assert!(got < CLEAN_DB, "wanted {what}'s mean under {CLEAN_DB} dB of its RMS at {name}, got {got:.1} dB");
    }
}

#[test]
fn a_bright_saw_carries_no_dc_at_any_pitch() {
    expect_clean(saw(true, false), "the bright saw");
}

#[test]
fn a_hard_synced_saw_carries_no_dc_with_or_without_bright() {
    expect_clean(saw(false, true), "the synced saw");
    expect_clean(saw(true, true), "the synced bright saw");
}

#[test]
fn the_bright_saw_keeps_the_plain_saws_level() {
    for (name, midi) in PITCHES {
        let (_, plain) = mean_and_rms(&held(saw(false, false), midi));
        let (_, bright) = mean_and_rms(&held(saw(true, false), midi));
        let gap = 20.0 * (bright / plain).log10();
        assert!(gap.abs() < ALIKE_DB, "wanted the bright saw within {ALIKE_DB} dB of the plain saw's RMS at {name}, got {gap:+.2} dB");
    }
}

#[test]
fn fm_turned_on_during_a_bright_saw_leaves_no_dc_behind() {
    let mut modulated = saw(true, false);
    modulated.osc1.fm = FM_AMOUNT;
    modulated.osc2.tune = MODULATOR_SEMITONES;
    for (name, midi) in PITCHES {
        let (stale, _) = mean_and_rms(&held_then(saw(true, false), Some(modulated), midi));
        let (fresh, rms) = mean_and_rms(&held(modulated, midi));
        let got = 20.0 * ((stale - fresh).abs() / rms).log10();
        assert!(got < CLEAN_DB, "wanted a bright saw that took FM mid-note to carry the mean of one modulated from the start, within {CLEAN_DB} dB of the RMS at {name}, got a gap of {got:.1} dB");
    }
}
