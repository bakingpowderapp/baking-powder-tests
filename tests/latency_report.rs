use bp::fx::params::{Compressor, FREAK_BANDS, Freak, FreakSlope, Gate, Lookahead, MOTION_BANDS, Motion, MotionMode, Sidechain, Suppressor, Utility};
use bp::fx::{Clock, Effect, Kind, Params, Slot};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{iter, panic, thread};
use support::{BLOCK, stage};

const RATES: [f64; 3] = [44_100.0, 48_000.0, 96_000.0];
const BPM: f64 = 120.0;
const CLICK: f32 = 0.25;
const SETTLE_SECONDS: f64 = 1.0;
const LEAD_SECONDS: f64 = 0.25;
const TAIL_SECONDS: f64 = 0.25;
const SEARCH_SECONDS: f64 = 0.1;
const HALF_SAMPLE: f64 = 0.5;
const CLOSE: f32 = 1e-4;
const QUIET: f32 = 1e-6;
const BEGINS: f32 = 0.1;
const BESIDE: usize = 8;
const RESPONSE: usize = 4096;
const NONE: [usize; 3] = [0, 0, 0];
const TENTH_MS: [usize; 3] = [3, 4, 9];
const USUAL_LIMITER: [usize; 3] = [65, 71, 143];
const POLISH_LIMITER: [usize; 3] = [105, 114, 229];
const LONGEST_LIMITER: [usize; 3] = [440, 479, 959];
const HALFBANDS: [usize; 3] = [2, 2, 2];
const ONE_MS: [usize; 3] = [44, 48, 96];
const SPECTRAL_HOP: [usize; 3] = [1280, 1280, 1280];
const SINC_HALF: [usize; 3] = [7, 7, 7];
const FIVE_MS: [usize; 3] = [221, 240, 480];
const FIVE_MS_AND_LINEAR: [usize; 3] = [617, 671, 1342];
const LINEAR: [usize; 3] = [396, 431, 862];
const LINEAR_AND_HALFBANDS: [usize; 3] = [398, 433, 864];
const USUAL_MS: f64 = 1.5;
const POLISH_MS: f64 = 2.4;
const LONGEST_MS: f64 = 10.0;
const OPEN_DB: f64 = 0.0;
const ONE_BAND: usize = 1;

const CONVENTION_ZERO: [(Kind, &str); 4] = [
    (Kind::Repeat, "dry at wet zero; a beat repeat in time, no delay"),
    (Kind::Delay, "dry at wet zero; a time effect, zero by convention"),
    (Kind::VolumeLfo, "dry at wet zero; a level moving in time, no delay"),
    (Kind::Reverb, "dry at wet zero; a time effect, zero by convention"),
];

#[derive(Clone, Copy, Debug)]
enum Judge {
    Exact,
    Close,
    Dispersive,
    AllPass,
    Beyond(fn() -> Slot),
}

struct Case {
    option: &'static str,
    slot: Slot,
    expected: [usize; 3],
    judge: Judge,
}

struct Measured {
    kind: Kind,
    option: &'static str,
    rate: f64,
    reported: usize,
    lag: f64,
    worst: (usize, f32),
    faults: Vec<String>,
}

fn with(params: Params) -> Slot {
    Slot { params, ..Slot::new(params.kind()) }
}

fn dry(kind: Kind) -> Slot {
    Slot { wet: 0.0, ..Slot::new(kind) }
}

fn limiter(lookahead_ms: f64) -> Slot {
    with(Params::Utility(Utility { limit: true, ceiling_db: OPEN_DB, lookahead_ms, ..Utility::DEFAULT }))
}

fn resting_gate(lookahead: Lookahead) -> Slot {
    with(Params::Gate(Gate { lookahead, flip: true, threshold_db: OPEN_DB, ..Gate::DEFAULT }))
}

fn wired_freak(slope: FreakSlope, oversample: bool, band_count: usize) -> Slot {
    with(Params::Freak(Freak { slope, oversample, band_count, ..Freak::DEFAULT }))
}

fn soloed_freak(slope: FreakSlope, oversample: bool) -> Slot {
    let mut freak = Freak { slope, oversample, ..Freak::DEFAULT };
    for band in &mut freak.bands {
        band.solo = true;
    }
    with(Params::Freak(freak))
}

