use bp::arp::{Arp, Articulation};
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer, Tail};
use bp::core::Params as Patch;
use bp::core::params::Envelope;
use bp::division::Division;
use bp::fx::params::{Repeat, RepeatMix};
use bp::fx::{Kind, Params, Slot};
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{AUDIBLE, BLOCK, FRAMES_PER_BEAT, SAMPLE_RATE, bounce, core_layer, peak_of, struck, worst_delta};

const BEATS: i32 = 16;
const STEPS_PER_BEAT: i32 = 4;
const STEP_BEATS: f32 = 0.25;
const NOTE_BEATS: f32 = 0.2;
const LOWEST: u8 = 48;
const SPREAD: i32 = 17;
const CHANCE: f64 = 0.5;
const LATE_START: f64 = 8.0;
const CLEAR_OF_THE_START: f64 = 0.5;
const HEADER: usize = 68;
const SCALE: f32 = 8_388_608.0;
const EXPORTED_WITHIN: f32 = 1e-6;
const ALIKE: f32 = 1e-4;

fn pluck() -> Patch {
    let mut patch = Patch::DEFAULT;
    patch.phase_random = 0.0;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 40.0, sustain: 0.6, release_ms: 1.0, velocity: 0.0, ..Envelope::DEFAULT };
    patch
}

fn project() -> Project {
    let repeat = Repeat { interval: Division::Quarter, gate: 2, chance: CHANCE, mix: RepeatMix::Insert, ..Repeat::DEFAULT };
    let notes = (0..BEATS * STEPS_PER_BEAT).map(|step| Note { pitch: LOWEST + (step * 7 % SPREAD) as u8, start: step as f32 * STEP_BEATS, length: NOTE_BEATS, velocity: VELOCITY, articulation: Articulation::PLAIN }).collect();
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![struck(Arp::DEFAULT), Row::Core(core_layer(1.0, pluck())), Row::Effect(Slot { params: Params::Repeat(repeat), ..Slot::new(Kind::Repeat) })];
    project.tracks[0].clips.push(Clip { start: 0, length: BEATS * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn open(project: &Project) -> (Audio, Renderer) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(project, None, &[], &[]);
    (audio, renderer)
}

fn frame(beat: f64) -> usize {
    (beat * FRAMES_PER_BEAT).round() as usize
}

fn pass(audio: &mut Audio, renderer: &mut Renderer, from: f64) -> Vec<f32> {
    audio.play(from);
    let total = frame(f64::from(BEATS) - from);
    let mut out = Vec::with_capacity(total);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    while out.len() < total {
        let n = BLOCK.min(total - out.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    out
}

fn exported(project: &Project) -> Vec<f32> {
    let wav = bounce(project, &[], (0, BEATS * TICKS_PER_BEAT), Tail::Cut).expect("wanted the song exported");
    wav[HEADER..].as_chunks::<3>().0.iter().step_by(2).map(|b| (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / SCALE).collect()
}

fn plain() -> Vec<f32> {
    let mut dry = project();
    dry.tracks[0].rack.pop();
    let (mut audio, mut renderer) = open(&dry);
    pass(&mut audio, &mut renderer, 0.0)
}

#[test]
fn a_chance_repeat_stutters_the_same_on_every_pass() {
    let project = project();
    let (mut audio, mut renderer) = open(&project);
    let first = pass(&mut audio, &mut renderer, 0.0);
    let second = pass(&mut audio, &mut renderer, 0.0);
    assert!(peak_of(&first) > AUDIBLE, "wanted the repeated line heard, got a peak of {}", peak_of(&first));
    let untouched = worst_delta(&first, &plain());
    assert!(untouched > AUDIBLE, "wanted CHANCE {CHANCE} to repeat some beats, got the line as it plays without the row, worst delta {untouched}");
    let apart = worst_delta(&first, &second) / peak_of(&first);
    assert!(apart < ALIKE, "wanted the second pass to repeat the beats the first did, every sample within {ALIKE} of the peak, got one {apart:.4} of the peak off");
}

#[test]
fn an_export_repeats_the_beats_a_second_pass_did() {
    let project = project();
    let (mut audio, mut renderer) = open(&project);
    pass(&mut audio, &mut renderer, 0.0);
    let second = pass(&mut audio, &mut renderer, 0.0);
    let exported = exported(&project);
    assert_eq!(exported.len(), second.len(), "wanted the export as long as the pass");
    let apart = worst_delta(&exported, &second);
    assert!(apart < EXPORTED_WITHIN, "wanted the export to carry the second pass sample for sample within {EXPORTED_WITHIN}, got a worst delta of {apart}");
}

#[test]
fn a_pass_begun_mid_song_repeats_the_beats_the_whole_song_does() {
    let project = project();
    let (mut audio, mut renderer) = open(&project);
    let whole = pass(&mut audio, &mut renderer, 0.0);
    let (mut audio, mut renderer) = open(&project);
    let late = pass(&mut audio, &mut renderer, LATE_START);
    let (from, clear) = (frame(LATE_START), frame(CLEAR_OF_THE_START));
    let apart = worst_delta(&whole[from + clear..], &late[clear..]) / peak_of(&whole);
    assert!(apart < ALIKE, "wanted a pass begun at beat {LATE_START} to repeat the beats the whole song repeats from there, every sample within {ALIKE} of the peak once {CLEAR_OF_THE_START} beats clear of the start, got one {apart:.4} of the peak off");
}
