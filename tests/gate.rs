use bp::arp::Arp;
use bp::audio::{Audio, Renderer};
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::division::Division;
use bp::project::{DEFAULT_BPM, Project, Row, VELOCITY};
use support::{SAMPLE_RATE, SILENT, core_layer, midi};

const BUFFERS: [usize; 3] = [2048, 512, 256];
const MIDI: u8 = 60;
const GAP: usize = 100;
const TAIL_SECONDS: f64 = 1.0;
const SECONDS: f64 = 3.0;
const GATE_FLOOR: usize = 48;
const FAST_BPM: f64 = 300.0;

fn project(bpm: f64, arp: Arp) -> Project {
    let mut patch = Params::DEFAULT;
    patch.amp.attack_ms = 1.0;
    patch.amp.sustain = 1.0;
    patch.amp.release_ms = 1.0;
    let mut project = Project::new();
    project.bpm = bpm;
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(arp), Row::Core(core_layer(1.0, patch))];
    project
}

fn render(renderer: &mut Renderer, out: &mut Vec<f32>, frames: usize, block: usize) {
    let (mut l, mut r) = (vec![0.0; block], vec![0.0; block]);
    let mut done = 0;
    while done < frames {
        let n = block.min(frames - done);
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
        done += n;
    }
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

fn held_length(bpm: f64, hold: usize, block: usize) -> usize {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, block, Seer::Inline);
    audio.sync(&project(bpm, Arp::DEFAULT), None, &[], &[]);
    audio.note_on(0, MIDI, VELOCITY);
    let mut out = Vec::new();
    render(&mut renderer, &mut out, hold, block);
    audio.note_off(0, MIDI);
    render(&mut renderer, &mut out, (SAMPLE_RATE * TAIL_SECONDS) as usize, block);
    let spans = spans(&out);
    assert_eq!(spans.len(), 1, "one note held {hold} frames gives one span, got {spans:?}");
    spans[0].1 - spans[0].0
}

fn arp_lengths(bpm: f64, arp: Arp, block: usize) -> Vec<usize> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, block, Seer::Inline);
    audio.sync(&project(bpm, arp), None, &[], &[]);
    audio.note_on(0, MIDI, VELOCITY);
    let mut out = Vec::new();
    render(&mut renderer, &mut out, (SAMPLE_RATE * SECONDS) as usize, block);
    spans(&out).iter().map(|(start, end)| end - start).collect()
}

fn check(bpm: f64, arp: Arp, gate: usize, at_least: usize) {
    for block in BUFFERS {
        let wanted = held_length(bpm, gate, block);
        let lengths = arp_lengths(bpm, arp, block);
        let off: Vec<usize> = lengths.iter().copied().filter(|length| length.abs_diff(wanted) > 1).collect();
        assert!(lengths.len() >= at_least && off.is_empty(), "buffer {block}: a note held {gate} frames sounds for {wanted}; {} arp notes sound for {lengths:?}", lengths.len());
    }
}

#[test]
fn an_arp_note_sounds_exactly_its_gate_at_any_buffer() {
    let arp = Arp { on: true, rate: Division::ThirtySecondDotted, gate: 0.383, ..Arp::DEFAULT };
    let step = arp.rate.beats() * SAMPLE_RATE * 60.0 / DEFAULT_BPM;
    let gate = (step * arp.gate).round() as usize;
    check(DEFAULT_BPM, arp, gate, (SECONDS * SAMPLE_RATE / step) as usize - 1);
}

#[test]
fn a_tiny_gate_holds_the_floor() {
    let arp = Arp { on: true, rate: Division::SixtyFourth, gate: Arp::MIN_GATE, ..Arp::DEFAULT };
    let step = arp.rate.beats() * SAMPLE_RATE * 60.0 / FAST_BPM;
    assert!(((step * arp.gate).round() as usize) < GATE_FLOOR, "the gate must ask for less than the floor to test it");
    check(FAST_BPM, arp, GATE_FLOOR, (SECONDS * SAMPLE_RATE / step) as usize - 1);
}