fn wired_motion(slope: FreakSlope, band_count: usize) -> Slot {
    with(Params::Motion(Motion { slope, band_count, ..Motion::DEFAULT }))
}

fn banded_suppressor(slope: FreakSlope) -> Slot {
    with(Params::Suppressor(Suppressor { band_count: 2, slope, ..Suppressor::DEFAULT }))
}

fn soloed_motion(slope: FreakSlope) -> Slot {
    let mut motion = Motion { slope, ..Motion::DEFAULT };
    for band in &mut motion.bands {
        band.solo = true;
    }
    with(Params::Motion(motion))
}

fn cases() -> Vec<(Kind, Case)> {
    let case = |option, slot, expected, judge| Case { option, slot, expected, judge };
    let mut cases = vec![
        (Kind::Utility, case("limiter off", Slot::new(Kind::Utility), NONE, Judge::Exact)),
        (Kind::Utility, case("hard clip", with(Params::Utility(Utility { limit: true, hard: true, ceiling_db: OPEN_DB, ..Utility::DEFAULT })), NONE, Judge::Exact)),
        (Kind::Utility, case("look-ahead 0.1 ms", limiter(Utility::MIN_LOOKAHEAD_MS), TENTH_MS, Judge::Exact)),
        (Kind::Utility, case("look-ahead 1.5 ms", limiter(USUAL_MS), USUAL_LIMITER, Judge::Exact)),
        (Kind::Utility, case("look-ahead 2.4 ms", limiter(POLISH_MS), POLISH_LIMITER, Judge::Exact)),
        (Kind::Utility, case("look-ahead 10 ms", limiter(LONGEST_MS), LONGEST_LIMITER, Judge::Exact)),
        (Kind::Compressor, case("look-ahead off", Slot::new(Kind::Compressor), NONE, Judge::Exact)),
        (Kind::Compressor, case("look-ahead 1 ms", with(Params::Compressor(Compressor { lookahead: Lookahead::Ms1, ..Compressor::DEFAULT })), ONE_MS, Judge::Exact)),
        (Kind::Compressor, case("look-ahead 5 ms", with(Params::Compressor(Compressor { lookahead: Lookahead::Ms5, ..Compressor::DEFAULT })), FIVE_MS, Judge::Exact)),
        (Kind::Gate, case("look-ahead off", resting_gate(Lookahead::Off), NONE, Judge::Exact)),
        (Kind::Gate, case("look-ahead 1 ms", resting_gate(Lookahead::Ms1), ONE_MS, Judge::Exact)),
        (Kind::Gate, case("look-ahead 5 ms", resting_gate(Lookahead::Ms5), FIVE_MS, Judge::Exact)),
        (Kind::Sidechain, case("look-ahead off, no key", Slot::new(Kind::Sidechain), NONE, Judge::Exact)),
        (Kind::Sidechain, case("look-ahead 1 ms, no key", with(Params::Sidechain(Sidechain { lookahead: Lookahead::Ms1, ..Sidechain::DEFAULT })), ONE_MS, Judge::Exact)),
        (Kind::Sidechain, case("look-ahead 5 ms, no key", with(Params::Sidechain(Sidechain { lookahead: Lookahead::Ms5, ..Sidechain::DEFAULT })), FIVE_MS, Judge::Exact)),
        (Kind::Freak, case("24 slope, oversampled, a wire", wired_freak(FreakSlope::Linkwitz, true, FREAK_BANDS), HALFBANDS, Judge::Exact)),
        (Kind::Freak, case("24 slope, a wire", wired_freak(FreakSlope::Linkwitz, false, FREAK_BANDS), NONE, Judge::Exact)),
        (Kind::Freak, case("LIN slope, oversampled, a wire", wired_freak(FreakSlope::Linear, true, FREAK_BANDS), LINEAR_AND_HALFBANDS, Judge::Exact)),
        (Kind::Freak, case("LIN slope, a wire", wired_freak(FreakSlope::Linear, false, FREAK_BANDS), LINEAR, Judge::Exact)),
        (Kind::Freak, case("LIN slope, one band, a wire", wired_freak(FreakSlope::Linear, false, ONE_BAND), NONE, Judge::Exact)),
        (Kind::Freak, case("24 slope, every band soloed, oversampled, halfbands beyond the plain split", soloed_freak(FreakSlope::Linkwitz, true), HALFBANDS, Judge::Beyond(|| soloed_freak(FreakSlope::Linkwitz, false)))),
        (Kind::Freak, case("24 slope, every band soloed, all-pass phase from the click", soloed_freak(FreakSlope::Linkwitz, false), NONE, Judge::AllPass)),
        (Kind::Freak, case("12 slope, every band soloed, all-pass phase from the click", soloed_freak(FreakSlope::TwoPole, false), NONE, Judge::AllPass)),
        (Kind::Freak, case("30 slope, every band soloed, all-pass phase from the click", soloed_freak(FreakSlope::Butterworth, false), NONE, Judge::AllPass)),
        (Kind::Freak, case("LIN slope, every band soloed, oversampled", soloed_freak(FreakSlope::Linear, true), LINEAR_AND_HALFBANDS, Judge::Dispersive)),
        (Kind::Freak, case("LIN slope, every band soloed", soloed_freak(FreakSlope::Linear, false), LINEAR, Judge::Close)),
        (Kind::Motion, case("12 slope, a wire", wired_motion(FreakSlope::TwoPole, MOTION_BANDS), NONE, Judge::Exact)),
        (Kind::Motion, case("24 slope, a wire", wired_motion(FreakSlope::Linkwitz, MOTION_BANDS), NONE, Judge::Exact)),
        (Kind::Motion, case("30 slope, a wire", wired_motion(FreakSlope::Butterworth, MOTION_BANDS), NONE, Judge::Exact)),
        (Kind::Motion, case("LIN slope, a wire", wired_motion(FreakSlope::Linear, MOTION_BANDS), LINEAR, Judge::Exact)),
        (Kind::Motion, case("LIN slope, one band, a wire", wired_motion(FreakSlope::Linear, ONE_BAND), NONE, Judge::Exact)),
        (Kind::Motion, case("24 slope, every band soloed, all-pass phase from the click", soloed_motion(FreakSlope::Linkwitz), NONE, Judge::AllPass)),
        (Kind::Motion, case("30 slope, every band soloed, all-pass phase from the click", soloed_motion(FreakSlope::Butterworth), NONE, Judge::AllPass)),
        (Kind::Motion, case("LIN slope, every band soloed", soloed_motion(FreakSlope::Linear), LINEAR, Judge::Close)),
        (Kind::Motion, case("one band, a phaser at its defaults", stage(MotionMode::Phaser), NONE, Judge::Dispersive)),
        (Kind::Eq, case("defaults", Slot::new(Kind::Eq), NONE, Judge::Dispersive)),
        (Kind::Space, case("no key", Slot::new(Kind::Space), SPECTRAL_HOP, Judge::Exact)),
        (Kind::Suppressor, case("defaults, a 5 ms look-ahead", Slot::new(Kind::Suppressor), FIVE_MS, Judge::Close)),
        (Kind::Suppressor, case("24 slope, two bands, all-pass phase from the click", banded_suppressor(FreakSlope::Linkwitz), FIVE_MS, Judge::AllPass)),
        (Kind::Suppressor, case("LIN slope, two bands", banded_suppressor(FreakSlope::Linear), FIVE_MS_AND_LINEAR, Judge::Close)),
        (Kind::Time, case("playing", Slot::new(Kind::Time), SINC_HALF, Judge::Exact)),
        (Kind::Transient, case("defaults", Slot::new(Kind::Transient), NONE, Judge::Dispersive)),
    ];
    for (kind, why) in CONVENTION_ZERO {
        cases.push((kind, case(why, dry(kind), NONE, Judge::Exact)));
    }
    cases
}

