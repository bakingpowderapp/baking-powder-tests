use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::core::params::Envelope;
use bp::json;
use bp::project::{Aim, Clip, LaneLfo, LfoShape, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{BLOCK, CORE_ROW, FRAMES_PER_BEAT, SAMPLE_RATE, WINDOW, core_layer, midi};

const SECONDS: usize = 4;
const TOLERANCE: f32 = 0.08;
const RATE_HZ: f64 = 2.0;
const CYCLE: usize = (SAMPLE_RATE / RATE_HZ) as usize;
const FIRST_NOTE: (f32, f32) = (0.0, 4.0);
const SECOND_NOTE: (f32, f32) = (5.0, 2.0);
const SECOND_NOTE_FRAME: usize = (SECOND_NOTE.0 as f64 * FRAMES_PER_BEAT) as usize;
const BAND: &str = "OSC 1";
const KNOB: &str = "LEVEL";

fn lfo(once: bool) -> LaneLfo {
    LaneLfo { amount: 1.0, shape: LfoShape::Saw, sync: false, rate_hz: RATE_HZ, retrigger: true, once, ..LaneLfo::blank(Aim::knob(CORE_ROW, BAND, KNOB)) }
}

fn project(lfo: LaneLfo) -> Project {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.osc1.level = 0.5;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch))];
    track.automation.lfos.push(lfo);
    let notes = [FIRST_NOTE, SECOND_NOTE].map(|(start, length)| Note { pitch: 69, start, length, velocity: VELOCITY, articulation: Articulation::PLAIN });
    track.clips.push(Clip { start: 0, length: 8 * TICKS_PER_BEAT, label: String::new(), notes: notes.to_vec(), file: None });
    project
}

fn envelope(project: &Project) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(project, None, &[], &[]);
    audio.play(0.0);
    let frames = SECONDS * SAMPLE_RATE as usize;
    let (mut left, mut right) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(frames);
    while out.len() < frames {
        let n = BLOCK.min(frames - out.len());
        renderer.render(&mut left[..n], &mut right[..n]);
        out.extend_from_slice(&left[..n]);
    }
    let peaks: Vec<f32> = out.as_chunks::<WINDOW>().0.iter().map(|window| window.iter().fold(0.0, |peak, s| s.abs().max(peak))).collect();
    let loudest = peaks.iter().copied().fold(0.0, f32::max);
    peaks.iter().map(|peak| peak / loudest).collect()
}

fn expect_level(envelope: &[f32], frame: usize, wanted: f32) {
    let got = envelope[frame / WINDOW];
    assert!((got - wanted).abs() <= TOLERANCE, "wanted level {wanted:.2} at frame {frame}, got {got:.3}");
}

fn reopened(project: &Project) -> Project {
    let saved = project.to_json().write();
    Project::from_json(&json::parse(&saved).expect("a saved project is valid JSON")).expect("a saved project opens")
}

#[test]
fn once_falls_silent_within_its_first_cycle_and_stays_there_through_the_held_note() {
    let once = envelope(&project(lfo(true)));
    expect_level(&once, WINDOW, 1.0);
    expect_level(&once, CYCLE - WINDOW, 0.0);
    expect_level(&once, CYCLE + CYCLE / 2, 0.0);
    expect_level(&once, 3 * CYCLE + CYCLE / 2, 0.0);
}

#[test]
fn without_once_the_lane_is_loud_again_a_quarter_second_into_its_second_cycle() {
    let plain = envelope(&project(lfo(false)));
    expect_level(&plain, CYCLE - WINDOW, 0.0);
    expect_level(&plain, CYCLE + WINDOW, 1.0);
    expect_level(&plain, CYCLE + CYCLE / 2, 0.5);
}

#[test]
fn a_later_note_restarts_the_once_lane() {
    let once = envelope(&project(lfo(true)));
    expect_level(&once, SECOND_NOTE_FRAME - WINDOW, 0.0);
    expect_level(&once, SECOND_NOTE_FRAME + WINDOW, 1.0);
    expect_level(&once, SECOND_NOTE_FRAME + CYCLE + CYCLE / 2, 0.0);
}

#[test]
fn once_survives_a_save_and_is_written_only_when_on() {
    let kept = reopened(&project(lfo(true)));
    let lane = kept.tracks[0].automation.lfo(&Aim::knob(CORE_ROW, BAND, KNOB)).expect("the lane reopens with its row");
    assert!(lane.once, "wanted the reopened lane to keep ONCE on, got it off");
    let plain = project(lfo(false)).to_json().write();
    assert!(!plain.contains("\"once\""), "wanted no once flag in a save with ONCE off, got one in {plain}");
}
