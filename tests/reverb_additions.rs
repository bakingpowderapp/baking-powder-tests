use bp::arp::Arp;
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params as CoreParams;
use bp::core::params::Envelope;
use bp::fx::params::{Age, EarlyPattern, Filter, Reverb, Tank, Utility};
use bp::fx::{Kind, Params, Slot};
use bp::json;
use bp::project::{Project, Row, VELOCITY};
use support::{AUDIBLE, BLOCK, SAMPLE_RATE, SILENT, core_layer, frames, midi, peak_of, worst_delta};

const SINE_KEY: u8 = 69;
const MID_KEY: u8 = 60;
const HIGH_KEY: u8 = 108;
const SIZE: f64 = 0.25;
const DECAY_S: f64 = 1.0;
const HIGH_CROSSOVER_HZ: f64 = 1500.0;
const SOFT_MS: f64 = 50.0;
const NOTE_SECONDS: f64 = 0.2;
const RENDER_SECONDS: f64 = 1.6;
const STEP_SECONDS: f64 = 0.1;
const LATE: (f64, f64) = (0.4, 1.4);
const FULL_RATE: f64 = 60.0;
const RATE_TOLERANCE: f64 = 0.4;
const ALIKE: f64 = 0.3;
const HALVED_AT_LEAST: f64 = 1.5;
const DOUBLED_AT_MOST: f64 = 0.7;
const CLICK_SECONDS: f64 = 0.01;
const BURST_MS: f64 = 3.0;
const FRONT_SECONDS: f64 = 0.04;
const TAIL_SECONDS: f64 = 0.5;
const CENTRED_DB: f64 = 3.0;
const LEFT_AT_LEAST_DB: f64 = 6.0;
const HALL_DECAY_S: f64 = 2.0;
const HALL_RENDER_SECONDS: f64 = 1.6;
const ENVELOPE_SECONDS: f64 = 0.005;
const HALL_PEAK_BY_SECONDS: f64 = 0.06;
const FROZEN_DECAY_S: f64 = 0.3;
const FREEZE_AT: f64 = 0.5;
const FREEZE_RENDER_SECONDS: f64 = 1.5;
const HELD: (f64, f64) = (0.1, 1.2);
const HOLDS_WITHIN_DB: f64 = 6.0;
const BURST: f32 = 2.0;
const SWAY_MS: f64 = 3.0;
const SWAY_HZ: f64 = 2.0;
const SWAY_SECONDS: f64 = 0.5;
const SWAY_WINDOW: (f64, f64) = (0.2, 0.5);
const LEVEL_WITHIN_DB: f64 = 3.0;
const MOVED: f32 = 0.01;
const PRE_DELAY_MS: f64 = 100.0;
const SPAN_MS: f64 = 40.0;
const NARROWER_BY_DB: f64 = 6.0;
const RAISED_TIMES: f64 = 4.0;
const RAISED_DECAY_S: f64 = 0.5;
const RAISED_RENDER_SECONDS: f64 = 2.0;
const RAISED_EARLY: f64 = 0.3;
const RAISED_LATE: f64 = 1.5;
const FALLEN_BY_DB: f64 = 20.0;
const BOOSTED_DECAY_S: f64 = 0.1;
const BOOSTED_SIZE: f64 = 4.0;
const QUIETER_THAN_DRY_TIMES: f32 = 2.0;
const SLOW_HZ: f64 = 0.05;
const FAST_HZ: f64 = 8.0;
const SWITCH_AT: f64 = 0.5;
const LIVE_SECONDS: f64 = 2.0;
const LIVE_WINDOW: (f64, f64) = (1.0, 2.0);
const MOVEMENT_STEP_SECONDS: f64 = 0.05;
const LIKE_THE_FAST_RATE: f64 = 0.5;
const UNLIKE_THE_SLOW_RATE: f64 = 3.0;
const CREST_SECONDS: f64 = 0.05;
const SHARPER_BY_DB: f64 = 1.5;
const FIRST_MS: f64 = 10.0;
const FRONT_LOUDER_BY_DB: f64 = 5.0;
const FRONT_LEVEL: f64 = 0.6;
const PATTERN_DECAY_S: f64 = 0.3;
const PATTERN_WINDOW_MS: f64 = 45.0;
const PATTERN_LEVEL_WITHIN_DB: f64 = 3.0;
const HALL_PATTERN_LATER_BY_MS: f64 = 8.0;
const BLOOM_MS: f64 = 60.0;
const BLOOM_LATER_BY_SECONDS: f64 = 0.01;
const BLOOM_SPAN_SECONDS: f64 = 0.15;
const DULLER_BY_DB: f64 = 3.0;
const DISPERSED_AT_LEAST_DB: f64 = -10.0;
const AGED_SPAN: (f64, f64) = (0.0, 0.5);
const DISPERSED_WINDOW: (f64, f64) = (0.3, 0.8);
const DISPERSED_LEVEL_WITHIN_DB: f64 = 2.0;

