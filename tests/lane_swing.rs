use bp::core::Params;
use bp::fx::{Kind, Slot};
use bp::project::{Aim, Row};
use support::core_layer;

const CENTRE_HZ: f64 = 1000.0;
const AMOUNT: f64 = 0.2;
const OLD_SPAN_OCTAVES: f64 = 9.965_784_284_662_087;
const WITHIN: f64 = 1e-6;
const FLOOR_HZ: f64 = 5.0;

fn core_row() -> Row {
    Row::Core(core_layer(1.0, Params::DEFAULT))
}

fn lane_share(row: Row, band: &str, name: &str) -> f64 {
    row.lane_share(&Aim::knob(0, band, name))
}

fn cutoff_swing_octaves(centre_hz: f64, amount: f64) -> (f64, f64) {
    let share = lane_share(core_row(), "FILTER", "CUTOFF");
    let mut patch = Params::DEFAULT;
    patch.filter.cutoff_hz = centre_hz;
    let centre = patch.travel_of("FILTER", "CUTOFF").expect("wanted the cutoff among the Core's knobs");
    let mut up = patch;
    up.set_travel("FILTER", "CUTOFF", centre + 0.5 * amount * share);
    let mut down = patch;
    down.set_travel("FILTER", "CUTOFF", centre - 0.5 * amount * share);
    ((up.filter.cutoff_hz / centre_hz).log2(), (centre_hz / down.filter.cutoff_hz).log2())
}

#[test]
fn a_cutoff_lanes_amount_means_the_octaves_it_did_before_the_floor_moved() {
    let share = lane_share(core_row(), "FILTER", "CUTOFF");
    let pinned = (20_000f64 / 20.0).ln() / (20_000f64 / 5.0).ln();
    assert!((share - pinned).abs() < WITHIN, "wanted the cutoff lane's share pinned to the old 20 Hz to 20 kHz span over the knob's 5 Hz to 20 kHz, {pinned}, got {share}");
    let (up, down) = cutoff_swing_octaves(CENTRE_HZ, AMOUNT);
    let wanted = 0.5 * AMOUNT * OLD_SPAN_OCTAVES;
    assert!((up - wanted).abs() < WITHIN && (down - wanted).abs() < WITHIN, "wanted a cutoff lane at amount {AMOUNT} about {CENTRE_HZ} Hz to swing {wanted:.4} octaves each way, the law the lessons record, got +{up:.4} and -{down:.4}");
}

#[test]
fn a_cutoff_lanes_trough_still_reaches_the_new_floor() {
    let share = lane_share(core_row(), "FILTER", "CUTOFF");
    let mut patch = Params::DEFAULT;
    patch.filter.cutoff_hz = 20.0;
    let centre = patch.travel_of("FILTER", "CUTOFF").expect("wanted the cutoff among the Core's knobs");
    patch.set_travel("FILTER", "CUTOFF", centre - 0.5 * share);
    assert!((patch.filter.cutoff_hz - FLOOR_HZ).abs() < WITHIN, "wanted a full lane under a 20 Hz knob to reach the {FLOOR_HZ} Hz floor, got {} Hz", patch.filter.cutoff_hz);
}

#[test]
fn lanes_on_unmoved_and_widened_controls_keep_their_reach() {
    assert!(lane_share(core_row(), "FILTER", "RESONANCE") == 1.0, "wanted an unmoved control's lane share exactly 1");
    let motion = Row::Effect(Slot::new(Kind::Motion));
    let chorus = lane_share(motion, "CHORUS 1", "DEPTH");
    assert!((chorus - 1.0 / 5.0).abs() < WITHIN, "wanted the chorus depth lane's share pinned to the old 0 to 1 over the knob's 0 to 5, got {chorus}");
    let phaser = lane_share(motion, "PHASER 1", "RATE");
    assert!((phaser - (640f64).ln() / (40_000f64).ln()).abs() < WITHIN, "wanted the phaser rate lane's share pinned to the old 0.05 to 32 Hz span over the new 0.01 to 400, got {phaser}");
}
