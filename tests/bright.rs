use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{BLOCK, SAMPLE_RATE, amplitude, core_layer, midi};

const MIDI: u8 = 66;
const FINE_CENTS: f64 = 16.5;
const SETTLED: usize = 24_000;
const HEARD: usize = 48_000;
const KEPT_DB: f64 = 0.5;
const ALIKE_DB: f64 = 0.2;
const LOW_HZ: f64 = 4_000.0;
const TOP_HZ: (f64, f64) = (16_000.0, 19_000.0);
const RESTORED_DB: f64 = 2.5;
const ABSENT_DB: f64 = -40.0;

fn fundamental() -> f64 {
    440.0 * ((f64::from(MIDI) - 69.0) / 12.0 + FINE_CENTS / 1200.0).exp2()
}

fn harmonics() -> impl Iterator<Item = usize> {
    (1..).take_while(|h| *h as f64 * fundamental() < TOP_HZ.1)
}

fn project(saw: f64, square: f64, bright: bool) -> Project {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [saw, square, 0.0, 0.0];
    patch.osc1.fine = FINE_CENTS;
    patch.osc1.bright = bright;
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(0.25, patch))];
    let notes = vec![Note { pitch: MIDI, start: 0.0, length: 4.0, velocity: VELOCITY, articulation: Articulation::PLAIN }];
    track.clips.push(Clip { start: 0, length: 4 * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn render(project: &Project) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(project, None, &[], &[]);
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

fn spectrum(project: &Project) -> Vec<(usize, f64)> {
    let heard = render(project);
    let first = amplitude(&heard, fundamental());
    harmonics().map(|h| (h, 20.0 * (amplitude(&heard, h as f64 * fundamental()) / first).log10())).collect()
}

fn off_ideal(h: usize, db: f64) -> f64 {
    db + 20.0 * (h as f64).log10()
}

#[test]
fn a_bright_saw_keeps_every_harmonic_to_19_khz() {
    for (h, db) in spectrum(&project(1.0, 0.0, true)) {
        let off = off_ideal(h, db);
        assert!(off.abs() < KEPT_DB, "wanted harmonic {h} of the bright saw, {:.0} Hz, within {KEPT_DB} dB of 1/{h}, got {off:+.2} dB", h as f64 * fundamental());
    }
}

#[test]
fn the_bright_edge_restores_the_top_octave_the_plain_saw_loses() {
    let plain = spectrum(&project(1.0, 0.0, false));
    let bright = spectrum(&project(1.0, 0.0, true));
    for ((h, plain), (_, bright)) in plain.into_iter().zip(bright) {
        let hz = h as f64 * fundamental();
        let lift = bright - plain;
        if hz < LOW_HZ {
            assert!(lift.abs() < ALIKE_DB, "wanted harmonic {h}, {hz:.0} Hz, alike bright or plain, got {lift:+.2} dB");
        }
        if hz >= TOP_HZ.0 {
            assert!(lift > RESTORED_DB, "wanted harmonic {h}, {hz:.0} Hz, {RESTORED_DB} dB or more louder bright than plain, got {lift:+.2} dB");
        }
    }
}

#[test]
fn a_bright_square_keeps_its_odd_harmonics_to_19_khz() {
    for (h, db) in spectrum(&project(0.0, 1.0, true)) {
        let hz = h as f64 * fundamental();
        if h % 2 == 1 {
            let off = off_ideal(h, db);
            assert!(off.abs() < KEPT_DB, "wanted odd harmonic {h} of the bright square, {hz:.0} Hz, within {KEPT_DB} dB of 1/{h}, got {off:+.2} dB");
        } else {
            assert!(db < ABSENT_DB, "wanted even harmonic {h} of the bright square, {hz:.0} Hz, under {ABSENT_DB} dB, got {db:+.1} dB");
        }
    }
}
