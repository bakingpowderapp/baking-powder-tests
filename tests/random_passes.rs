use bp::arp::{Arp, Articulation, Style};
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::core::Params as Patch;
use bp::core::params::Envelope;
use bp::division::Division;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BAR, VELOCITY};
use support::{AUDIBLE, BLOCK, FRAMES_PER_BEAT, SAMPLE_RATE, core_layer, peak_of, struck};

const BARS: i32 = 4;
const BEATS: f64 = 16.0;
const PAST_THE_SONG: usize = 4_037;
const LATE_START: f64 = 8.0;
const CHORD: [u8; 4] = [57, 60, 64, 69];
const COPIES: usize = 3;
const DETUNE_CENTS: f64 = 14.0;
const NOTE_BEATS: f32 = 0.9;

fn plucked(copies: usize, scatter: f64) -> Patch {
    let mut patch = Patch::DEFAULT;
    patch.osc1.unison = copies;
    patch.osc1.detune = DETUNE_CENTS;
    patch.phase_random = scatter;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 60.0, sustain: 0.5, release_ms: 1.0, velocity: 0.0, ..Envelope::DEFAULT };
    patch
}

fn song(arp: Arp, patch: Patch, clips: Vec<Clip>) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![struck(arp), Row::Core(core_layer(0.5, patch))];
    project.tracks[0].clips = clips;
    project
}

fn shuffled_chords() -> Project {
    let arp = Arp { on: true, style: Style::RandomOnce, rate: Division::Sixteenth, ..Arp::DEFAULT };
    song(arp, plucked(1, 0.0), (0..BARS).map(|bar| Clip::chord(bar * TICKS_PER_BAR, String::new(), &CHORD)).collect())
}

fn scattered_line() -> Project {
    let notes = (0..BARS * 4).map(|beat| Note { pitch: CHORD[beat as usize % CHORD.len()], start: beat as f32, length: NOTE_BEATS, velocity: VELOCITY, articulation: Articulation::PLAIN }).collect();
    song(Arp::DEFAULT, plucked(COPIES, 1.0), vec![Clip { start: 0, length: BARS * TICKS_PER_BAR, label: String::new(), notes, file: None }])
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

fn pass(audio: &mut Audio, renderer: &mut Renderer, from: f64, total: usize) -> Vec<f32> {
    audio.play(from);
    let mut out = Vec::with_capacity(total);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    while out.len() < total {
        let n = BLOCK.min(total - out.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    out
}

fn apart(got: &[f32], wanted: &[f32]) -> Option<usize> {
    got.iter().zip(wanted).position(|(a, b)| a.to_bits() != b.to_bits())
}

fn same_on_a_second_pass(project: &Project, what: &str) {
    let (mut audio, mut renderer) = open(project);
    let first = pass(&mut audio, &mut renderer, 0.0, frame(BEATS) + PAST_THE_SONG);
    let second = pass(&mut audio, &mut renderer, 0.0, frame(BEATS));
    assert!(peak_of(&first) > AUDIBLE, "wanted {what} heard, got a peak of {}", peak_of(&first));
    let at = apart(&second, &first[..frame(BEATS)]);
    assert!(at.is_none(), "wanted {what} to render on a second pass as on the first, as an export does, got a difference at frame {at:?}");
}

#[test]
fn a_random_once_arp_shuffles_each_chord_the_same_on_every_pass() {
    same_on_a_second_pass(&shuffled_chords(), "four chords under a Random Once arp");
}

#[test]
fn a_scattered_unison_starts_each_note_the_same_on_every_pass() {
    same_on_a_second_pass(&scattered_line(), "a line on three scattered unison copies");
}

#[test]
fn a_pass_begun_mid_song_draws_what_the_whole_song_draws_there() {
    for (project, what) in [(shuffled_chords(), "the Random Once arp"), (scattered_line(), "the scattered unison")] {
        let (mut audio, mut renderer) = open(&project);
        let whole = pass(&mut audio, &mut renderer, 0.0, frame(BEATS));
        let (mut audio, mut renderer) = open(&project);
        let late = pass(&mut audio, &mut renderer, LATE_START, frame(BEATS - LATE_START));
        let at = apart(&late, &whole[frame(LATE_START)..]);
        assert!(at.is_none(), "wanted {what} begun at beat {LATE_START} to render as the whole song does from there, got a difference {at:?} frames in");
    }
}
