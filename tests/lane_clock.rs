use bp::arp::{Arp, Articulation};
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::core::Params;
use bp::core::params::Envelope;
use bp::division::Division;
use bp::project::{Aim, Clip, DEFAULT_BPM, LaneLfo, Note, Project, Row, TICKS_PER_BEAT, Track, VELOCITY};
use std::f64::consts::TAU;
use support::{AUDIBLE, BLOCK, CORE_ROW, FRAMES_PER_BEAT, Onsets, SAMPLE_RATE, WINDOW, core_layer, midi, peak_of};

const TOLERANCE: f32 = 0.03;
const KEY: u8 = 69;
const SETTLE: usize = WINDOW * 4;
const STEP_SECONDS: f64 = 0.125;
const GRID: usize = 64;
const BETWEEN_LINES: usize = 24_060;
const NEAR: f64 = 0.01;

fn lfo(rate_hz: f64, phase: f64, retrigger: bool) -> LaneLfo {
    LaneLfo { amount: 1.0, sync: false, rate_hz, phase, retrigger, ..LaneLfo::blank(Aim::knob(CORE_ROW, "OSC 1", "LEVEL")) }
}

fn patch() -> Params {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.osc1.level = 0.5;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    patch
}

fn project(lfo: LaneLfo, arp: Arp, notes: &[(f32, f32)]) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(arp), Row::Core(core_layer(1.0, patch()))];
    track.automation.lfos.push(lfo);
    if !notes.is_empty() {
        let notes = notes.iter().map(|&(start, length)| Note { pitch: KEY, start, length, velocity: VELOCITY, articulation: Articulation::PLAIN }).collect();
        track.clips.push(Clip { start: 0, length: 8 * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    }
    project
}

struct Session {
    audio: Audio,
    renderer: Renderer,
    out: Vec<f32>,
}

impl Session {
    fn open(project: &Project) -> Session {
        let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
        renderer.patient();
        renderer.start_workers(0, || None);
        audio.sync(project, None, &[], &[]);
        Session { audio, renderer, out: Vec::new() }
    }

    fn render(&mut self, seconds: f64) {
        let end = self.out.len() + (seconds * SAMPLE_RATE).round() as usize;
        let (mut left, mut right) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
        while self.out.len() < end {
            let n = BLOCK.min(end - self.out.len());
            self.renderer.render(&mut left[..n], &mut right[..n]);
            self.out.extend_from_slice(&left[..n]);
        }
    }

    fn frame(&self) -> usize {
        self.out.len()
    }
}

fn envelope(samples: &[f32]) -> Vec<f32> {
    let peaks: Vec<f32> = samples.as_chunks::<WINDOW>().0.iter().map(|window| window.iter().fold(0.0, |peak, s| s.abs().max(peak))).collect();
    let loudest = peaks.iter().copied().fold(0.0, f32::max);
    assert!(loudest > AUDIBLE, "wanted the note to sound, got a peak of {loudest}");
    peaks.iter().map(|peak| peak / loudest).collect()
}

fn expect_sine(envelope: &[f32], from: usize, to: usize, shape: impl Fn(f64) -> f64) {
    for (index, got) in envelope.iter().enumerate().take(to / WINDOW).skip(from / WINDOW) {
        let frame = index * WINDOW + WINDOW / 2;
        let wanted = shape(frame as f64 / SAMPLE_RATE) as f32;
        assert!((got - wanted).abs() <= TOLERANCE, "wanted level {wanted:.3} at frame {frame}, got {got:.3}");
    }
}

fn from_trough(t: f64) -> f64 {
    0.5 + 0.5 * (TAU * (t + 0.75)).sin()
}

fn from_centre(t: f64) -> f64 {
    0.5 + 0.5 * (TAU * t).sin()
}

#[test]
fn a_key_restarts_a_retriggered_lfo_with_the_transport_stopped() {
    let mut session = Session::open(&project(lfo(1.0, 0.75, true), Arp::DEFAULT, &[]));
    session.audio.note_on(0, KEY, VELOCITY);
    session.render(0.64);
    let again = session.frame();
    session.audio.note_on(0, KEY, VELOCITY);
    session.render(1.2);
    let envelope = envelope(&session.out);
    expect_sine(&envelope, SETTLE, again, from_trough);
    let pressed = again as f64 / SAMPLE_RATE;
    expect_sine(&envelope, again + SETTLE, session.frame(), |t| from_trough(t - pressed));
}

#[test]
fn a_synced_lfo_restarts_on_a_key_and_counts_its_division_while_stopped() {
    let quarter = LaneLfo { amount: 1.0, sync: true, division: Division::Quarter, phase: 0.75, retrigger: true, ..LaneLfo::blank(Aim::knob(CORE_ROW, "OSC 1", "LEVEL")) };
    let mut session = Session::open(&project(quarter, Arp::DEFAULT, &[]));
    session.render(0.3);
    let pressed = session.frame();
    session.audio.note_on(0, KEY, VELOCITY);
    session.render(1.0);
    let envelope = envelope(&session.out);
    let key = pressed as f64 / SAMPLE_RATE;
    expect_sine(&envelope, pressed + SETTLE, session.frame(), |t| 0.5 + 0.5 * (TAU * (2.0 * (t - key) + 0.75)).sin());
}

#[test]
fn a_clip_note_between_grid_lines_restarts_the_lfo_from_the_grid_line_holding_it() {
    let start = BETWEEN_LINES as f32 / FRAMES_PER_BEAT as f32;
    let mut session = Session::open(&project(lfo(1.0, 0.75, true), Arp::DEFAULT, &[(0.0, 4.0), (start, 3.0)]));
    session.audio.play(0.0);
    session.render(1.9);
    let loudest = peak_of(&session.out);
    let line = BETWEEN_LINES / GRID * GRID;
    let before = peak_of(&session.out[line - GRID..line]);
    assert!(before > 0.9 * loudest, "wanted the held note still at full level through the lane step before the grid line at frame {line}, got {before:.4} against {loudest:.4}");
    let restarted = peak_of(&session.out[line..BETWEEN_LINES]);
    assert!(restarted < 0.1 * loudest, "wanted the lfo restarted at its trough from the grid line at frame {line}, ahead of the onset at frame {BETWEEN_LINES}, got {restarted:.4} against {loudest:.4}");
    let envelope = envelope(&session.out);
    let onset = line as f64 / SAMPLE_RATE;
    expect_sine(&envelope, line + SETTLE, session.frame(), |t| from_trough(t - onset));
}

#[test]
fn editing_another_track_leaves_a_held_keys_lfo_alone() {
    let mut project = project(lfo(1.0, 0.75, true), Arp::DEFAULT, &[]);
    project.tracks.push(Track::blank());
    let mut session = Session::open(&project);
    session.audio.play(0.0);
    session.render(0.32);
    let pressed = session.frame();
    session.audio.note_on(0, KEY, VELOCITY);
    session.render(0.3);
    project.tracks[1].clips.push(Clip { start: 0, length: 8 * TICKS_PER_BEAT, label: String::new(), notes: vec![Note { pitch: 48, start: 6.0, length: 1.0, velocity: VELOCITY, articulation: Articulation::PLAIN }], file: None });
    session.audio.sync(&project, None, &[], &[]);
    session.render(0.9);
    let envelope = envelope(&session.out);
    let key = pressed as f64 / SAMPLE_RATE;
    expect_sine(&envelope, pressed + SETTLE, session.frame(), |t| from_trough(t - key));
}

#[test]
fn a_tempo_change_while_stopped_leaves_a_hertz_lfos_phase_alone() {
    let mut project = project(lfo(1.0, 0.0, false), Arp::DEFAULT, &[]);
    let mut session = Session::open(&project);
    session.audio.note_on(0, KEY, VELOCITY);
    session.render(0.25);
    project.bpm = 60.0;
    session.audio.sync(&project, None, &[], &[]);
    session.render(1.0);
    let envelope = envelope(&session.out);
    expect_sine(&envelope, SETTLE, session.frame(), from_centre);
}

#[test]
fn play_realigns_a_retriggered_lfo_to_the_song() {
    let mut session = Session::open(&project(lfo(1.0, 0.75, true), Arp::DEFAULT, &[(0.0, 2.0)]));
    session.audio.note_on(0, KEY, VELOCITY);
    session.render(0.37);
    session.audio.note_off(0, KEY);
    session.render(0.1);
    let played = session.frame();
    session.audio.play(0.0);
    session.render(1.0);
    let envelope = envelope(&session.out[played..]);
    expect_sine(&envelope, SETTLE, session.frame() - played, from_trough);
}

#[test]
fn arp_steps_restart_a_retriggered_lfo() {
    let arp = Arp { on: true, sync: false, time_ms: STEP_SECONDS * 1000.0, gate: 0.5, ..Arp::DEFAULT };
    let mut session = Session::open(&project(lfo(0.25, 0.25, true), arp, &[]));
    session.audio.note_on(0, KEY, VELOCITY);
    session.render(1.0);
    let mut onsets = Onsets::default();
    onsets.feed(&session.out);
    assert!(onsets.at.len() >= 6, "wanted at least six arp steps in the first second, got onsets at {:?}", onsets.at);
    let step = (STEP_SECONDS * SAMPLE_RATE) as usize;
    let peaks: Vec<f32> = session.out.chunks_exact(step).map(|chunk| chunk.iter().fold(0.0, |peak, s| s.abs().max(peak))).collect();
    for (index, peak) in peaks.iter().enumerate().skip(1) {
        assert!((peak - peaks[0]).abs() <= TOLERANCE * peaks[0], "wanted step {index} as loud as step 0, the LFO starting over from its peak on each step the arp plays, got {peak:.4} against {:.4}", peaks[0]);
    }
}

#[test]
fn the_faces_can_read_the_lane_clock() {
    let mut session = Session::open(&project(lfo(1.0, 0.75, true), Arp::DEFAULT, &[]));
    session.render(0.5);
    session.render(0.25);
    let beats = |seconds: f64| seconds * DEFAULT_BPM / 60.0;
    let lane = session.audio.lane();
    assert!((lane.beats - beats(0.75)).abs() < NEAR && (lane.seconds - 0.75).abs() < NEAR, "wanted the lane clock near {:.3} beats and 0.75 s after three quarters of a second stopped, got {:.3} beats and {:.3} s", beats(0.75), lane.beats, lane.seconds);
}

#[test]
fn the_stop_button_returns_the_playhead_without_a_jump() {
    let mut session = Session::open(&project(lfo(1.0, 0.0, false), Arp::DEFAULT, &[]));
    session.audio.note_on(0, KEY, VELOCITY);
    session.audio.play(0.0);
    session.render(0.32);
    session.audio.stop();
    session.audio.place(1.0);
    session.render(1.0);
    let envelope = envelope(&session.out);
    expect_sine(&envelope, SETTLE, session.frame(), from_centre);
}

#[test]
fn a_tempo_change_while_stopped_away_from_the_top_leaves_a_hertz_lfo_alone() {
    let mut project = project(lfo(1.0, 0.0, false), Arp::DEFAULT, &[]);
    let mut session = Session::open(&project);
    session.audio.note_on(0, KEY, VELOCITY);
    session.audio.play(0.0);
    session.render(0.3);
    session.audio.stop();
    session.render(0.2);
    project.bpm = 60.0;
    session.audio.sync(&project, None, &[], &[]);
    session.render(1.0);
    let envelope = envelope(&session.out);
    expect_sine(&envelope, SETTLE, session.frame(), from_centre);
}

#[test]
fn a_synced_lfo_changes_speed_and_keeps_its_phase_when_the_tempo_changes_while_stopped() {
    let quarter = LaneLfo { amount: 1.0, sync: true, division: Division::Quarter, phase: 0.75, retrigger: true, ..LaneLfo::blank(Aim::knob(CORE_ROW, "OSC 1", "LEVEL")) };
    let mut project = project(quarter, Arp::DEFAULT, &[]);
    let mut session = Session::open(&project);
    session.audio.note_on(0, KEY, VELOCITY);
    session.render(0.25);
    let changed = session.frame() as f64 / SAMPLE_RATE;
    project.bpm = 60.0;
    session.audio.sync(&project, None, &[], &[]);
    session.render(1.0);
    let envelope = envelope(&session.out);
    let cycles = |t: f64| if t < changed { 2.0 * t } else { 2.0 * changed + (t - changed) };
    expect_sine(&envelope, SETTLE, session.frame(), |t| 0.5 + 0.5 * (TAU * (cycles(t) + 0.75)).sin());
}
