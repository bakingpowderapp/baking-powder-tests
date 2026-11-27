use bp::fx::{Clock, Effect, Kind, Params, Slot};
use std::f64::consts::TAU;
use support::{BLOCK, HUSH_DB, SAMPLE_RATE};

const BPM: f64 = 120.0;
const TONE_HZ: f64 = 440.0;
const LEVEL: f32 = 0.5;
const CLOSE: f32 = 1e-6;

#[test]
fn the_utility_is_fully_wet_from_its_first_frame() {
    let mut effect = Effect::new(Kind::Utility, SAMPLE_RATE, BLOCK);
    let mut slot = Slot::new(Kind::Utility);
    if let Params::Utility(utility) = &mut slot.params {
        utility.gain_db = HUSH_DB;
    }
    let clock = Clock { sample_rate: SAMPLE_RATE, bpm: BPM, beat: None };
    let input: Vec<f32> = (0..BLOCK).map(|frame| (TAU * TONE_HZ * frame as f64 / SAMPLE_RATE).cos() as f32 * LEVEL).collect();
    let (mut l, mut r) = (input.clone(), input.clone());
    effect.process(&mut l, &mut r, &slot, &clock);
    let hushed = 10f32.powf(HUSH_DB as f32 / 20.0);
    assert!(input[0].abs() > LEVEL / 2.0, "wanted the first frame fed to carry sound, got {}", input[0]);
    for (side, heard) in [("left", &l), ("right", &r)] {
        let stray = heard.iter().zip(&input).position(|(out, fed)| (out - fed * hushed).abs() > CLOSE);
        assert_eq!(stray, None, "wanted a utility hushed by {HUSH_DB} dB to put out its input at that gain from its first frame on the {side}, as if neither its gain nor its wet mix eased in, first apart at frame");
    }
}