fn through(slot: &Slot, rate: f64) -> (Vec<f32>, Vec<f32>, usize) {
    let click = ((SETTLE_SECONDS + LEAD_SECONDS) * rate) as usize;
    let total = click + (TAIL_SECONDS * rate) as usize;
    let mut input = vec![0.0f32; total];
    input[click] = CLICK;
    let mut effect = Effect::new(slot.kind(), rate, BLOCK);
    let clock = Clock { sample_rate: rate, bpm: BPM, beat: None };
    let mut output = Vec::with_capacity(total);
    for chunk in input.chunks(BLOCK) {
        let (mut l, mut r) = (chunk.to_vec(), chunk.to_vec());
        effect.process(&mut l, &mut r, slot, &clock);
        output.extend_from_slice(&l);
    }
    (input, output, click)
}

fn correlation(clicks: &[(usize, f32)], output: &[f32], lag: isize) -> f64 {
    clicks
        .iter()
        .map(|&(n, x)| {
            let at = n as isize + lag;
            if at < 0 || at >= output.len() as isize { 0.0 } else { f64::from(x) * f64::from(output[at as usize]) }
        })
        .sum()
}

fn peak_lag(correlations: &[f64]) -> f64 {
    let peak = (1..correlations.len() - 1).max_by(|a, b| correlations[*a].total_cmp(&correlations[*b])).unwrap_or(1);
    let (before, at, after) = (correlations[peak - 1], correlations[peak], correlations[peak + 1]);
    let curvature = 2.0 * (2.0 * at - before - after);
    let offset = if curvature > 0.0 { (after - before) / curvature } else { 0.0 };
    (peak as f64 - 1.0) + offset
}

