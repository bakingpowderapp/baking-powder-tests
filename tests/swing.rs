use bp::arp::{Arp, Join, Lane, Pattern};
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::core::Params;
use bp::division::Division;
use bp::json;
use bp::project::{Project, Row, VELOCITY};
use support::{BLOCK, FRAMES_PER_BEAT, GAP, Onsets, SAMPLE_RATE, SILENT, WITHIN, core_layer, midi, unseeded};

const MIDI: u8 = 60;
const HALF_SWING: f64 = 0.5;
const FULL_SWING: f64 = 1.0;
const STRAIGHT: f64 = 0.0;
const FULL_GATE: f64 = 1.0;
const QUARTER_LATE: f64 = 0.25;
const EVEN_SPAN: f64 = 1.25;
const ODD_SPAN: f64 = 0.75;
const LINES: usize = 12;
const HALF_BEAT: f64 = 0.5;
const PAIRS: usize = 4;
const STEP_MS: f64 = 100.0;
const LATE_MS: f64 = 50.0;
const FREE_STEPS: usize = 10;
const TIED_GATE: i8 = 40;
const PERCENT: f64 = 100.0;
const TAIL: usize = 12_000;
const SAVED_SWING: f64 = 0.3;

fn frames(beats: f64) -> usize {
    (beats * FRAMES_PER_BEAT).round() as usize
}

fn ms_frames(ms: f64) -> usize {
    (ms * SAMPLE_RATE / 1000.0).round() as usize
}

fn swung(line: usize, late: f64) -> usize {
    frames(line as f64 + if line % 2 == 1 { late } else { 0.0 })
}

fn project(arp: Arp) -> Project {
    let mut patch = Params::DEFAULT;
    patch.amp.attack_ms = 1.0;
    patch.amp.sustain = 1.0;
    patch.amp.release_ms = 1.0;
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(arp), Row::Core(core_layer(1.0, patch))];
    project
}

fn synced(swing: f64, gate: f64) -> Arp {
    Arp { on: true, retrigger: false, rate: Division::Quarter, gate, swing, ..Arp::DEFAULT }
}

fn resting(slot: usize) -> Pattern {
    let mut pattern = Pattern::DEFAULT;
    pattern.set_length(2);
    pattern.set_rest(slot, true);
    pattern
}

fn render(renderer: &mut Renderer, out: &mut Vec<f32>, total: usize) {
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut done = 0;
    while done < total {
        let n = BLOCK.min(total - done);
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
        done += n;
    }
}

fn heard(arp: Arp, playing: bool, total: usize) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(&project(arp), None, &[], &[]);
    audio.note_on(0, MIDI, VELOCITY);
    if playing {
        audio.play(0.0);
    }
    let mut out = Vec::with_capacity(total);
    render(&mut renderer, &mut out, total);
    out
}

fn onsets(samples: &[f32]) -> Vec<usize> {
    let mut onsets = Onsets::default();
    onsets.feed(samples);
    onsets.at
}

fn spans(samples: &[f32]) -> Vec<(usize, usize)> {
    let (mut spans, mut quiet, mut open) = (Vec::new(), GAP, None);
    for (frame, sample) in samples.iter().enumerate() {
        if sample.abs() < SILENT {
            quiet += 1;
            if quiet == GAP && let Some(start) = open.take() {
                spans.push((start, frame + 1 - GAP));
            }
        } else {
            if quiet >= GAP {
                open = Some(frame);
            }
            quiet = 0;
        }
    }
    spans
}

fn sounding(hold: usize) -> usize {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(&project(Arp::DEFAULT), None, &[], &[]);
    audio.note_on(0, MIDI, VELOCITY);
    let mut out = Vec::new();
    render(&mut renderer, &mut out, hold);
    audio.note_off(0, MIDI);
    render(&mut renderer, &mut out, TAIL);
    let spans = spans(&out);
    assert_eq!(spans.len(), 1, "wanted one note from a key held {hold} frames, got {spans:?}");
    spans[0].1 - spans[0].0
}

fn check(onsets: &[usize], expected: &[usize], what: &str) {
    let missed: Vec<usize> = expected.iter().copied().filter(|&at| !onsets.iter().any(|&got| got.abs_diff(at) <= WITHIN)).collect();
    let stray: Vec<usize> = onsets.iter().copied().filter(|&got| !expected.iter().any(|&at| got.abs_diff(at) <= WITHIN)).collect();
    assert!(missed.is_empty() && stray.is_empty(), "{what}: wanted onsets at frames {expected:?}, got {onsets:?}; missed {missed:?}, stray {stray:?}");
}

fn judge(notes: &[(usize, usize)], expected: &[(usize, usize)], what: &str) {
    let heard: Vec<(usize, usize)> = notes.iter().map(|(start, end)| (*start, end - start)).collect();
    let agree = heard.len() == expected.len() && heard.iter().zip(expected).all(|(got, want)| got.0.abs_diff(want.0) <= WITHIN && got.1.abs_diff(want.1) <= WITHIN);
    assert!(agree, "{what}: wanted notes (start, frames sounding) {expected:?}, got {heard:?}");
}

