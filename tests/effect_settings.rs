use bp::face::Faced;
use bp::fx::params::{Curve, Eq, EqBand, Freak, Reverb, Shape, Utility};
use bp::fx::{Beat, Clock, Effect, Kind, Params, Slot};
use std::f64::consts::TAU;
use support::{SAMPLE_RATE, peak_of, worst_delta};

const BPM: f64 = 120.0;
const MAX_FRAMES: usize = 1024;
const UNIFORM: [usize; 1] = [256];
const RAGGED: [usize; 12] = [1, 7, 64, 3, 512, 128, 256, 1, 33, 900, 17, 256];
const WILD: f64 = 1e12;
const CELLS_PAST_THE_LAST: usize = 512;
const OPEN_ENDED: f64 = 1e6;
const OPEN_ENDED_TOP: f64 = 8.0;
const ROLLS: u64 = 8;
const SEED: u64 = 0xA5A5_0000_0000_0001;
const STRIDE: u64 = 7919;
const SECONDS: f64 = 0.5;
const LEVEL: f64 = 0.35;
const SAME: f32 = 1e-12;
const HUSH: f32 = 1e-6;
const SETTLE_SECONDS: f64 = 1.0;
const LISTEN_SECONDS: f64 = 0.25;
const BURST: usize = 256;
const SWINGS_ON_ITS_OWN: (&str, &str) = ("BIAS LFO", "DEPTH");
const SOUND_SECONDS: f64 = 0.5;
const QUIET_SECONDS: f64 = 1.0;
const TAIL_SECONDS: f64 = 0.1;
const RINGING: f32 = 1e-9;
const BRIEF_QUIET_SECONDS: f64 = 0.06;
const LONG: [usize; 1] = [1024];
const SINGLE: [usize; 1] = [1];
const TONE_HZ: f64 = 250.0;
const TONE_FRAMES: usize = 24_300;
const TONE_LEVEL: f64 = 0.5;
const DYNAMIC_BAND: usize = 2;
const DEEP_DB: f64 = -18.0;
const LOW_THRESHOLD_DB: f64 = -40.0;
const STEEP_RATIO: f64 = 20.0;
const QUIET_MS: [f64; 4] = [80.0, 85.0, 90.0, 95.0];
const DRAINING_FRAMES: usize = 300;
const PUSHED_DB: f64 = 18.0;
const CEILING_DB: f64 = -12.0;
const SHARPEST: f64 = 18.0;
const TOP_HZ: f64 = 20_000.0;
const OVERFLOWING: f32 = 1e6;
const RETURNED_WITHIN: f32 = 1e-3;
const ECHO_SECONDS: f64 = 4.5;
const LONG_LINES: [(&str, &str, f64); 5] = [("TIME", "LEFT SYNC", 0.0), ("TIME", "RIGHT SYNC", 0.0), ("TIME", "LEFT TIME", WILD), ("TIME", "RIGHT TIME", WILD), ("LOOP", "FEEDBACK", 0.0)];
const SMALLEST_PLATE: f64 = 0.1;
const BUILDS: [f64; 4] = [0.25, 0.5, 0.75, 1.0];
const RUNAWAY_SECONDS: f64 = 2.0;
const TEN_TIMES_FULL_SCALE: f32 = 10.0;

type Stereo = [Vec<f32>; 2];

struct Dice(u64);