fn correlation_lag(input: &[f32], output: &[f32], longest: usize) -> f64 {
    let clicks: Vec<(usize, f32)> = input.iter().copied().enumerate().filter(|(_, x)| *x != 0.0).collect();
    let correlations: Vec<f64> = (-1..=longest as isize + 1).map(|lag| correlation(&clicks, output, lag)).collect();
    peak_lag(&correlations)
}

fn lag_behind(reference: &[f32], output: &[f32], from: usize) -> f64 {
    let correlations: Vec<f64> = (-1..=BESIDE as isize + 1).map(|lag| (from..from + RESPONSE).map(|n| f64::from(reference[n]) * f64::from(output[(n as isize + lag) as usize])).sum()).collect();
    peak_lag(&correlations)
}

fn shifted(input: &[f32], n: usize, by: usize) -> f32 {
    if n >= by { input[n - by] } else { 0.0 }
}

fn first_mismatch(input: &[f32], output: &[f32], by: usize) -> Option<(usize, f32, f32)> {
    output.iter().enumerate().find_map(|(n, &got)| {
        let wanted = shifted(input, n, by);
        (got != wanted).then_some((n, wanted, got))
    })
}

fn worst_stray(input: &[f32], output: &[f32], by: usize) -> (usize, f32) {
    output.iter().enumerate().map(|(n, &got)| (n, (got - shifted(input, n, by)).abs())).fold((0, 0.0), |worst, (n, delta)| if delta > worst.1 { (n, delta) } else { worst })
}

fn beginning(output: &[f32], at: usize) -> Option<String> {
    let before = output[..at].iter().fold(0.0f32, |peak, sample| peak.max(sample.abs()));
    if before >= QUIET {
        return Some(format!("wanted silence under {QUIET} before frame {at}, the click plus its report, got {before}"));
    }
    let first = output[at].abs();
    (first < BEGINS * CLICK).then(|| format!("wanted the all-pass response to begin at frame {at}, the click plus its report, with at least {BEGINS} of the click, got {first}"))
}