fn sine() -> CoreParams {
    let mut patch = CoreParams::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.amp = Envelope { attack_ms: SOFT_MS, decay_ms: 1.0, sustain: 1.0, release_ms: SOFT_MS, ..Envelope::DEFAULT };
    patch
}

fn click() -> CoreParams {
    let mut patch = CoreParams::DEFAULT;
    patch.osc1.level = 0.0;
    patch.noise.on = true;
    patch.noise.level = 1.0;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 0.0, release_ms: 1.0, ..Envelope::DEFAULT };
    patch
}

fn burst() -> CoreParams {
    let mut patch = CoreParams::DEFAULT;
    patch.osc1.level = 0.0;
    patch.noise.on = true;
    patch.noise.level = 1.0;
    let short = Envelope { attack_ms: 1.0, decay_ms: BURST_MS, sustain: 0.0, release_ms: 1.0, ..Envelope::DEFAULT };
    patch.amp = short;
    patch
}

fn bare(decay_s: f64, size: f64) -> Reverb {
    Reverb { decay_s, size, damping_hz: 20_000.0, filter1: Filter { hz: Filter::MIN_HZ, ..Filter::LOW_CUT }, filter2: Filter { hz: Filter::MAX_HZ, ..Filter::HIGH_CUT }, pre_delay_ms: 0.0, mod_depth: 0.0, ..Reverb::DEFAULT }
}

fn row(params: Reverb) -> Row {
    Row::Effect(Slot { wet: 1.0, params: Params::Reverb(params), ..Slot::new(Kind::Reverb) })
}

fn hard_left() -> Row {
    Row::Effect(Slot { wet: 1.0, params: Params::Utility(Utility { balance: -1.0, ..Utility::DEFAULT }), ..Slot::new(Kind::Utility) })
}

fn project(patch: CoreParams, rows: Vec<Row>) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = [midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch))].into_iter().chain(rows).collect();
    project
}

fn open(project: &Project) -> (Audio, bp::audio::Renderer) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(project, None, &[], &[]);
    (audio, renderer)
}

