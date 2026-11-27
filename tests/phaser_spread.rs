use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::fx::params::motion::MotionBand;
use bp::fx::params::{MotionMode, Phaser};
use bp::fx::{self, Slot};
use bp::json;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{BLOCK, SAMPLE_RATE, amplitude, core_layer, midi, one_band, worst_delta};

const SOURCE_MIDI: u8 = 96;
const SOURCE_FINE_CENTS: f64 = 25.7;
const CENTRE_HZ: f64 = 880.0;
const STACKED_NOTCH_TIMES: f64 = 2.414;
const SETTLED: usize = 24_000;
const HEARD: usize = 48_000;
const WIDE_SPREAD_OCTAVES: f64 = 3.0;
const RIGHT_OFFSET_OCTAVES: f64 = 1.0;
const SLOW_RATE_HZ: f64 = 0.02;
const NOTCHED_DB: f64 = 4.0;
const CLEARED_DB: f64 = 3.0;
const PARTED: f32 = 1e-3;

fn source_hz() -> f64 {
    440.0 * ((f64::from(SOURCE_MIDI) - 69.0) / 12.0 + SOURCE_FINE_CENTS / 1200.0).exp2()
}

fn phaser(tweak: impl FnOnce(&mut Phaser)) -> Row {
    mixed(0.5, tweak)
}

fn mixed(wet: f64, tweak: impl FnOnce(&mut Phaser)) -> Row {
    let mut phaser = Phaser { center_hz: CENTRE_HZ, depth_octaves: 0.0, feedback: 0.0, spread: 0.0, stages: 4, ..Phaser::DEFAULT };
    tweak(&mut phaser);
    Row::Effect(Slot { wet, ..one_band(MotionBand { mode: MotionMode::Phaser, phaser, ..MotionBand::DEFAULT }) })
}

fn project(row: Row) -> Project {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.osc1.fine = SOURCE_FINE_CENTS;
    patch.amp.sustain = 1.0;
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(0.25, patch)), row];
    let notes = vec![Note { pitch: SOURCE_MIDI, start: 0.0, length: 8.0, velocity: VELOCITY, articulation: Articulation::PLAIN }];
    track.clips.push(Clip { start: 0, length: 8 * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn render(project: &Project) -> (Vec<f32>, Vec<f32>) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(project, None, &[], &[]);
    audio.play(0.0);
    let frames = SETTLED + HEARD;
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let (mut left, mut right) = (Vec::with_capacity(frames), Vec::with_capacity(frames));
    while left.len() < frames {
        let n = BLOCK.min(frames - left.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        left.extend_from_slice(&l[..n]);
        right.extend_from_slice(&r[..n]);
    }
    (left.split_off(SETTLED), right.split_off(SETTLED))
}

fn source_db(row: Row) -> f64 {
    let (left, _) = render(&project(row));
    20.0 * amplitude(&left, source_hz()).log10()
}

#[test]
fn a_stage_spread_moves_the_stacked_notch_away() {
    let notch_hz = STACKED_NOTCH_TIMES * CENTRE_HZ;
    assert!((source_hz() - notch_hz).abs() < 2.0, "wanted the source within 2 Hz of the four stacked stages' upper notch at {notch_hz:.1} Hz, got {:.1} Hz", source_hz());
    let dry = source_db(mixed(0.0, |_| {}));
    let stacked = source_db(phaser(|_| {}));
    let spread = source_db(phaser(|p| p.stage_spread = WIDE_SPREAD_OCTAVES));
    assert!(dry - stacked > NOTCHED_DB, "wanted the four stacked stages to notch the source at {notch_hz:.0} Hz by more than {NOTCHED_DB} dB, got {stacked:.1} dB against {dry:.1} dB dry");
    assert!(dry - spread < CLEARED_DB, "wanted the stages spread {WIDE_SPREAD_OCTAVES} octaves apart to clear the notch, the source within {CLEARED_DB} dB of dry, got {spread:.1} dB against {dry:.1} dB dry");
}

#[test]
fn a_right_offset_parts_the_channels() {
    let (left, right) = render(&project(phaser(|_| {})));
    assert!(worst_delta(&left, &right) < PARTED, "wanted the channels alike with no right offset, got a worst gap of {}", worst_delta(&left, &right));
    let (left, right) = render(&project(phaser(|p| p.right_offset = RIGHT_OFFSET_OCTAVES)));
    assert!(worst_delta(&left, &right) > PARTED, "wanted the right channel's notches an octave above the left's to part the channels, got a worst gap of {}", worst_delta(&left, &right));
}

fn rate_through_the_save(rate_hz: f64) -> f64 {
    let written = phaser(|p| p.rate_hz = rate_hz).to_json().write();
    let parsed = json::parse(&written).unwrap_or_else(|| panic!("wanted the phaser row to write valid JSON, got {written}"));
    let Some(Row::Effect(slot)) = Row::from_json(&parsed) else { panic!("wanted the phaser row to read back as an effect from {written}") };
    match slot.params {
        fx::Params::Motion(motion) if motion.bands[0].mode == MotionMode::Phaser => motion.bands[0].phaser.rate_hz,
        _ => panic!("wanted a Motion row with its phaser band back from {written}"),
    }
}

#[test]
fn the_rate_floor_is_the_phasers_own() {
    let slow = rate_through_the_save(SLOW_RATE_HZ);
    assert!((slow - SLOW_RATE_HZ).abs() < 1e-9, "wanted a rate of {SLOW_RATE_HZ} Hz, under the other rows' floor of {}, kept through the save, got {slow}", bp::fx::params::RATE_MIN_HZ);
    let floor = rate_through_the_save(Phaser::MIN_RATE_HZ / 2.0);
    assert!((floor - Phaser::MIN_RATE_HZ).abs() < 1e-9, "wanted a rate under the phaser's floor clamped up to {}, got {floor}", Phaser::MIN_RATE_HZ);
}