#[test]
fn a_synced_arp_delays_every_odd_line_by_swing_times_half_a_step() {
    let heard = heard(synced(HALF_SWING, Arp::MIN_GATE), true, frames(LINES as f64 + HALF_BEAT));
    let expected: Vec<usize> = (0..=LINES).map(|line| swung(line, QUARTER_LATE)).collect();
    check(&onsets(&heard), &expected, "swing 0.5 on quarter steps from beat 0");
}

#[test]
fn a_free_arp_counts_its_swing_from_the_key() {
    let arp = Arp { on: true, sync: false, time_ms: STEP_MS, gate: Arp::MIN_GATE, swing: FULL_SWING, ..Arp::DEFAULT };
    let heard = heard(arp, false, ms_frames(FREE_STEPS as f64 * STEP_MS));
    let expected: Vec<usize> = (0..FREE_STEPS).map(|step| ms_frames(step as f64 * STEP_MS + if step % 2 == 1 { LATE_MS } else { 0.0 })).collect();
    check(&onsets(&heard), &expected, "swing 1.0 at 100 ms steps with the transport stopped");
}

#[test]
fn no_swing_leaves_the_grid_straight() {
    let heard = heard(synced(STRAIGHT, Arp::MIN_GATE), true, frames(LINES as f64 + HALF_BEAT));
    let expected: Vec<usize> = (0..=LINES).map(|line| swung(line, 0.0)).collect();
    check(&onsets(&heard), &expected, "swing 0 on quarter steps from beat 0");
}

#[test]
fn a_full_gate_ends_the_even_step_where_the_delayed_odd_one_starts() {
    let total = frames(2.0 * PAIRS as f64 + HALF_BEAT);
    let mut notes = spans(&heard(Arp { pattern: resting(1), ..synced(HALF_SWING, FULL_GATE) }, true, total));
    notes.extend(spans(&heard(Arp { pattern: resting(0), ..synced(HALF_SWING, FULL_GATE) }, true, total)));
    notes.sort_unstable();
    let holds = [frames(EVEN_SPAN), frames(ODD_SPAN)];
    let sounds = holds.map(sounding);
    let mut expected = Vec::new();
    let mut start = 0;
    for line in 0..2 * PAIRS {
        expected.push((start, sounds[line % 2]));
        start += holds[line % 2];
    }
    judge(&notes, &expected, "gate 1.0 and swing 0.5, each parity heard with the other resting");
}

#[test]
fn a_tie_holds_the_note_across_both_swung_steps() {
    let mut pattern = Pattern::DEFAULT;
    pattern.set_length(4);
    pattern.set_join(1, Join::Tie);
    pattern.set(Lane::Gate, 1, TIED_GATE);
    let arp = Arp { on: true, stepped: true, rate: Division::Quarter, swing: HALF_SWING, pattern, ..Arp::DEFAULT };
    let notes = spans(&heard(arp, false, frames(2.0 * PAIRS as f64 + HALF_BEAT)));
    let gate = |step: usize| f64::from(pattern.get(Lane::Gate, step)) / PERCENT;
    let holds = [frames(EVEN_SPAN + ODD_SPAN * gate(1)), frames(EVEN_SPAN * gate(2)), frames(ODD_SPAN * gate(3))];
    let sounds = holds.map(sounding);
    let expected: Vec<(usize, usize)> = [0, 2, 3, 4, 6, 7].iter().zip(sounds.iter().cycle()).map(|(step, sound)| (swung(*step, QUARTER_LATE), *sound)).collect();
    judge(&notes, &expected, "STEP mode, step 1 tied into step 2, swing 0.5");
}

#[test]
fn swing_survives_a_save_and_a_straight_arp_writes_none() {
    let saved = project(Arp { on: true, swing: SAVED_SWING, ..Arp::DEFAULT });
    let text = saved.to_json().write();
    assert!(text.contains("\"swing\""), "wanted swing written into {text}");
    let reopened = Project::from_json(&json::parse(&text).expect("a saved project is valid JSON")).expect("a saved project opens");
    let swing = match reopened.tracks[0].rack.first() {
        Some(Row::Midi(row)) => row.arp.swing,
        _ => panic!("wanted the MIDI row back at the top of the first track from {text}"),
    };
    assert!(swing == SAVED_SWING && unseeded(&reopened.tracks[0].rack) == saved.tracks[0].rack, "wanted swing {SAVED_SWING} back exactly as saved, got {swing} from {text}");
    let straight = project(Arp { on: true, swing: STRAIGHT, ..Arp::DEFAULT }).to_json().write();
    assert!(!straight.contains("swing"), "wanted a straight arp's save without swing, got {straight}");
    let plain = Project::new().to_json().write();
    assert!(!plain.contains("swing"), "wanted a plain project's save without swing, got {plain}");
}
