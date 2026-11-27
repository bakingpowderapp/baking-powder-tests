use bp::arp::{Arp, Articulation, MidiRow, Style};
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::core::Params;
use bp::core::params::Envelope;
use bp::division::Division;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT};
use support::{AUDIBLE, BLOCK, FRAMES_PER_BEAT, SAMPLE_RATE, channels_apart, core_layer, frames, peak_of, song_render};

const BEATS: f64 = 4.0;
const TAIL_SECONDS: f64 = 0.5;
const SWITCH_BEATS: f64 = 1.6;
const KEYS: [(u8, f32, f32, u8); 4] = [(60, 0.0, 4.0, 100), (64, 0.0, 4.0, 80), (67, 1.0, 3.0, 120), (72, 2.5, 1.5, 90)];
const LEVEL: f64 = 0.5;

fn arp() -> Arp {
    Arp { on: true, style: Style::UpDown, rate: Division::Sixteenth, gate: 0.5, steps: 2, ..Arp::DEFAULT }
}

fn patch() -> Params {
    let mut patch = Params::DEFAULT;
    patch.amp = Envelope { attack_ms: 2.0, decay_ms: 80.0, sustain: 0.6, release_ms: 30.0, ..Envelope::DEFAULT };
    patch
}

fn project(row: MidiRow) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![Row::Midi(row), Row::Core(core_layer(LEVEL, patch()))];
    let notes = KEYS.iter().map(|&(pitch, start, length, velocity)| Note { pitch, start, length, velocity, articulation: Articulation::PLAIN }).collect();
    project.tracks[0].clips.push(Clip { start: 0, length: BEATS as i32 * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn end() -> usize {
    (BEATS * FRAMES_PER_BEAT).round() as usize + frames(TAIL_SECONDS)
}

fn switch() -> usize {
    (SWITCH_BEATS * FRAMES_PER_BEAT).round() as usize
}

fn loudest(heard: &[Vec<f32>; 2], from: usize) -> f32 {
    peak_of(&heard[0][from..]).max(peak_of(&heard[1][from..]))
}

fn render(renderer: &mut Renderer, heard: &mut [Vec<f32>; 2], until: usize) {
    let (mut l, mut r) = ([0.0; BLOCK], [0.0; BLOCK]);
    while heard[0].len() < until {
        let n = BLOCK.min(until - heard[0].len());
        renderer.render(&mut l[..n], &mut r[..n]);
        heard[0].extend_from_slice(&l[..n]);
        heard[1].extend_from_slice(&r[..n]);
    }
}

fn switched(change: fn(&mut MidiRow)) -> [Vec<f32>; 2] {
    let mut project = project(MidiRow { arp: arp(), ..MidiRow::DEFAULT });
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(&project, None, &[], &[]);
    audio.play(0.0);
    let mut heard = [Vec::new(), Vec::new()];
    render(&mut renderer, &mut heard, switch());
    if let Row::Midi(row) = &mut project.tracks[0].rack[0] {
        change(row);
    }
    audio.sync(&project, None, &[], &[]);
    render(&mut renderer, &mut heard, end());
    heard
}

#[test]
fn a_bypassed_arp_passes_the_notes_through_untouched() {
    let bypassed = song_render(&project(MidiRow { bypass: true, arp: arp(), ..MidiRow::DEFAULT }), end());
    let off = song_render(&project(MidiRow { arp: Arp { on: false, ..arp() }, ..MidiRow::DEFAULT }), end());
    let plain = song_render(&project(MidiRow::DEFAULT), end());
    let arped = song_render(&project(MidiRow { arp: arp(), ..MidiRow::DEFAULT }), end());
    let peak = loudest(&bypassed, 0);
    assert!(peak > AUDIBLE, "wanted the bypassed row's notes heard, got a peak of {peak}");
    assert_eq!(channels_apart(&bypassed, &off), [None, None], "wanted a bypassed arp to sound bit for bit as the same arp turned off, got the left and right channels apart from these frames");
    assert_eq!(channels_apart(&bypassed, &plain), [None, None], "wanted a bypassed arp to sound bit for bit as a plain MIDI row, got the left and right channels apart from these frames");
    assert_ne!(channels_apart(&arped, &bypassed), [None, None], "wanted the arp heard on the row left unbypassed, got it bit for bit as the bypassed row");
}

#[test]
fn bypassing_the_row_under_a_held_chord_plays_on_as_turning_its_arp_off_there() {
    let bypassed = switched(|row| row.bypass = true);
    let off = switched(|row| row.arp.on = false);
    let arped = song_render(&project(MidiRow { arp: arp(), ..MidiRow::DEFAULT }), end());
    let peak = loudest(&bypassed, switch());
    assert!(peak > AUDIBLE, "wanted the held chord heard after the row was bypassed at frame {}, got a peak of {peak}", switch());
    assert_eq!(channels_apart(&bypassed, &off), [None, None], "wanted bypassing the row at frame {} to sound bit for bit as turning its arp off there, got the left and right channels apart from these frames", switch());
    let apart = channels_apart(&bypassed, &arped);
    assert!(apart.iter().all(|frame| frame.is_some_and(|frame| frame >= switch())), "wanted the bypassed render to match the arp up to frame {} and part from it after, got the channels apart from {apart:?}", switch());
}