impl Dice {
    fn roll(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

struct Control {
    band: &'static str,
    name: &'static str,
    rest: f64,
    low: f64,
    high: f64,
}

fn frames(seconds: f64) -> usize {
    (seconds * SAMPLE_RATE) as usize
}

fn passage(seconds: f64) -> Stereo {
    let mut noise = Dice(SEED);
    [0, 1].map(|side| {
        (0..frames(seconds))
            .map(|frame| {
                let t = frame as f64 / SAMPLE_RATE;
                let kick = t % 0.25;
                let hat = (t + 0.125) % 0.25;
                let pluck = (t + 0.11 * side as f64) % 0.375;
                let drum = (-kick / 0.12).exp() * (TAU * 48.0 * kick).sin() + 0.3 * (-hat / 0.015).exp() * (noise.roll() * 2.0 - 1.0);
                let chord = 0.18 * ((TAU * 220.0 * t).sin() + (TAU * [277.18, 329.63][side] * t).sin());
                let string = 0.4 * (-pluck / 0.09).exp() * (TAU * 659.25 * pluck).sin();
                (LEVEL * (drum + chord + string)) as f32
            })
            .collect()
    })
}

fn kicks(seconds: f64) -> Stereo {
    let side: Vec<f32> = (0..frames(seconds))
        .map(|frame| {
            let kick = (frame as f64 / SAMPLE_RATE) % 0.5;
            (0.8 * (-kick / 0.1).exp() * (TAU * 55.0 * kick).sin()) as f32
        })
        .collect();
    [side.clone(), side]
}

fn silence(seconds: f64) -> Stereo {
    [vec![0.0; frames(seconds)], vec![0.0; frames(seconds)]]
}

fn play(slot: &Slot, input: &Stereo, key: &Stereo, sizes: &[usize]) -> Stereo {
    run(&mut Effect::new(slot.kind(), SAMPLE_RATE, MAX_FRAMES), slot, input, key, sizes)
}

fn run(effect: &mut Effect, slot: &Slot, input: &Stereo, key: &Stereo, sizes: &[usize]) -> Stereo {
    let total = input[0].len();
    let (mut l, mut r) = (input[0].clone(), input[1].clone());
    let (mut done, mut turn) = (0, 0);
    while done < total {
        let n = sizes[turn % sizes.len()].min(total - done);
        turn += 1;
        let clock = Clock { sample_rate: SAMPLE_RATE, bpm: BPM, beat: Some(Beat { anchor: 0.0, since: done as i64, frames_per_beat: SAMPLE_RATE * 60.0 / BPM }) };
        effect.feed(&key[0][done..done + n], &key[1][done..done + n]);
        effect.process(&mut l[done..done + n], &mut r[done..done + n], slot, &clock);
        done += n;
    }
    [l, r]
}

fn controls(kind: Kind) -> Vec<Control> {
    let rest = Slot::new(kind);
    let mut found: Vec<Control> = Vec::new();
    let (mut cell, mut blanks) = (0, 0);
    while blanks < CELLS_PAST_THE_LAST {
        match rest.setting_of(cell) {
            Some((band, name, at_rest)) => {
                blanks = 0;
                if !found.iter().any(|control| control.band == band && control.name == name) {
                    let end = |value: f64| {
                        let mut turned = rest;
                        turned.assign(band, name, value);
                        turned.setting_of(cell).map_or(0.0, |(_, _, value)| value)
                    };
                    let (low, high) = (end(-WILD), end(WILD));
                    found.push(Control { band, name, rest: at_rest, low, high: if high > OPEN_ENDED { OPEN_ENDED_TOP } else { high } });
                }
            }
            None => blanks += 1,
        }
        cell += 1;
    }
    found
}

fn setting(slot: &Slot, band: &str, name: &str) -> Option<f64> {
    let (mut cell, mut blanks) = (0, 0);
    while blanks < CELLS_PAST_THE_LAST {
        match slot.setting_of(cell) {
            Some((found_band, found_name, value)) if found_band == band && found_name == name => return Some(value),
            Some(_) => blanks = 0,
            None => blanks += 1,
        }
        cell += 1;
    }
    None
}

fn rolled(kind: Kind, controls: &[Control], seed: u64) -> Slot {
    let mut dice = Dice(seed);
    let mut slot = Slot::new(kind);
    for control in controls {
        let (choice, travel) = (dice.roll(), dice.roll());
        if choice < 0.25 {
            slot.assign(control.band, control.name, control.low);
        } else if choice < 0.5 {
            slot.assign(control.band, control.name, control.high);
        } else if !slot.params.set_travel(control.band, control.name, travel) {
            slot.assign(control.band, control.name, control.low + (control.high - control.low) * travel);
        }
    }
    slot
}

fn settings(kind: Kind) -> Vec<(String, Slot)> {
    let controls = controls(kind);
    let (mut lowest, mut highest) = (Slot::new(kind), Slot::new(kind));
    for control in &controls {
        lowest.assign(control.band, control.name, control.low);
        highest.assign(control.band, control.name, control.high);
    }
    let mut tried = vec![(String::from("defaults"), Slot::new(kind)), (String::from("every control at its lowest"), lowest), (String::from("every control at its highest"), highest)];
    tried.extend((0..ROLLS).map(|roll| (format!("random setting {roll}"), rolled(kind, &controls, SEED + STRIDE * roll))));
    tried
}

fn finite(heard: &Stereo) -> bool {
    heard.iter().all(|side| peak_of(side).is_finite())
}

fn loudest(heard: &Stereo, from: usize) -> f32 {
    heard.iter().map(|side| peak_of(&side[from..])).fold(0.0f32, |worst, peak| if peak.is_nan() { f32::INFINITY } else { worst.max(peak) })
}

fn replayed(slot: &Slot, before: &Stereo, after: &Stereo, sizes: &[usize]) -> Stereo {
    let mut effect = Effect::new(slot.kind(), SAMPLE_RATE, MAX_FRAMES);
    run(&mut effect, slot, before, before, sizes);
    effect.restart();
    run(&mut effect, slot, after, after, sizes)
}

fn then(mut first: Stereo, second: &Stereo) -> Stereo {
    for (side, more) in first.iter_mut().zip(second) {
        side.extend_from_slice(more);
    }
    first
}

fn failing(check: impl Fn(Kind, &Slot) -> Option<String> + Sync) -> Vec<String> {
    let check = &check;
    std::thread::scope(|scope| {
        let kinds: Vec<_> = Kind::ALL
            .into_iter()
            .map(|kind| scope.spawn(move || settings(kind).into_iter().filter_map(|(label, slot)| check(kind, &slot).map(|why| format!("{}, {label}: {why}", kind.label()))).collect::<Vec<String>>()))
            .collect();
        kinds.into_iter().flat_map(|kind| kind.join().expect("an effect's settings ran to the end")).collect()
    })
}

#[test]
fn a_typed_value_lands_within_its_knobs_own_ends() {
    let mut strays = Vec::new();
    for kind in Kind::ALL {
        for control in controls(kind) {
            let end = |travel: f64| {
                let mut turned = Slot::new(kind);
                turned.params.set_travel(control.band, control.name, travel).then(|| setting(&turned, control.band, control.name)).flatten()
            };
            if let (Some(first), Some(last)) = (end(0.0), end(1.0))
                && (control.low < first.min(last) || control.high > first.max(last))
            {
                strays.push(format!("{} {} {}: typed values reach {} to {}, the knob {} to {}", kind.label(), control.band, control.name, control.low, control.high, first.min(last), first.max(last)));
            }
        }
    }
    assert!(strays.is_empty(), "{strays:#?}");
}

#[test]
fn block_size_never_changes_an_effect_at_any_setting() {
    let (input, key) = (passage(SECONDS), kicks(SECONDS));
    let failures = failing(|_, slot| {
        let uniform = play(slot, &input, &key, &UNIFORM);
        let ragged = play(slot, &input, &key, &RAGGED);
        let worst = worst_delta(&uniform[0], &ragged[0]).max(worst_delta(&uniform[1], &ragged[1]));
        (worst > SAME).then(|| format!("ragged and uniform blocks differ by up to {worst:e}"))
    });
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn no_setting_breaks_an_effects_output() {
    let (input, key) = (passage(SECONDS), kicks(SECONDS));
    let failures = failing(|_, slot| (!finite(&play(slot, &input, &key, &UNIFORM))).then(|| String::from("a sample out is not a number")));
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn silence_in_settles_to_silence_out_at_any_setting() {
    let (input, key) = (silence(SETTLE_SECONDS + LISTEN_SECONDS), kicks(SETTLE_SECONDS + LISTEN_SECONDS));
    let failures = failing(|kind, slot| {
        let mut still = *slot;
        for control in controls(kind).iter().filter(|control| (control.band, control.name) == SWINGS_ON_ITS_OWN) {
            still.assign(control.band, control.name, control.rest);
        }
        let peak = loudest(&play(&still, &input, &key, &UNIFORM), frames(SETTLE_SECONDS));
        (peak > HUSH).then(|| format!("peak {peak:e} a second into silence, over the {HUSH:e} floor"))
    });
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn bad_input_is_contained_at_any_setting() {
    let clean = passage(SECONDS + LISTEN_SECONDS);
    let input: Stereo = [0, 1].map(|side| {
        let mut burst = vec![f32::NAN; BURST];
        burst.extend(vec![f32::INFINITY; BURST]);
        burst.extend(vec![f32::NEG_INFINITY; BURST]);
        burst.extend_from_slice(&clean[side]);
        burst
    });
    let key = kicks(SECONDS + LISTEN_SECONDS + 3.0 * BURST as f64 / SAMPLE_RATE + 1.0 / SAMPLE_RATE);
    let failures = failing(|_, slot| {
        let heard = play(slot, &input, &key, &UNIFORM);
        let tail: Stereo = [0, 1].map(|side| heard[side][heard[side].len() - frames(LISTEN_SECONDS)..].to_vec());
        (!finite(&tail)).then(|| String::from("still not a number half a second after the burst"))
    });
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn once_silent_a_second_play_starts_as_the_first_did_at_any_setting() {
    let input = then(passage(SOUND_SECONDS), &silence(QUIET_SECONDS));
    let key = then(kicks(SOUND_SECONDS), &silence(QUIET_SECONDS));
    let failures = failing(|_, slot| {
        let mut effect = Effect::new(slot.kind(), SAMPLE_RATE, MAX_FRAMES);
        let first = run(&mut effect, slot, &input, &key, &UNIFORM);
        if loudest(&first, first[0].len() - frames(TAIL_SECONDS)) > RINGING {
            return None;
        }
        effect.restart();
        let second = run(&mut effect, slot, &input, &key, &UNIFORM);
        let worst = worst_delta(&first[0], &second[0]).max(worst_delta(&first[1], &second[1]));
        (worst > HUSH).then(|| format!("the second play is up to {worst:e} from the first"))
    });
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn block_size_never_changes_what_a_restart_does_at_any_setting() {
    let before = then(passage(SOUND_SECONDS), &silence(BRIEF_QUIET_SECONDS));
    let after = passage(SOUND_SECONDS);
    let failures = failing(|_, slot| {
        let even = replayed(slot, &before, &after, &UNIFORM);
        let worst = [&RAGGED[..], &LONG[..]].iter().map(|sizes| replayed(slot, &before, &after, sizes)).map(|other| worst_delta(&even[0], &other[0]).max(worst_delta(&even[1], &other[1]))).fold(0.0f32, f32::max);
        (worst > SAME).then(|| format!("the play after a restart differs by up to {worst:e} between block sizes"))
    });
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn a_dynamic_eq_band_restarts_the_same_at_any_block_size() {
    let mut eq = Eq::DEFAULT;
    eq.bands[DYNAMIC_BAND] = EqBand { on: true, gain_db: DEEP_DB, dynamic: true, threshold_db: LOW_THRESHOLD_DB, ratio: STEEP_RATIO, release_ms: EqBand::MAX_RELEASE_MS, ..eq.bands[DYNAMIC_BAND] };
    let slot = Slot { params: Params::Eq(eq), ..Slot::new(Kind::Eq) };
    let side: Vec<f32> = (0..TONE_FRAMES).map(|frame| (TONE_LEVEL * (TAU * TONE_HZ * frame as f64 / SAMPLE_RATE).sin()) as f32).collect();
    let tone: Stereo = [side.clone(), side];
    for quiet_ms in QUIET_MS {
        let before = then(tone.clone(), &silence(quiet_ms * 0.001));
        let even = replayed(&slot, &before, &tone, &UNIFORM);
        for (blocks, sizes) in [("ragged", &RAGGED[..]), ("1024-frame", &LONG[..]), ("single-frame", &SINGLE[..])] {
            let other = replayed(&slot, &before, &tone, sizes);
            let worst = worst_delta(&even[0], &other[0]).max(worst_delta(&even[1], &other[1]));
            assert!(worst <= SAME, "after {quiet_ms} ms of silence and a restart, a dynamic EQ band plays up to {worst:e} apart in {blocks} blocks");
        }
    }
}

#[test]
fn a_restart_leaves_the_utility_alone_while_its_limiter_still_holds_sound() {
    let slot = Slot { params: Params::Utility(Utility { gain_db: PUSHED_DB, limit: true, ceiling_db: CEILING_DB, lookahead_ms: Utility::MAX_LOOKAHEAD_MS, ..Utility::DEFAULT }), ..Slot::new(Kind::Utility) };
    let loud = passage(SOUND_SECONDS);
    let draining: Stereo = [vec![0.0; DRAINING_FRAMES], vec![0.0; DRAINING_FRAMES]];
    let quiet = silence(TAIL_SECONDS);
    let tail = |restarted: bool| {
        let mut effect = Effect::new(Kind::Utility, SAMPLE_RATE, MAX_FRAMES);
        run(&mut effect, &slot, &loud, &loud, &UNIFORM);
        run(&mut effect, &slot, &draining, &draining, &UNIFORM);
        if restarted {
            effect.restart();
        }
        run(&mut effect, &slot, &quiet, &quiet, &UNIFORM)
    };
    let (left, restarted) = (tail(false), tail(true));
    assert!(loudest(&left, 0) > HUSH, "wanted the limiter still letting its delayed sound out {DRAINING_FRAMES} frames into silence");
    let worst = worst_delta(&left[0], &restarted[0]).max(worst_delta(&left[1], &restarted[1]));
    assert!(worst <= SAME, "a restart {DRAINING_FRAMES} frames into silence changed the limiter's draining tail by up to {worst:e}");
}

#[test]
fn an_eq_that_overflowed_passes_sound_again_at_safe_settings() {
    let mut stacked = Eq::DEFAULT;
    for band in &mut stacked.bands {
        (band.on, band.shape, band.hz, band.q) = (true, Shape::LowPassPlus, TOP_HZ, SHARPEST);
    }
    let (wild, calm) = (Slot { params: Params::Eq(stacked), ..Slot::new(Kind::Eq) }, Slot::new(Kind::Eq));
    let (input, key) = (passage(SECONDS), kicks(SECONDS));
    let mut effect = Effect::new(Kind::Eq, SAMPLE_RATE, MAX_FRAMES);
    let overdriven = run(&mut effect, &wild, &input, &key, &UNIFORM);
    assert!(finite(&overdriven), "eight stacked steep bands put out a sample that is not a number");
    assert!(loudest(&overdriven, 0) > OVERFLOWING, "wanted eight stacked steep bands to drive the EQ far past full scale, got a peak of {:e}", loudest(&overdriven, 0));
    run(&mut effect, &calm, &input, &key, &UNIFORM);
    let heard = run(&mut effect, &calm, &input, &key, &UNIFORM);
    let worst = worst_delta(&heard[0], &input[0]).max(worst_delta(&heard[1], &input[1]));
    assert!(worst < RETURNED_WITHIN, "back at its defaults after overflowing, the EQ is up to {worst:e} from its input");
}

#[test]
fn a_delay_restarted_in_silence_drops_echoes_still_on_their_way() {
    let mut slot = Slot::new(Kind::Delay);
    for (band, name, value) in LONG_LINES {
        assert!(slot.assign(band, name, value).is_some(), "wanted the Delay to have a {band} {name} control");
    }
    let before = then(passage(SOUND_SECONDS), &silence(QUIET_SECONDS));
    let after = silence(ECHO_SECONDS);
    let later = |restarted: bool| {
        let mut effect = Effect::new(Kind::Delay, SAMPLE_RATE, MAX_FRAMES);
        run(&mut effect, &slot, &before, &before, &UNIFORM);
        if restarted {
            effect.restart();
        }
        loudest(&run(&mut effect, &slot, &after, &after, &UNIFORM), 0)
    };
    assert!(later(false) > HUSH, "wanted the long lines' echo to arrive when nothing restarts the Delay");
    let peak = later(true);
    assert!(peak <= HUSH, "a Delay restarted in silence still let out an echo from before, peak {peak:e}");
}

#[test]
fn a_small_plate_never_runs_away_as_build_comes_up() {
    let (input, key) = (passage(RUNAWAY_SECONDS), kicks(RUNAWAY_SECONDS));
    for build in BUILDS {
        let slot = Slot { params: Params::Reverb(Reverb { size: SMALLEST_PLATE, build, ..Reverb::DEFAULT }), ..Slot::new(Kind::Reverb) };
        let peak = loudest(&play(&slot, &input, &key, &UNIFORM), 0);
        assert!(peak < TEN_TIMES_FULL_SCALE, "the smallest plate with BUILD at {build} peaks at {peak:e} from a passage that stays under full scale");
    }
}

#[test]
fn a_phase_band_biased_before_drive_is_silent_from_its_first_frame() {
    let mut freak = Freak::DEFAULT;
    freak.bands[0].curve = Some(Curve::Phase);
    freak.bands[0].bias = -Freak::MAX_BIAS;
    freak.bands[0].bias_pre = true;
    let slot = Slot { params: Params::Freak(freak), ..Slot::new(Kind::Freak) };
    let quiet = silence(LISTEN_SECONDS);
    let peak = loudest(&play(&slot, &quiet, &quiet, &UNIFORM), 0);
    assert!(peak <= HUSH, "a Phase band biased before drive puts out a peak of {peak:e} from silence");
}
