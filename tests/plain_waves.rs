use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{BLOCK, SAMPLE_RATE, amplitude, core_layer, midi};

const C4: u8 = 60;
const C7: u8 = 96;
const SETTLED: usize = 24_000;
const HEARD: usize = 96_000;
const FLOOR_DB: f64 = -90.0;
const GRID_HZ: f64 = 97.0;
const CLEAR_HZ: f64 = 60.0;
const SCAN_TOP_HZ: f64 = 20_000.0;
const SCAN_LOW_HZ: f64 = 1_000.0;
const TOP_BAND_HZ: (f64, f64) = (8_000.0, 12_000.0);
const ROLL_OFF_DB: (f64, f64) = (0.6, 2.0);
const NARROW_PULSE: f64 = 0.125;
const UNDER_NAIVE_DB: f64 = 10.0;

fn hz_of(midi: u8) -> f64 {
    440.0 * ((f64::from(midi) - 69.0) / 12.0).exp2()
}

fn project(pitch: u8, shape: impl FnOnce(&mut Params)) -> Project {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels[0] = 0.0;
    shape(&mut patch);
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(0.25, patch))];
    let notes = vec![Note { pitch, start: 0.0, length: 8.0, velocity: VELOCITY, articulation: Articulation::PLAIN }];
    track.clips.push(Clip { start: 0, length: 8 * TICKS_PER_BEAT, label: String::new(), notes, file: None });
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

fn clear_of_harmonics(hz: f64, fundamental: f64) -> bool {
    let nearest = (hz / fundamental).round() * fundamental;
    (hz - nearest).abs() >= CLEAR_HZ
}

fn between_harmonics(fundamental: f64) -> Vec<f64> {
    let first_fold = (1..).map(|k| SAMPLE_RATE - k as f64 * fundamental).skip_while(|hz| *hz > SAMPLE_RATE / 2.0).take_while(|hz| *hz > SCAN_LOW_HZ);
    let grid = (0..).map(|i| SCAN_LOW_HZ + i as f64 * GRID_HZ).take_while(|hz| *hz < SCAN_TOP_HZ);
    first_fold.chain(grid).filter(|hz| *hz < SCAN_TOP_HZ && clear_of_harmonics(*hz, fundamental)).collect()
}

fn loudest_fold(heard: &[f32], fundamental: f64) -> (f64, f64) {
    let first = amplitude(heard, fundamental);
    between_harmonics(fundamental).into_iter().map(|hz| (20.0 * (amplitude(heard, hz) / first).log10(), hz)).fold((f64::NEG_INFINITY, 0.0), |worst, (db, hz)| if db > worst.0 { (db, hz) } else { worst })
}

fn assert_clean(name: &str, midi: u8, shape: impl FnOnce(&mut Params)) {
    let fundamental = hz_of(midi);
    let heard = render(&project(midi, shape));
    let (db, hz) = loudest_fold(&heard, fundamental);
    assert!(db < FLOOR_DB, "wanted every component between the {name}'s harmonics at {fundamental:.0} Hz under {FLOOR_DB} dB re the fundamental, got {db:+.1} dB at {hz:.0} Hz");
}

#[test]
fn a_plain_saw_folds_nothing_under_20_khz() {
    assert_clean("plain saw at C4", C4, |p| p.osc1.levels[0] = 1.0);
    assert_clean("plain saw at C7", C7, |p| p.osc1.levels[0] = 1.0);
}

#[test]
fn a_narrow_plain_pulse_folds_nothing_at_c7() {
    assert_clean("plain pulse", C7, |p| {
        p.osc1.levels[1] = 1.0;
        p.osc1.pulse_width = NARROW_PULSE;
    });
}

fn naive_triangle(fundamental: f64) -> Vec<f32> {
    (0..HEARD).map(|i| (1.0 - 4.0 * ((fundamental * i as f64 / SAMPLE_RATE).fract() - 0.5).abs()) as f32).collect()
}

#[test]
fn a_plain_triangle_folds_well_under_the_naive_wave_at_c7() {
    let fundamental = hz_of(C7);
    let (plain_db, plain_hz) = loudest_fold(&render(&project(C7, |p| p.osc1.levels[2] = 1.0)), fundamental);
    let (naive_db, naive_hz) = loudest_fold(&naive_triangle(fundamental), fundamental);
    assert!(plain_db < naive_db - UNDER_NAIVE_DB, "wanted the plain triangle's loudest fold at {fundamental:.0} Hz at least {UNDER_NAIVE_DB} dB under the naive triangle's {naive_db:+.1} dB at {naive_hz:.0} Hz, its rounded corners band-limiting it, got {plain_db:+.1} dB at {plain_hz:.0} Hz");
}

#[test]
fn the_plain_saw_keeps_its_soft_top() {
    let fundamental = hz_of(C4);
    let heard = render(&project(C4, |p| p.osc1.levels[0] = 1.0));
    let first = amplitude(&heard, fundamental);
    for h in (1..).take_while(|h| *h as f64 * fundamental < TOP_BAND_HZ.1) {
        let hz = h as f64 * fundamental;
        if hz < TOP_BAND_HZ.0 {
            continue;
        }
        let under = -20.0 * (amplitude(&heard, hz) / first).log10() - 20.0 * (h as f64).log10();
        assert!(under > ROLL_OFF_DB.0 && under < ROLL_OFF_DB.1, "wanted harmonic {h} of the plain saw, {hz:.0} Hz, {} to {} dB under 1/{h}, the soft edge's roll-off, got {under:+.2} dB", ROLL_OFF_DB.0, ROLL_OFF_DB.1);
    }
}
