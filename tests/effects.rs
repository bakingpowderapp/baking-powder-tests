use bp::fx::{Beat, Clock, Effect, Kind, Slot};
use support::{BLOCK, SAMPLE_RATE, peak_of};

const BPM: f64 = 120.0;
const TONE_HZ: f64 = 440.0;
const LEVEL: f32 = 0.5;
const SETTLE: usize = 200;
const LISTEN: usize = 50;
const FLOOR: f32 = 1e-9;

fn tone_at(frame: f64) -> f32 {
    (std::f64::consts::TAU * TONE_HZ * frame / SAMPLE_RATE).sin() as f32 * LEVEL
}

fn tone(frame: usize) -> f32 {
    tone_at(frame as f64)
}

fn process(effect: &mut Effect, slot: &Slot, input: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(input.len());
    for (index, chunk) in input.chunks(BLOCK).enumerate() {
        let (mut l, mut r) = (chunk.to_vec(), chunk.to_vec());
        let clock = Clock { sample_rate: SAMPLE_RATE, bpm: BPM, beat: Some(Beat { anchor: 0.0, since: (index * BLOCK) as i64, frames_per_beat: SAMPLE_RATE * 60.0 / BPM }) };
        effect.process(&mut l, &mut r, slot, &clock);
        out.extend_from_slice(&l);
    }
    out
}

fn failing(check: impl Fn(Kind) -> Option<String>) -> Vec<String> {
    Kind::ALL.iter().filter_map(|&kind| check(kind).map(|why| format!("{}: {why}", kind.label()))).collect()
}

#[test]
fn silence_in_is_silence_out() {
    let failures = failing(|kind| {
        let mut effect = Effect::new(kind, SAMPLE_RATE, BLOCK);
        let out = process(&mut effect, &Slot::new(kind), &vec![0.0; BLOCK * SETTLE]);
        let peak = peak_of(&out);
        (peak > FLOOR).then(|| format!("peak {peak} from silence, over the {FLOOR} floor"))
    });
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn wet_zero_is_the_dry_late_by_the_rows_report() {
    let input: Vec<f32> = (0..BLOCK * SETTLE).map(tone).collect();
    let failures = failing(|kind| {
        let mut effect = Effect::new(kind, SAMPLE_RATE, BLOCK);
        let slot = Slot { wet: 0.0, ..Slot::new(kind) };
        let out = process(&mut effect, &slot, &input);
        let late = slot.latency(SAMPLE_RATE);
        let dry = |n: usize| if n >= late { input[n - late] } else { 0.0 };
        let divergence = out.iter().enumerate().position(|(n, got)| dry(n).to_bits() != got.to_bits())?;
        let worst = out.iter().enumerate().map(|(n, got)| (dry(n) - got).abs()).fold(0.0f32, f32::max);
        Some(format!("diverges from its input {late} frames late, its reported latency, at frame {divergence}, worst delta {worst}"))
    });
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn non_finite_input_is_contained() {
    let mut input = vec![f32::NAN; BLOCK];
    input.extend(vec![f32::INFINITY; BLOCK]);
    input.extend(vec![f32::NEG_INFINITY; BLOCK]);
    input.extend((0..BLOCK * SETTLE).map(tone));
    let failures = failing(|kind| {
        let mut effect = Effect::new(kind, SAMPLE_RATE, BLOCK);
        let out = process(&mut effect, &Slot::new(kind), &input);
        let tail = &out[out.len() - BLOCK * LISTEN..];
        if tail.iter().any(|sample| !sample.is_finite()) {
            return Some(String::from("still non-finite a second after the burst"));
        }
        let peak = peak_of(tail);
        (peak < 0.05).then(|| format!("tone lost after the burst, peak {peak}"))
    });
    assert!(failures.is_empty(), "{failures:#?}");
}
