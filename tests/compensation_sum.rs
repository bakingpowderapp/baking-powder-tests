use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::fx::params::{Freak, FreakSlope};
use bp::fx::{self, Kind, Slot};
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, Timing, Track, VELOCITY};
use bp::wav::Wave;
use std::sync::Arc;
use support::{BLOCK, LEVEL, NO_END, Onsets, SAMPLE_RATE, WITHIN, beat_frame, core_layer, midi, wav};

const PATH: &str = "/x/click.wav";
const CLICK: i16 = 8_192;
const QUARTER: f32 = 0.25;
const CLICK_BEAT: usize = 1;
const FILE_BEATS: i32 = 2;
const FRAMES: usize = 36_000;
const LIMITER: usize = 71;
const LINEAR: usize = 431;
const EXACT: f32 = 1e-6;
const CLOSE: f32 = 1e-4;
const SHOWN: usize = 8;
const PITCHES: [u8; 3] = [48, 60, 72];

fn click_wave() -> Arc<Wave> {
    let mut samples = vec![0; FILE_BEATS as usize * beat_frame(1)];
    samples[beat_frame(CLICK_BEAT)] = CLICK;
    Arc::new(Wave::parse(String::from(PATH), String::from(PATH), &wav(&samples)).expect("wanted the click file to read"))
}

fn clicking(rack: Vec<Row>) -> Track {
    Track { rack, clips: vec![Clip::audio(0, FILE_BEATS * TICKS_PER_BEAT, String::from("click"), String::from(PATH))], ..Track::audio() }
}

fn limiter() -> Row {
    let mut slot = Slot::new(Kind::Utility);
    if let fx::Params::Utility(p) = &mut slot.params {
        p.limit = true;
        p.hard = false;
        p.ceiling_db = 0.0;
        p.lookahead_ms = 1.5;
    }
    Row::Effect(slot)
}

fn linear_crossover() -> Row {
    Row::Effect(Slot { params: fx::Params::Freak(Freak { slope: FreakSlope::Linear, oversample: false, ..Freak::DEFAULT }), ..Slot::new(Kind::Freak) })
}

fn snappy() -> Params {
    let mut patch = Params::DEFAULT;
    patch.amp.attack_ms = 1.0;
    patch.amp.release_ms = 1.0;
    patch
}

fn note_at(beat: f32, pitch: u8) -> Clip {
    Clip::midi(0, String::from("note"), vec![Note { pitch, start: beat, length: 1.0, velocity: VELOCITY, articulation: Articulation::PLAIN }], NO_END)
}

fn audio_project(tracks: Vec<Track>) -> Project {
    let mut project = Project::new();
    project.tracks.clear();
    for (at, track) in tracks.into_iter().enumerate() {
        project.add_track(at, track);
    }
    project
}

fn all_seventy_one() -> Project {
    let mut project = Project::new();
    project.add_midi_track(2);
    for (track, pitch) in project.tracks.iter_mut().zip(PITCHES) {
        track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(LEVEL, snappy())), limiter()];
        track.clips.push(note_at(CLICK_BEAT as f32, pitch));
    }
    project.metronome = true;
    project
}

fn render(project: &Project, waves: &[Arc<Wave>]) -> (Vec<f32>, usize) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, None, &[], waves);
    let output = audio.timing().output;
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(FRAMES + BLOCK);
    while out.len() < FRAMES {
        renderer.render(&mut l, &mut r);
        out.extend_from_slice(&l);
    }
    out.truncate(FRAMES);
    (out, output)
}

fn heard(out: &[f32], floor: f32) -> Vec<(usize, f32)> {
    out.iter().enumerate().filter(|(_, sample)| sample.abs() > floor).map(|(frame, sample)| (frame, *sample)).collect()
}

fn one_click(out: &[f32], frame: usize, height: f32, within: f32, what: &str) {
    let heard = heard(out, within);
    let alone = heard.len() == 1 && heard[0].0 == frame && (heard[0].1 - height).abs() <= within;
    assert!(alone, "{what}: wanted one click of {height} at frame {frame} and nothing else above {within}, got {} frames above it, the first {:?}", heard.len(), &heard[..heard.len().min(SHOWN)]);
}

