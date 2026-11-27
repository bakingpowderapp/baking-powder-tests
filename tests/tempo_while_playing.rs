use bp::arp::{Arp, Articulation};
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::core::Params as CoreParams;
use bp::division::Division;
use bp::fx::params::{Delay, DelayMode};
use bp::fx::{Kind, Params, Slot};
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{BLOCK, FRAMES_PER_BEAT, Onsets, SAMPLE_RATE, SILENT, WITHIN, beat_frame, core_layer, midi};

const SLOWER_BPM: f64 = 90.0;
const SLOWER_FRAMES_PER_BEAT: f64 = SAMPLE_RATE * 60.0 / SLOWER_BPM;
const CHANGE_BEAT: usize = 4;
const LOOP_BEATS: usize = 8;
const PLAYED_BEATS: usize = 24;
const KEY: u8 = 60;
const SHORT: f32 = 0.25;
const HELD: (f32, f32) = (3.0, 2.0);
const RELEASE_MS: f64 = 1.0;
const ECHO_WET: f64 = 0.5;

struct Session {
    audio: Audio,
    renderer: Renderer,
    out: Vec<f32>,
}

impl Session {
    fn playing(project: &Project, range: Option<(f64, f64)>) -> Session {
        let (mut audio, renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
        audio.sync(project, None, &[], &[]);
        audio.set_loop(range);
        audio.play(0.0);
        Session { audio, renderer, out: Vec::new() }
    }

    fn render(&mut self, frames: usize) {
        let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
        let mut done = 0;
        while done < frames {
            let n = BLOCK.min(frames - done);
            self.renderer.render(&mut l[..n], &mut r[..n]);
            self.out.extend_from_slice(&l[..n]);
            done += n;
        }
    }

    fn sync(&mut self, project: &Project) {
        self.audio.sync(project, None, &[], &[]);
    }
}

fn song(notes: &[(f32, f32)], effects: Vec<Row>) -> Project {
    let mut patch = CoreParams::DEFAULT;
    patch.amp.attack_ms = 1.0;
    patch.amp.release_ms = RELEASE_MS;
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch))];
    track.rack.extend(effects);
    let notes = notes.iter().map(|&(start, length)| Note { pitch: KEY, start, length, velocity: VELOCITY, articulation: Articulation::PLAIN }).collect();
    track.clips = vec![Clip { start: 0, length: LOOP_BEATS as i32 * TICKS_PER_BEAT, label: String::new(), notes, file: None }];
    project
}

fn every(step: usize) -> Vec<(f32, f32)> {
    (0..LOOP_BEATS).step_by(step).map(|beat| (beat as f32, SHORT)).collect()
}

fn quarter_echo() -> Row {
    let params = Delay { mode: DelayMode::Clean, link: true, sync_l: true, division_l: Division::Quarter, feedback: 0.0, ..Delay::DEFAULT };
    Row::Effect(Slot { wet: ECHO_WET, params: Params::Delay(params), ..Slot::new(Kind::Delay) })
}

fn heard_at(beat: usize) -> usize {
    match beat.checked_sub(CHANGE_BEAT) {
        None => beat_frame(beat),
        Some(after) => beat_frame(CHANGE_BEAT) + (after as f64 * SLOWER_FRAMES_PER_BEAT).round() as usize,
    }
}

fn quarter(beat: usize) -> usize {
    (if beat < CHANGE_BEAT { FRAMES_PER_BEAT } else { SLOWER_FRAMES_PER_BEAT }).round() as usize
}

fn slowed(project: &Project, frames: usize) -> Vec<f32> {
    let change = beat_frame(CHANGE_BEAT);
    let mut session = Session::playing(project, Some((0.0, LOOP_BEATS as f64)));
    session.render(change);
    let mut slower = project.clone();
    slower.bpm = SLOWER_BPM;
    session.sync(&slower);
    session.render(frames - change);
    session.out
}

fn check(out: &[f32], wanted: &[usize], what: &str) {
    let mut onsets = Onsets::default();
    onsets.feed(out);
    let near = |at: usize, frame: usize| at.abs_diff(frame) <= WITHIN;
    let missed: Vec<usize> = wanted.iter().copied().filter(|&frame| !onsets.at.iter().any(|&at| near(at, frame))).collect();
    let stray: Vec<usize> = onsets.at.iter().copied().filter(|&at| !wanted.iter().any(|&frame| near(at, frame))).collect();
    assert!(missed.is_empty() && stray.is_empty(), "wanted {what}, onsets within {WITHIN} of frames {wanted:?}; missed frames {missed:?}, got stray onsets at frames {stray:?}");
}

#[test]
fn notes_fall_every_slower_beat_from_a_tempo_change_while_looping() {
    let out = slowed(&song(&every(1), Vec::new()), heard_at(PLAYED_BEATS));
    let wanted: Vec<usize> = (0..PLAYED_BEATS).map(heard_at).collect();
    check(&out, &wanted, &format!("a note on every beat of the {LOOP_BEATS} beat loop, {FRAMES_PER_BEAT} frames apart up to the change to {SLOWER_BPM} bpm at frame {} and {SLOWER_FRAMES_PER_BEAT} apart from it", beat_frame(CHANGE_BEAT)));
}

#[test]
fn a_note_held_across_a_tempo_change_ends_on_its_beat_at_the_new_tempo() {
    let (start, length) = HELD;
    let last = (start + length) as usize;
    let ended = heard_at(last);
    let out = slowed(&song(&[HELD], Vec::new()), heard_at(last + 1));
    let release = (RELEASE_MS * 0.001 * SAMPLE_RATE).round() as usize;
    let silent = out.iter().rposition(|sample| sample.abs() > SILENT).map_or(0, |at| at + 1);
    assert!((ended..=ended + release + WITHIN).contains(&silent), "wanted the note held from beat {start} across the change to {SLOWER_BPM} bpm at frame {} to end on beat {last} at frame {ended}, not the old tempo's frame {}, and fall silent within its {release} frame release, got silence from frame {silent}", beat_frame(CHANGE_BEAT), beat_frame(last));
}

#[test]
fn a_quarter_synced_delay_echoes_each_later_note_one_slower_beat_after_it() {
    let out = slowed(&song(&every(2), vec![quarter_echo()]), heard_at(PLAYED_BEATS));
    let wanted: Vec<usize> = (0..PLAYED_BEATS).step_by(2).flat_map(|beat| [heard_at(beat), heard_at(beat) + quarter(beat)]).collect();
    check(&out, &wanted, &format!("a note on every second beat and its echo a quarter note after it, {FRAMES_PER_BEAT} frames up to the change to {SLOWER_BPM} bpm at frame {} and {SLOWER_FRAMES_PER_BEAT} for every note from it", beat_frame(CHANGE_BEAT)));
}

#[test]
fn the_metronome_ticks_every_slower_beat_from_a_tempo_change_while_looping() {
    let mut project = song(&[], Vec::new());
    project.metronome = true;
    let out = slowed(&project, heard_at(PLAYED_BEATS));
    let wanted: Vec<usize> = (0..PLAYED_BEATS).map(heard_at).collect();
    check(&out, &wanted, &format!("a tick on every beat, {FRAMES_PER_BEAT} frames apart up to the change to {SLOWER_BPM} bpm at frame {} and {SLOWER_FRAMES_PER_BEAT} apart from it", beat_frame(CHANGE_BEAT)));
}