fn measure(kind: Kind, case: &Case, index: usize) -> Measured {
    let rate = RATES[index];
    let reported = case.slot.latency(rate);
    let expected = case.expected[index];
    let (input, output, click) = through(&case.slot, rate);
    let lag = match case.judge {
        Judge::Beyond(plain) => {
            let plain = plain();
            let (_, reference, _) = through(&plain, rate);
            plain.latency(rate) as f64 + lag_behind(&reference, &output, click)
        }
        _ => correlation_lag(&input, &output, (SEARCH_SECONDS * rate) as usize),
    };
    let worst = worst_stray(&input, &output, reported);
    let mut faults = Vec::new();
    if reported != expected {
        faults.push(format!("reports {reported}, wanted {expected}"));
    }
    match case.judge {
        Judge::AllPass => faults.extend(beginning(&output, click + expected)),
        Judge::Beyond(_) => {
            if (lag - expected as f64).abs() >= HALF_SAMPLE {
                faults.push(format!("the oversampled response sits {lag:.3} samples after the click, measured against the same split without oversampling, wanted within {HALF_SAMPLE} of {expected}"));
            }
        }
        Judge::Exact | Judge::Close | Judge::Dispersive => {
            if (lag - expected as f64).abs() >= HALF_SAMPLE {
                faults.push(format!("the correlation peak sits {lag:.3} samples after the click at frame {click}, wanted within {HALF_SAMPLE} of {expected}"));
            }
        }
    }
    match case.judge {
        Judge::Exact => {
            if let Some((frame, wanted, got)) = first_mismatch(&input, &output, reported) {
                faults.push(format!("frame {frame} wanted {wanted}, the input {reported} frames earlier, got {got}"));
            }
        }
        Judge::Close => {
            if worst.1 >= CLOSE {
                faults.push(format!("frame {} strays {} from the input {reported} frames earlier, wanted under {CLOSE}", worst.0, worst.1));
            }
        }
        Judge::Dispersive | Judge::AllPass | Judge::Beyond(_) => {}
    }
    Measured { kind, option: case.option, rate, reported, lag, worst, faults }
}

fn side_by_side<T: Sync, R: Send>(jobs: &[T], work: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let workers = thread::available_parallelism().map_or(1, NonZeroUsize::get).min(jobs.len());
    let mut done: Vec<(usize, R)> = thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    iter::from_fn(|| {
                        let at = next.fetch_add(1, Ordering::Relaxed);
                        jobs.get(at).map(|job| (at, work(job)))
                    })
                    .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|handle| handle.join().unwrap_or_else(|panic| panic::resume_unwind(panic))).collect()
    });
    done.sort_unstable_by_key(|&(at, _)| at);
    done.into_iter().map(|(_, result)| result).collect()
}

fn table(rows: &[Measured]) -> String {
    rows.iter()
        .map(|row| {
            let verdict = if row.faults.is_empty() { String::from("ok") } else { row.faults.join("; ") };
            format!("{:<11} {:<26} {:>6} Hz  reported {:>4}  measured {:>9.3}  strays {:.2e} at frame {:<7} {verdict}\n", row.kind.label(), row.option, row.rate, row.reported, row.lag, row.worst.1, row.worst.0)
        })
        .collect()
}

#[test]
fn every_row_kind_is_audited() {
    let audited: Vec<Kind> = cases().into_iter().map(|(kind, _)| kind).collect();
    let missing: Vec<&str> = Kind::ALL.iter().filter(|kind| !audited.contains(kind)).map(|kind| kind.label()).collect();
    assert!(missing.is_empty(), "wanted every row kind in the audit, missing {missing:?}");
}

#[test]
fn every_row_reports_the_delay_a_click_measures() {
    let cases = cases();
    let runs: Vec<(Kind, &Case, usize)> = cases.iter().flat_map(|(kind, case)| (0..RATES.len()).map(move |index| (*kind, case, index))).collect();
    let rows = side_by_side(&runs, |&(kind, case, index)| measure(kind, case, index));
    let failed = rows.iter().filter(|row| !row.faults.is_empty()).count();
    assert!(failed == 0, "{failed} of {} rows failed the audit:\n{}", rows.len(), table(&rows));
}

#[test]
fn bypass_reports_zero_and_wet_zero_the_full_figure() {
    for rate in RATES {
        let armed = limiter(USUAL_MS);
        let bypassed = Slot { bypass: true, ..armed };
        let silent = Slot { wet: 0.0, ..armed };
        assert_eq!(bypassed.latency(rate), 0, "wanted a bypassed limiter to report nothing at {rate} Hz, got {}", bypassed.latency(rate));
        assert_eq!(silent.latency(rate), armed.latency(rate), "wanted a limiter at wet zero to report its full figure at {rate} Hz, got {} against {}", silent.latency(rate), armed.latency(rate));
        let (input, output, _) = through(&silent, rate);
        let mismatch = first_mismatch(&input, &output, armed.latency(rate));
        assert!(mismatch.is_none(), "at wet zero and {rate} Hz frame {:?} wanted the dry delayed by {}, got {:?}", mismatch.map(|m| m.0), armed.latency(rate), mismatch.map(|m| (m.1, m.2)));
    }
}