fn nothing_at(out: &[f32], frames: [usize; 2], within: f32, what: &str) {
    let got = [out[frames[0]], out[frames[1]]];
    assert!(got.iter().all(|sample| sample.abs() <= within), "{what}: wanted nothing at frames {frames:?}, got {got:?}");
}

fn onsets(out: &[f32]) -> Vec<usize> {
    let mut onsets = Onsets::default();
    onsets.feed(out);
    onsets.at
}

fn on_grid(at: &[usize], wanted: &[usize]) -> bool {
    at.len() == wanted.len() && at.iter().zip(wanted).all(|(got, want)| got.abs_diff(*want) <= WITHIN)
}

#[test]
fn a_dry_click_and_one_through_a_limiter_land_as_one() {
    let n = beat_frame(CLICK_BEAT);
    let (out, output) = render(&audio_project(vec![clicking(Vec::new()), clicking(vec![limiter()])]), &[click_wave()]);
    assert_eq!(output, LIMITER, "wanted the output latency at {LIMITER}, got {output}");
    one_click(&out, n + LIMITER, 2.0 * QUARTER, EXACT, "a dry track beside a limited one");
    nothing_at(&out, [n, n + 2 * LIMITER], EXACT, "a dry track beside a limited one");
}

#[test]
fn a_dry_click_and_one_through_a_linear_crossover_land_as_one() {
    let n = beat_frame(CLICK_BEAT);
    let (out, output) = render(&audio_project(vec![clicking(Vec::new()), clicking(vec![linear_crossover()])]), &[click_wave()]);
    assert_eq!(output, LINEAR, "wanted the output latency at {LINEAR}, got {output}");
    one_click(&out, n + LINEAR, 2.0 * QUARTER, CLOSE, "a dry track beside a linear crossover");
    nothing_at(&out, [n, n + 2 * LINEAR], CLOSE, "a dry track beside a linear crossover");
}

#[test]
fn two_latent_tracks_and_a_dry_one_land_as_one() {
    let n = beat_frame(CLICK_BEAT);
    let (out, output) = render(&audio_project(vec![clicking(Vec::new()), clicking(vec![limiter()]), clicking(vec![linear_crossover()])]), &[click_wave()]);
    assert_eq!(output, LINEAR, "wanted the output latency at {LINEAR}, got {output}");
    one_click(&out, n + LINEAR, 3.0 * QUARTER, CLOSE, "a dry track, a limited one and a linear crossover");
    nothing_at(&out, [n, n + LIMITER], CLOSE, "a dry track, a limited one and a linear crossover");
}

#[test]
fn the_all_seventy_one_project_plays_its_notes_and_its_click_together() {
    let project = all_seventy_one();
    let timing = Timing::of(&project, SAMPLE_RATE);
    let lines: Vec<(usize, usize)> = timing.tracks.iter().map(|track| (track.latency, track.to_master)).collect();
    assert!(timing.output == LIMITER && lines.iter().all(|&(latency, to_master)| latency == LIMITER && to_master == 0), "wanted every track at {LIMITER} with no line to the master and the output at {LIMITER}, got {lines:?} and {}", timing.output);
    let n = beat_frame(CLICK_BEAT);
    let mut notes_only = project.clone();
    notes_only.metronome = false;
    let (out, output) = render(&notes_only, &[]);
    assert_eq!(output, LIMITER, "wanted the output latency at {LIMITER}, got {output}");
    let at = onsets(&out);
    assert!(on_grid(&at, &[n + LIMITER]), "wanted the three notes as one onset at frame {}, got {at:?}", n + LIMITER);
    let mut click_only = project.clone();
    for track in &mut click_only.tracks {
        track.clips.clear();
    }
    let beats: Vec<usize> = (0..=CLICK_BEAT).map(|beat| beat_frame(beat) + LIMITER).collect();
    let at = onsets(&render(&click_only, &[]).0);
    assert!(on_grid(&at, &beats), "wanted the metronome struck {LIMITER} late at frames {beats:?}, got {at:?}");
    let at = onsets(&render(&project, &[]).0);
    assert!(on_grid(&at, &beats), "wanted the notes and the metronome as one onset per beat at frames {beats:?}, got {at:?}");
}
