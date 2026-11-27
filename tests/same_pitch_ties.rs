use bp::arp::{Arp, Articulation, Join, Lane, Pattern};
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::core::Params;
use bp::core::params::{Envelope, Modulation};
use bp::division::Division;
use bp::project::{DEFAULT_BPM, Project, Row, SongNote, VELOCITY};
use support::{AUDIBLE, BLOCK, SAMPLE_RATE, SILENT, arp_print, core_layer, midi, peak_of};

const KEY: u8 = 69;
const STEP_MS: f64 = 250.0;
const STEP: usize = 12_000;
const MARGIN: usize = 2_400;
const FULL: i8 = 100;
const NONE: i8 = 0;
const MONO: usize = 1;
const POLY: usize = 16;
const TWO_BEATS: f64 = 2.0;

fn patch(voices: usize) -> Params {
    let mut patch = Params::DEFAULT;
    patch.voices = voices;
    patch.osc1.level = 0.0;
    patch.modulation = Modulation { level1: 1.0, ..Modulation::DEFAULT };
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    patch
}

fn tied_steps(mods: [i8; 2]) -> Pattern {
    let mut pattern = Pattern::DEFAULT;
    pattern.set_length(mods.len());
    for (step, value) in mods.into_iter().enumerate() {
        pattern.set(Lane::Mod, step, value);
        pattern.set(Lane::Gate, step, FULL);
    }
    pattern.set_join(1, Join::Tie);
    pattern
}

fn timed(pattern: Pattern) -> Arp {
    Arp { on: true, stepped: true, sync: false, time_ms: STEP_MS, pattern, ..Arp::DEFAULT }
}

fn quarters(pattern: Pattern) -> Arp {
    Arp { on: true, stepped: true, rate: Division::Quarter, pattern, ..Arp::DEFAULT }
}

fn project(voices: usize) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(timed(tied_steps([NONE, FULL]))), Row::Core(core_layer(1.0, patch(voices)))];
    project
}

fn render(renderer: &mut Renderer, left: &mut Vec<f32>, end: usize) {
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    while left.len() < end {
        let n = BLOCK.min(end - left.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        left.extend_from_slice(&l[..n]);
    }
}

pub(crate) fn step_peaks(voices: usize) -> (f32, f32) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(&project(voices), None, &[], &[]);
    audio.note_on(0, KEY, VELOCITY);
    let mut left = Vec::new();
    render(&mut renderer, &mut left, 2 * STEP);
    (peak_of(&left[MARGIN..STEP - MARGIN]), peak_of(&left[STEP + MARGIN..2 * STEP - MARGIN]))
}

fn delivered(voices: usize) {
    let (first, second) = step_peaks(voices);
    assert!(first < SILENT && second > AUDIBLE, "wanted the first step silent at MOD 0 and the step tied into it on the same pitch audible at MOD +{FULL} between frames {} and {} on {voices} voices, got peaks {first} and {second}", STEP + MARGIN, 2 * STEP - MARGIN);
}

#[test]
fn a_tied_step_on_the_same_pitch_delivers_its_mod_to_a_polyphonic_core() {
    delivered(POLY);
}

#[test]
fn a_tied_step_on_the_same_pitch_delivers_its_mod_to_a_mono_core() {
    delivered(MONO);
}

fn held() -> Vec<SongNote> {
    vec![SongNote { start: 0.0, end: TWO_BEATS, pitch: KEY, velocity: VELOCITY, articulation: Articulation::PLAIN }]
}

fn flattened(mods: [i8; 2]) -> Vec<(f64, f64, Join, Option<f32>)> {
    arp_print(&held(), quarters(tied_steps(mods)), DEFAULT_BPM).iter().map(|note| (note.start, note.end, note.articulation.join, note.articulation.modulation)).collect()
}

#[test]
fn a_same_pitch_tie_that_changes_its_mod_flattens_to_a_second_note_joined_by_a_tie() {
    let notes = flattened([NONE, FULL]);
    assert_eq!(notes, vec![(0.0, 1.0, Join::Play, Some(0.0)), (1.0, 2.0, Join::Tie, Some(1.0))], "wanted the tied step's new MOD kept as a second note joined by a tie, got {notes:?}");
}

#[test]
fn a_same_pitch_tie_that_keeps_its_mod_flattens_as_the_row_sends_it_a_second_note_joined_by_a_tie() {
    let notes = flattened([FULL, FULL]);
    assert_eq!(notes, vec![(0.0, 1.0, Join::Play, Some(1.0)), (1.0, 2.0, Join::Tie, Some(1.0))], "wanted the tie that changes nothing printed as the row sends it, a second note joined by a tie, got {notes:?}");
}
