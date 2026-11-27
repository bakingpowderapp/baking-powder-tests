use bp::arp::{Arp, Articulation};
use bp::audio::{Audio, Renderer};
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BAR, VELOCITY};
use support::{FRAMES_PER_BEAT, SAMPLE_RATE, SILENT, core_layer, midi};

const BUFFERS: [usize; 4] = [2048, 512, 256, 7];
const PITCH: u8 = 60;
const START: f32 = 0.25;
const LENGTH: f32 = 0.75;
const TAIL_SECONDS: f64 = 0.5;
const GAP: usize = 100;

fn project(with_note: bool) -> Project {
    let mut patch = Params::DEFAULT;
    patch.amp.attack_ms = 1.0;
    patch.amp.sustain = 1.0;
    patch.amp.release_ms = 1.0;
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch))];
    if with_note {
        let notes = vec![Note { pitch: PITCH, start: START, length: LENGTH, velocity: VELOCITY, articulation: Articulation::PLAIN }];
        project.tracks[0].clips.push(Clip { start: 0, length: TICKS_PER_BAR, label: String::new(), notes, file: None });
    }
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

fn held_length(hold: usize, block: usize) -> usize {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, block, Seer::Inline);
    audio.sync(&project(false), None, &[], &[]);
    audio.note_on(0, PITCH, VELOCITY);
    let mut out = Vec::new();
    render(&mut renderer, &mut out, hold, block);
    audio.note_off(0, PITCH);
    render(&mut renderer, &mut out, (SAMPLE_RATE * TAIL_SECONDS) as usize, block);
    let spans = spans(&out);
    assert_eq!(spans.len(), 1, "buffer {block}: one note held {hold} frames gives one span, got {spans:?}");
    spans[0].1 - spans[0].0
}

fn song_span(block: usize) -> (usize, usize) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, block, Seer::Inline);
    audio.sync(&project(true), None, &[], &[]);
    audio.play(0.0);
    let mut out = Vec::new();
    render(&mut renderer, &mut out, (f64::from(START + LENGTH) * FRAMES_PER_BEAT) as usize + (SAMPLE_RATE * TAIL_SECONDS) as usize, block);
    let spans = spans(&out);
    assert_eq!(spans.len(), 1, "buffer {block}: one song note gives one span, got {spans:?}");
    (spans[0].0, spans[0].1 - spans[0].0)
}

#[test]
fn a_song_note_sounds_exactly_its_length_at_any_buffer() {
    let start_frame = (f64::from(START) * FRAMES_PER_BEAT).round() as usize;
    let length_frames = (f64::from(LENGTH) * FRAMES_PER_BEAT).round() as usize;
    for block in BUFFERS {
        let wanted = held_length(length_frames, block);
        let (start, length) = song_span(block);
        assert!(
            start.abs_diff(start_frame) <= 1 && length.abs_diff(wanted) <= 1,
            "buffer {block}: wanted a note from frame {start_frame} sounding {wanted} frames like a key held {length_frames}, got one from {start} sounding {length}"
        );
    }
}