fn render_to(renderer: &mut bp::audio::Renderer, left: &mut Vec<f32>, right: &mut Vec<f32>, seconds: f64) {
    let end = frames(seconds);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    while left.len() < end {
        let n = BLOCK.min(end - left.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        left.extend_from_slice(&l[..n]);
        right.extend_from_slice(&r[..n]);
    }
}

fn played(project: &Project, key: u8, note_seconds: f64, seconds: f64) -> (Vec<f32>, Vec<f32>) {
    let (audio, mut renderer) = open(project);
    audio.note_on(0, key, VELOCITY);
    let (mut left, mut right) = (Vec::new(), Vec::new());
    render_to(&mut renderer, &mut left, &mut right, note_seconds);
    audio.note_off(0, key);
    render_to(&mut renderer, &mut left, &mut right, seconds);
    (left, right)
}

fn energy(samples: &[f32]) -> f64 {
    samples.iter().map(|&x| f64::from(x) * f64::from(x)).sum()
}

fn db(ratio: f64) -> f64 {
    10.0 * ratio.max(1e-30).log10()
}

fn rms_db(samples: &[f32], from: f64) -> f64 {
    let span = &samples[frames(from)..frames(from + STEP_SECONDS)];
    db(energy(span) / span.len() as f64)
}

fn rate(samples: &[f32], (from, to): (f64, f64)) -> f64 {
    let points: Vec<(f64, f64)> = (0..).map(|k| from + k as f64 * STEP_SECONDS).take_while(|at| *at < to).map(|at| (at, rms_db(samples, at))).collect();
    let n = points.len() as f64;
    let (mean_t, mean_db) = (points.iter().map(|(t, _)| t).sum::<f64>() / n, points.iter().map(|(_, db)| db).sum::<f64>() / n);
    let slope = points.iter().map(|(t, db)| (t - mean_t) * (db - mean_db)).sum::<f64>() / points.iter().map(|(t, _)| (t - mean_t).powi(2)).sum::<f64>();
    -slope
}

fn expect_rate(got: f64, wanted: f64, what: &str) {
    assert!((got - wanted).abs() < RATE_TOLERANCE * wanted, "wanted {what} to fall about {wanted:.0} dB a second, got {got:.1}");
}

fn treble_tail(key: u8, high_decay: f64) -> Vec<f32> {
    let params = Reverb { high_decay, high_crossover_hz: HIGH_CROSSOVER_HZ, ..bare(DECAY_S, SIZE) };
    played(&project(sine(), vec![row(params)]), key, NOTE_SECONDS, RENDER_SECONDS).0
}

fn onset(left: &[f32], right: &[f32]) -> usize {
    left.iter().zip(right).position(|(l, r)| l.abs() > SILENT || r.abs() > SILENT).expect("wanted the tail to sound")
}

fn front_balance_db(tank: Tank, stereo: f64) -> f64 {
    let params = Reverb { tank, stereo, ..bare(DECAY_S, 0.5) };
    let (left, right) = played(&project(click(), vec![hard_left(), row(params)]), MID_KEY, CLICK_SECONDS, TAIL_SECONDS);
    let start = onset(&left, &right);
    let end = (start + frames(FRONT_SECONDS)).min(left.len());
    db(energy(&left[start..end]) / energy(&right[start..end]))
}

fn middle(left: &[f32], right: &[f32]) -> Vec<f32> {
    left.iter().zip(right).map(|(l, r)| 0.5 * (l + r)).collect()
}

fn envelope_peak_seconds(left: &[f32], right: &[f32]) -> f64 {
    let window = frames(ENVELOPE_SECONDS);
    let start = onset(left, right);
    let mid = middle(left, right);
    let mut loudest = (0.0, start);
    let mut at = start;
    while at + window <= mid.len() {
        let power = energy(&mid[at..at + window]);
        if power > loudest.0 {
            loudest = (power, at);
        }
        at += window;
    }
    (loudest.1 - start) as f64 / SAMPLE_RATE
}

fn hall(tank: Tank) -> Project {
    project(burst(), vec![row(Reverb { tank, decay_s: HALL_DECAY_S, pre_delay_ms: 0.0, mod_depth: 0.0, ..Reverb::DEFAULT })])
}

#[test]
fn at_times_one_the_treble_and_the_mid_decay_alike() {
    let high = rate(&treble_tail(HIGH_KEY, 1.0), LATE);
    let mid = rate(&treble_tail(MID_KEY, 1.0), LATE);
    expect_rate(mid, FULL_RATE, "a 262 Hz tail at a decay of 1 s");
    assert!((high - mid).abs() < ALIKE * mid, "wanted a 4 kHz tail to fall as fast as the 262 Hz one at treble times 1, got {high:.1} against {mid:.1} dB a second");
}

#[test]
fn under_one_the_treble_dies_faster_and_over_one_it_lingers_while_the_mid_keeps_its_time() {
    let whole = rate(&treble_tail(HIGH_KEY, 1.0), LATE);
    let halved = rate(&treble_tail(HIGH_KEY, 0.5), LATE);
    assert!(halved > HALVED_AT_LEAST * whole, "wanted a 4 kHz tail at treble times 0.5 to fall at least {HALVED_AT_LEAST} times as fast as at times 1, got {halved:.1} against {whole:.1} dB a second");
    let doubled = rate(&treble_tail(HIGH_KEY, 2.0), LATE);
    assert!(doubled < DOUBLED_AT_MOST * whole, "wanted a 4 kHz tail at treble times 2 to fall under {DOUBLED_AT_MOST} of its rate at times 1, got {doubled:.1} against {whole:.1} dB a second");
    for times in [0.5, 2.0] {
        expect_rate(rate(&treble_tail(MID_KEY, times), LATE), FULL_RATE, &format!("a 262 Hz tail with the treble at times {times}"));
    }
}

#[test]
fn a_hard_left_sound_starts_its_tail_in_the_centre_without_the_stereo_input_and_on_the_left_with_it() {
    for tank in [Tank::Plate, Tank::Hall] {
        let summed = front_balance_db(tank, 0.0);
        assert!(summed.abs() < CENTRED_DB, "wanted the {tank:?} tank's first {FRONT_SECONDS} s balanced with the stereo input off, got {summed:+.1} dB left over right");
        let kept = front_balance_db(tank, 1.0);
        assert!(kept > LEFT_AT_LEAST_DB, "wanted the {tank:?} tank's first {FRONT_SECONDS} s at least {LEFT_AT_LEAST_DB} dB left with the stereo input on, got {kept:+.1} dB");
    }
}

#[test]
fn the_hall_tank_peaks_within_sixty_milliseconds_and_decays_at_its_knob() {
    let (plate_l, plate_r) = played(&hall(Tank::Plate), MID_KEY, CLICK_SECONDS, HALL_RENDER_SECONDS);
    let (hall_l, hall_r) = played(&hall(Tank::Hall), MID_KEY, CLICK_SECONDS, HALL_RENDER_SECONDS);
    let plate_peak = envelope_peak_seconds(&plate_l, &plate_r);
    let hall_peak = envelope_peak_seconds(&hall_l, &hall_r);
    assert!(hall_peak < HALL_PEAK_BY_SECONDS && hall_peak < plate_peak, "wanted the hall's middle to peak within {HALL_PEAK_BY_SECONDS} s and before the plate's, got {hall_peak:.3} s against the plate's {plate_peak:.3}");
    expect_rate(rate(&middle(&hall_l, &hall_r), LATE), FULL_RATE / HALL_DECAY_S, "the hall's tail at a decay of 2 s");
}

#[test]
fn a_frozen_hall_neither_bursts_nor_fades() {
    let before_freeze = project(click(), vec![row(Reverb { tank: Tank::Hall, ..bare(FROZEN_DECAY_S, 1.0) })]);
    let frozen = project(click(), vec![row(Reverb { tank: Tank::Hall, freeze: true, ..bare(FROZEN_DECAY_S, 1.0) })]);
    let (mut audio, mut renderer) = open(&before_freeze);
    audio.note_on(0, MID_KEY, VELOCITY);
    let (mut left, mut right) = (Vec::new(), Vec::new());
    render_to(&mut renderer, &mut left, &mut right, FREEZE_AT);
    let before = peak_of(&left);
    assert!(before > AUDIBLE, "wanted the hall to sound before the freeze, got a peak of {before}");
    audio.sync(&frozen, None, &[], &[]);
    render_to(&mut renderer, &mut left, &mut right, FREEZE_AT + FREEZE_RENDER_SECONDS);
    let loudest = peak_of(&left[frames(FREEZE_AT)..]);
    assert!(loudest < BURST * before, "wanted the frozen hall to hold near its level, got a peak of {loudest} against {before} before the freeze");
    let early = rms_db(&left, FREEZE_AT + HELD.0);
    let late = rms_db(&left, FREEZE_AT + HELD.1);
    assert!((early - late).abs() < HOLDS_WITHIN_DB, "wanted the frozen hall to hold within {HOLDS_WITHIN_DB} dB over a second, got {early:.1} dB then {late:.1}");
}

#[test]
fn random_sway_moves_the_tail_without_changing_its_level() {
    let swayed = |random: f64| {
        let params = Reverb { mod_depth: SWAY_MS, mod_rate_hz: SWAY_HZ, mod_random: random, ..bare(DECAY_S, 1.0) };
        played(&project(sine(), vec![row(params)]), SINE_KEY, SWAY_SECONDS, SWAY_SECONDS).0
    };
    let (sine_sway, random_sway) = (swayed(0.0), swayed(1.0));
    let window = frames(SWAY_WINDOW.0)..frames(SWAY_WINDOW.1);
    let gap = db(energy(&random_sway[window.clone()]) / energy(&sine_sway[window.clone()]));
    assert!(gap.abs() < LEVEL_WITHIN_DB, "wanted the random sway within {LEVEL_WITHIN_DB} dB of the sine sway, got {gap:+.1} dB");
    let moved = worst_delta(&random_sway[window.clone()], &sine_sway[window]);
    assert!(moved > MOVED, "wanted the random sway to move the tail, got a worst difference of {moved}");
}

#[test]
fn early_width_zero_lands_the_reflections_in_the_centre() {
    let side_over_mid = |early_width: f64| {
        let params = Reverb { early_level: 1.0, early_span_ms: SPAN_MS, early_width, pre_delay_ms: PRE_DELAY_MS, ..bare(FROZEN_DECAY_S, SIZE) };
        let (left, right) = played(&project(click(), vec![row(params)]), MID_KEY, CLICK_SECONDS, TAIL_SECONDS);
        let start = onset(&left, &right);
        let end = (start + frames(SPAN_MS * 0.001)).min(left.len());
        let (mid, side): (Vec<f32>, Vec<f32>) = left[start..end].iter().zip(&right[start..end]).map(|(l, r)| (0.5 * (l + r), 0.5 * (l - r))).unzip();
        db(energy(&side) / energy(&mid))
    };
    let (wide, centred) = (side_over_mid(1.0), side_over_mid(0.0));
    assert!(centred < wide - NARROWER_BY_DB, "wanted the reflections at width 0 at least {NARROWER_BY_DB} dB narrower than at width 1, got {centred:+.1} against {wide:+.1} dB side over mid");
}

fn movement(samples: &[f32], (from, to): (f64, f64)) -> f64 {
    let step = frames(MOVEMENT_STEP_SECONDS);
    let levels: Vec<f64> = (frames(from)..frames(to) - step).step_by(step).map(|at| db(energy(&samples[at..at + step]) / step as f64)).collect();
    levels.windows(2).map(|pair| (pair[1] - pair[0]).abs()).sum::<f64>() / (levels.len() - 1) as f64
}

fn swayed_live(before_hz: f64, after_hz: f64) -> Vec<f32> {
    let swaying = |rate_hz: f64| project(sine(), vec![row(Reverb { mod_depth: SWAY_MS, mod_rate_hz: rate_hz, mod_random: 1.0, ..bare(DECAY_S, 1.0) })]);
    let (mut audio, mut renderer) = open(&swaying(before_hz));
    audio.note_on(0, SINE_KEY, VELOCITY);
    let (mut left, mut right) = (Vec::new(), Vec::new());
    render_to(&mut renderer, &mut left, &mut right, SWITCH_AT);
    audio.sync(&swaying(after_hz), None, &[], &[]);
    render_to(&mut renderer, &mut left, &mut right, LIVE_SECONDS);
    left
}

#[test]
fn raised_bass_and_treble_multipliers_together_still_decay() {
    let params = Reverb { low_decay: RAISED_TIMES, high_decay: RAISED_TIMES, ..bare(RAISED_DECAY_S, 1.0) };
    let (left, right) = played(&project(burst(), vec![row(params)]), MID_KEY, CLICK_SECONDS, RAISED_RENDER_SECONDS);
    let mid = middle(&left, &right);
    let (early, late) = (rms_db(&mid, RAISED_EARLY), rms_db(&mid, RAISED_LATE));
    assert!(late < early - FALLEN_BY_DB, "wanted a plate at {RAISED_DECAY_S} s with bass and treble at times {RAISED_TIMES} to fall at least {FALLEN_BY_DB} dB from {RAISED_EARLY} to {RAISED_LATE} s, got {early:.1} dB then {late:.1}");
}

#[test]
fn a_short_hall_at_full_size_with_bass_times_four_stays_under_its_source() {
    let params = Reverb { tank: Tank::Hall, size: BOOSTED_SIZE, low_decay: RAISED_TIMES, decay_s: BOOSTED_DECAY_S, pre_delay_ms: 0.0, mod_depth: 0.0, ..Reverb::DEFAULT };
    let (wet_l, wet_r) = played(&project(burst(), vec![row(params)]), MID_KEY, CLICK_SECONDS, TAIL_SECONDS);
    let (dry_l, dry_r) = played(&project(burst(), vec![]), MID_KEY, CLICK_SECONDS, TAIL_SECONDS);
    let (wet, dry) = (peak_of(&wet_l).max(peak_of(&wet_r)), peak_of(&dry_l).max(peak_of(&dry_r)));
    assert!(wet < QUIETER_THAN_DRY_TIMES * dry, "wanted a hall at {BOOSTED_DECAY_S} s, size {BOOSTED_SIZE} and bass times {RAISED_TIMES} to peak under {QUIETER_THAN_DRY_TIMES} times its source, got {wet} against {dry}");
}

#[test]
fn random_sway_follows_a_rate_change_while_playing() {
    let switched = movement(&swayed_live(SLOW_HZ, FAST_HZ), LIVE_WINDOW);
    let fast = movement(&swayed_live(FAST_HZ, FAST_HZ), LIVE_WINDOW);
    let slow = movement(&swayed_live(SLOW_HZ, SLOW_HZ), LIVE_WINDOW);
    assert!(switched > LIKE_THE_FAST_RATE * fast, "wanted the tail to move after a switch from {SLOW_HZ} to {FAST_HZ} Hz as it does at {FAST_HZ} Hz throughout, got {switched:.2} against {fast:.2} dB a step");
    assert!(switched > UNLIKE_THE_SLOW_RATE * slow, "wanted the tail to move after the switch far more than at {SLOW_HZ} Hz throughout, got {switched:.2} against {slow:.2} dB a step");
}

fn crest_db(left: &[f32], right: &[f32]) -> f64 {
    let start = onset(left, right);
    let end = (start + frames(CREST_SECONDS)).min(left.len());
    let mid = middle(&left[start..end], &right[start..end]);
    let peak = f64::from(peak_of(&mid));
    db(peak * peak * mid.len() as f64 / energy(&mid))
}

fn first_share_db(left: &[f32], right: &[f32]) -> f64 {
    let start = onset(left, right);
    let first = (start + frames(FIRST_MS * 0.001)).min(left.len());
    let span = (start + frames(FRONT_SPAN_FOR_SHARE)).min(left.len());
    db((energy(&left[start..first]) + energy(&right[start..first])) / (energy(&left[start..span]) + energy(&right[start..span])))
}

const FRONT_SPAN_FOR_SHARE: f64 = 0.2;

fn pattern_window(pattern: EarlyPattern) -> (Vec<f32>, Vec<f32>) {
    let params = Reverb { early_level: 1.0, early_span_ms: SPAN_MS, early_pattern: pattern, pre_delay_ms: PRE_DELAY_MS, ..bare(PATTERN_DECAY_S, SIZE) };
    let (left, right) = played(&project(burst(), vec![row(params)]), MID_KEY, CLICK_SECONDS, TAIL_SECONDS);
    let start = onset(&left, &right);
    let end = (start + frames(PATTERN_WINDOW_MS * 0.001)).min(left.len());
    (left[start..end].to_vec(), right[start..end].to_vec())
}

fn centroid_ms(left: &[f32], right: &[f32]) -> f64 {
    let power: Vec<f64> = left.iter().zip(right).map(|(l, r)| f64::from(l * l + r * r)).collect();
    let total: f64 = power.iter().sum();
    power.iter().enumerate().map(|(at, p)| at as f64 * p).sum::<f64>() / total / SAMPLE_RATE * 1000.0
}

fn bloom_peak_seconds(shape: f64) -> f64 {
    let params = Reverb { attack_ms: BLOOM_MS, attack_shape: shape, ..bare(DECAY_S, SIZE) };
    let (left, right) = played(&project(burst(), vec![row(params)]), MID_KEY, CLICK_SECONDS, TAIL_SECONDS);
    let start = onset(&left, &right);
    let end = (start + frames(BLOOM_SPAN_SECONDS)).min(left.len());
    envelope_peak_seconds(&left[start..end], &right[start..end])
}

fn brightness_db(samples: &[f32], (from, to): (f64, f64)) -> f64 {
    let span = &samples[frames(from)..frames(to).min(samples.len())];
    let edges: f64 = span.windows(2).map(|pair| f64::from(pair[1] - pair[0]).powi(2)).sum();
    db(edges / energy(span))
}

#[test]
fn the_newer_keys_read_their_defaults_and_round_trip() {
    let Some(Row::Effect(Slot { params: Params::Reverb(read), .. })) = Row::from_json(&json::parse(r#"{"kind": "reverb"}"#).expect("wanted the text to parse")) else {
        panic!("wanted a reverb row");
    };
    assert!(read.input_diffusion == 1.0 && read.hall_front == Reverb::HALL_FRONT && read.attack_shape == 0.0 && read.disperse == 0.0 && read.early_pattern == EarlyPattern::Scatter && read.age == Age::Now, "wanted the newer keys at their defaults");
    let saved = row(Reverb { early_pattern: EarlyPattern::Room, age: Age::Seventies, disperse: 0.5, ..Reverb::DEFAULT }).to_json();
    let Some(Row::Effect(Slot { params: Params::Reverb(back), .. })) = Row::from_json(&saved) else {
        panic!("wanted the saved reverb to read back");
    };
    assert!(back.early_pattern == EarlyPattern::Room && back.age == Age::Seventies && back.disperse == 0.5, "wanted the room pattern, the 1970s and the dispersion back from the JSON");
}

#[test]
fn input_diffusion_zero_leaves_the_front_sharp() {
    let rendered = |input_diffusion: f64| played(&project(burst(), vec![row(Reverb { input_diffusion, ..bare(DECAY_S, 1.0) })]), MID_KEY, CLICK_SECONDS, TAIL_SECONDS);
    let (smeared_l, smeared_r) = rendered(1.0);
    let (sharp_l, sharp_r) = rendered(0.0);
    let (smeared, sharp) = (crest_db(&smeared_l, &smeared_r), crest_db(&sharp_l, &sharp_r));
    assert!(sharp > smeared + SHARPER_BY_DB, "wanted the first {CREST_SECONDS} s at input diffusion 0 at least {SHARPER_BY_DB} dB spikier than at 1, got a crest of {sharp:.1} dB against {smeared:.1}");
}

#[test]
fn the_halls_front_level_sets_its_first_milliseconds() {
    let rendered = |hall_front: f64| played(&project(burst(), vec![row(Reverb { tank: Tank::Hall, hall_front, decay_s: HALL_DECAY_S, pre_delay_ms: 0.0, mod_depth: 0.0, ..Reverb::DEFAULT })]), MID_KEY, CLICK_SECONDS, TAIL_SECONDS);
    let (none_l, none_r) = rendered(0.0);
    let (full_l, full_r) = rendered(FRONT_LEVEL);
    let (none, full) = (first_share_db(&none_l, &none_r), first_share_db(&full_l, &full_r));
    assert!(full > none + FRONT_LOUDER_BY_DB, "wanted the hall's first {FIRST_MS} ms at front {FRONT_LEVEL} at least {FRONT_LOUDER_BY_DB} dB louder a share than at front 0, got {full:.1} against {none:.1} dB");
}

#[test]
fn early_patterns_keep_their_level_and_the_hall_pattern_lands_later() {
    let (scatter_l, scatter_r) = pattern_window(EarlyPattern::Scatter);
    let scatter = energy(&scatter_l) + energy(&scatter_r);
    let windows = [EarlyPattern::Room, EarlyPattern::Chamber, EarlyPattern::Stage, EarlyPattern::Hall].map(|pattern| (pattern, pattern_window(pattern)));
    for (pattern, (l, r)) in &windows {
        let gap = db((energy(l) + energy(r)) / scatter);
        assert!(gap.abs() < PATTERN_LEVEL_WITHIN_DB, "wanted the {pattern:?} pattern within {PATTERN_LEVEL_WITHIN_DB} dB of Scatter over the first {PATTERN_WINDOW_MS} ms, got {gap:+.1} dB");
    }
    let [.., (_, (hall_l, hall_r))] = &windows;
    let (scatter_at, hall_at) = (centroid_ms(&scatter_l, &scatter_r), centroid_ms(hall_l, hall_r));
    assert!(hall_at > scatter_at + HALL_PATTERN_LATER_BY_MS, "wanted the Hall pattern's energy at least {HALL_PATTERN_LATER_BY_MS} ms later than Scatter's, got {hall_at:.1} ms against {scatter_at:.1}");
}

#[test]
fn attack_shape_moves_the_bloom_late_or_early() {
    let (early, late) = (bloom_peak_seconds(-1.0), bloom_peak_seconds(1.0));
    assert!(late > early + BLOOM_LATER_BY_SECONDS, "wanted a {BLOOM_MS} ms attack to peak at least {BLOOM_LATER_BY_SECONDS} s later at shape +1 than at -1, got {late:.3} s against {early:.3}");
}

#[test]
fn the_seventies_dull_the_tail() {
    let rendered = |age: Age| played(&project(burst(), vec![row(Reverb { age, ..bare(DECAY_S, 1.0) })]), MID_KEY, CLICK_SECONDS, TAIL_SECONDS).0;
    let (now, then) = (brightness_db(&rendered(Age::Now), AGED_SPAN), brightness_db(&rendered(Age::Seventies), AGED_SPAN));
    assert!(then < now - DULLER_BY_DB, "wanted the 1970s at least {DULLER_BY_DB} dB duller than Now over the first half second, got {then:.1} against {now:.1} dB of edge");
}

#[test]
fn dispersion_changes_the_tail_without_changing_its_level() {
    let rendered = |disperse: f64| played(&project(burst(), vec![row(Reverb { disperse, ..bare(DECAY_S, 1.0) })]), MID_KEY, CLICK_SECONDS, TAIL_SECONDS + 0.5).0;
    let (plain, dispersed) = (rendered(0.0), rendered(1.0));
    let window = frames(DISPERSED_WINDOW.0)..frames(DISPERSED_WINDOW.1);
    let gap = db(energy(&dispersed[window.clone()]) / energy(&plain[window.clone()]));
    assert!(gap.abs() < DISPERSED_LEVEL_WITHIN_DB, "wanted full dispersion within {DISPERSED_LEVEL_WITHIN_DB} dB of none, got {gap:+.1} dB");
    let difference: Vec<f32> = dispersed[window.clone()].iter().zip(&plain[window.clone()]).map(|(a, b)| a - b).collect();
    let changed = db(energy(&difference) / energy(&plain[window]));
    assert!(changed > DISPERSED_AT_LEAST_DB, "wanted full dispersion to change the tail by at least {DISPERSED_AT_LEAST_DB} dB of its energy, got {changed:.1} dB");
}
