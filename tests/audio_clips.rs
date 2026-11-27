use bp::arp::Arp;
use bp::audio::{Audio, Tail};
use bp::audio::vision::Seer;
use bp::fx::{Kind, Slot};
use bp::json::{self, Json};
use bp::project::{Clip, File, Layer, Project, Row, TICKS_PER_BEAT, Track, joined};
use bp::wav::Wave;
use std::f64::consts::TAU;
use std::sync::Arc;
use support::{BLOCK, FRAMES_PER_BEAT, RATE, SAMPLE_RATE, SILENT, midi};

const CD_RATE: u32 = 44_100;
const FADE: usize = 64;
const HALF: i16 = 16_384;
const QUARTER: i16 = 8_192;
const THREE_QUARTERS: i16 = 24_576;
const PATH: &str = "/x/tone.wav";
const OTHER: &str = "/x/other.wav";
const TONE_HZ: f64 = 1_000.0;

fn wav(rate: u32, samples: &[i16]) -> Vec<u8> {
    let data: Vec<u8> = samples.iter().flat_map(|sample| sample.to_le_bytes()).collect();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&((36 + data.len()) as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&rate.to_le_bytes());
    bytes.extend_from_slice(&(rate * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&data);
    bytes
}

fn wave(path: &str, rate: u32, samples: &[i16]) -> Arc<Wave> {
    Arc::new(Wave::parse(String::from(path), String::from(path), &wav(rate, samples)).expect("wanted the test file to read"))
}

fn held(seconds: f64, rate: u32, level: i16) -> Vec<i16> {
    vec![level; (seconds * f64::from(rate)) as usize]
}

fn clip(start: i32, length: i32) -> Clip {
    Clip::audio(start * TICKS_PER_BEAT, length * TICKS_PER_BEAT, String::from("tone"), String::from(PATH))
}

fn project(clips: Vec<Clip>) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0] = Track::audio();
    project.tracks[0].clips = clips;
    project
}

fn render(project: &Project, waves: &[Arc<Wave>], from: f64, frames: usize) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, None, &[], waves);
    audio.play(from);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(frames);
    while out.len() < frames {
        let n = BLOCK.min(frames - out.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    out
}

fn first_sound(samples: &[f32]) -> Option<usize> {
    samples.iter().position(|sample| sample.abs() > SILENT)
}

fn last_sound(samples: &[f32]) -> Option<usize> {
    samples.iter().rposition(|sample| sample.abs() > SILENT)
}

fn beat_frame(beat: f64) -> usize {
    (beat * FRAMES_PER_BEAT).round() as usize
}

#[test]
fn a_file_clip_sounds_from_its_beat_and_fades_in_and_out_at_its_edges() {
    let waves = [wave(PATH, RATE, &held(3.0, RATE, HALF))];
    let out = render(&project(vec![clip(4, 4)]), &waves, 0.0, beat_frame(10.0));
    let (start, end) = (beat_frame(4.0), beat_frame(8.0));
    assert_eq!(first_sound(&out), Some(start), "wanted the first sound at frame {start}, got {:?}", first_sound(&out));
    assert_eq!(last_sound(&out), Some(end - 1), "wanted the last sound at frame {}, got {:?}", end - 1, last_sound(&out));
    assert!((out[start] - 0.5 / FADE as f32).abs() < SILENT, "wanted the first frame at 0.5 over {FADE}, got {}", out[start]);
    assert_eq!(out[start + FADE - 1], 0.5, "wanted the file at full level {FADE} frames in, got {}", out[start + FADE - 1]);
    assert_eq!(out[end - FADE], 0.5, "wanted the file still at full level {FADE} frames before the clip ends, got {}", out[end - FADE]);
    assert!(out[end - 1] < 0.02, "wanted the last frame faded near silence, got {}", out[end - 1]);
}

#[test]
fn the_file_keeps_its_own_speed_at_any_tempo() {
    let waves = [wave(PATH, RATE, &held(1.0, RATE, HALF))];
    let mut slow = project(vec![clip(0, 8)]);
    slow.bpm = 60.0;
    let fast = project(vec![clip(0, 8)]);
    let wanted = RATE as usize - 1;
    for (name, project) in [("60", slow), ("120", fast)] {
        let out = render(&project, &waves, 0.0, 3 * RATE as usize);
        assert_eq!(last_sound(&out), Some(wanted), "at {name} bpm wanted the one-second file to end at frame {wanted}, got {:?}", last_sound(&out));
    }
}

#[test]
fn a_forty_four_one_file_keeps_its_pitch_at_forty_eight() {
    let samples: Vec<i16> = (0..2 * CD_RATE).map(|frame| ((f64::from(frame) / f64::from(CD_RATE) * TONE_HZ * TAU).sin() * f64::from(HALF)) as i16).collect();
    let waves = [wave(PATH, CD_RATE, &samples)];
    let out = render(&project(vec![clip(0, 8)]), &waves, 0.0, RATE as usize + 4_800);
    let second = &out[4_800..];
    let crossings = second.windows(2).filter(|pair| (pair[0] < 0.0) != (pair[1] < 0.0)).count();
    let wanted = 2 * TONE_HZ as usize;
    assert!(crossings.abs_diff(wanted) <= 4, "wanted about {wanted} zero crossings in a second of a {TONE_HZ} Hz tone, got {crossings}");
}

#[test]
fn an_offset_starts_the_clip_deeper_in_the_file() {
    let mut samples = held(1.0, RATE, 0);
    samples.extend(held(1.0, RATE, HALF));
    let waves = [wave(PATH, RATE, &samples)];
    let plain = render(&project(vec![clip(0, 8)]), &waves, 0.0, 2 * RATE as usize);
    assert_eq!(first_sound(&plain), Some(RATE as usize), "wanted the plain clip silent for its first second, got sound at {:?}", first_sound(&plain));
    let mut deeper = clip(0, 8);
    deeper.file = Some(File { offset: 1.0, ..File::new(String::from(PATH)) });
    let out = render(&project(vec![deeper]), &waves, 0.0, 2 * RATE as usize);
    assert_eq!(first_sound(&out), Some(0), "wanted the clip a second into its file sounding at once, got {:?}", first_sound(&out));
    assert_eq!(out[FADE], 0.5, "wanted the held level {FADE} frames in, got {}", out[FADE]);
    assert_eq!(last_sound(&out), Some(RATE as usize - 1), "wanted the clip a second into its file to run out a second in, got {:?}", last_sound(&out));
}

#[test]
fn a_split_clip_plays_on_across_the_seam() {
    let waves = [wave(PATH, RATE, &held(2.0, RATE, HALF))];
    let mut split = project(vec![clip(0, 4)]);
    let bpm = split.bpm;
    assert!(split.tracks[0].split(0, 2 * TICKS_PER_BEAT, bpm), "wanted the clip split at beat two");
    let clips = &split.tracks[0].clips;
    assert_eq!(clips.iter().map(|clip| (clip.start, clip.length, clip.file.as_ref().map(File::entry))).collect::<Vec<_>>(), vec![(0, 2 * TICKS_PER_BEAT, Some(0.0)), (2 * TICKS_PER_BEAT, 2 * TICKS_PER_BEAT, Some(1.0))], "wanted two halves, the tail a second into its file");
    let out = render(&split, &waves, 0.0, 2 * RATE as usize);
    let whole = render(&project(vec![clip(0, 4)]), &waves, 0.0, 2 * RATE as usize);
    assert!(out == whole, "wanted the split halves to render exactly as the whole clip");
    let seam = beat_frame(2.0);
    let dip = out[FADE..2 * RATE as usize - FADE].iter().copied().fold(1.0f32, f32::min);
    assert_eq!(dip, 0.5, "wanted no dip at the seam at frame {seam}, got {dip}");
    let mut moved = split.clone();
    moved.tracks[0].clips[1].start += TICKS_PER_BEAT;
    let apart = render(&moved, &waves, 0.0, 2 * RATE as usize);
    assert!(apart[seam - 1] < 0.02 && apart[seam + FRAMES_PER_BEAT as usize / 2] == 0.0, "wanted the tail moved a beat later to fade out at the seam and leave a gap, got {} and {}", apart[seam - 1], apart[seam + FRAMES_PER_BEAT as usize / 2]);
}

#[test]
fn effects_the_fader_mute_and_a_solo_elsewhere_act_after_the_pour() {
    let waves = [wave(PATH, RATE, &held(1.0, RATE, HALF))];
    let dry = render(&project(vec![clip(0, 4)]), &waves, 0.0, RATE as usize);
    let mut delayed = project(vec![clip(0, 4)]);
    delayed.tracks[0].rack = vec![Row::Effect(Slot::new(Kind::Delay))];
    assert!(render(&delayed, &waves, 0.0, RATE as usize) != dry, "wanted a delay on the audio track's rack to change the sound");
    let mut faded = project(vec![clip(0, 4)]);
    faded.tracks[0].gain = 0.5;
    let halved: Vec<f32> = dry.iter().map(|sample| sample * 0.5).collect();
    assert!(render(&faded, &waves, 0.0, RATE as usize) == halved, "wanted the fader at half to halve every sample exactly");
    let mut muted = project(vec![clip(0, 4)]);
    muted.tracks[0].mute = true;
    assert_eq!(first_sound(&render(&muted, &waves, 0.0, RATE as usize)), None, "wanted a muted audio track silent");
    let mut soloed_elsewhere = project(vec![clip(0, 4)]);
    let mut other = Track::blank();
    other.solo = true;
    soloed_elsewhere.tracks.push(other);
    assert_eq!(first_sound(&render(&soloed_elsewhere, &waves, 0.0, RATE as usize)), None, "wanted the audio track silent while a MIDI track is soloed");
}

#[test]
fn stopping_fades_the_file_out_within_a_block_then_stays_silent() {
    let waves = [wave(PATH, RATE, &held(2.0, RATE, HALF))];
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(&project(vec![clip(0, 8)]), None, &[], &waves);
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    for _ in 0..10 {
        renderer.render(&mut l, &mut r);
    }
    assert_eq!(l[BLOCK - 1], 0.5, "wanted the file at full level before the stop, got {}", l[BLOCK - 1]);
    audio.stop();
    renderer.render(&mut l, &mut r);
    assert!(l[0] > 0.4 && l[BLOCK - 1] < 0.01 && l[BLOCK / 2] < l[0], "wanted the block after the stop to ramp from the file's level down to silence, got {} then {} then {}", l[0], l[BLOCK / 2], l[BLOCK - 1]);
    renderer.render(&mut l, &mut r);
    assert_eq!(first_sound(&l), None, "wanted the second block after the stop silent, got sound at {:?}", first_sound(&l));
}

#[test]
fn playing_from_a_beat_lands_inside_the_file() {
    let mut samples = held(1.0, RATE, QUARTER);
    samples.extend(held(1.0, RATE, THREE_QUARTERS));
    let waves = [wave(PATH, RATE, &samples)];
    let from_start = render(&project(vec![clip(0, 8)]), &waves, 0.0, 2 * FADE);
    assert_eq!(from_start[FADE], 0.25, "wanted the file's first second from beat zero, got {}", from_start[FADE]);
    let from_two = render(&project(vec![clip(0, 8)]), &waves, 2.0, 2 * FADE);
    assert_eq!(from_two[FADE], 0.75, "wanted the file's second second from beat two, got {}", from_two[FADE]);
    assert!((from_two[0] - 0.75 / FADE as f32).abs() < SILENT, "wanted the seek to fade in from silence, got {}", from_two[0]);
}

#[test]
fn the_bounce_hears_the_clip() {
    let waves = [wave(PATH, RATE, &held(1.0, RATE, HALF))];
    let project = project(vec![clip(4, 2)]);
    let bytes = support::bounce(&project, &waves, (0, project.end()), Tail::Rings).expect("wanted the project to bounce");
    let bounce = Wave::parse(String::from("bounce"), String::from("bounce.wav"), &bytes).expect("wanted the bounce to read back");
    let before = bounce.peak(0.0, beat_frame(4.0) as f64 - 2.0 * bp::wav::COARSE as f64);
    let during = bounce.peak(beat_frame(5.0) as f64, beat_frame(5.0) as f64 + 100.0);
    assert_eq!(before, 0, "wanted the bounce silent before the clip, got a peak of {before}");
    assert!((127..=129).contains(&during), "wanted the bounce at the file's half level inside the clip, got a peak of {during}");
}

#[test]
fn the_document_keeps_the_kind_the_file_and_the_offset() {
    let mut original = project(vec![clip(4, 8)]);
    let file = File { offset: 1.5, ..File::new(String::from(PATH)) };
    original.tracks[0].clips[0].file = Some(file.clone());
    original.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(Layer::DEFAULT)];
    original.tracks.push(Track::blank());
    let text = original.to_json().write();
    let reopened = Project::from_json(&json::parse(&text).expect("wanted the document to parse")).expect("wanted the document to read");
    assert!(reopened.tracks[0].audio && !reopened.tracks[1].audio, "wanted the audio track and the MIDI track to keep their kinds");
    assert_eq!(reopened.tracks[0].clips[0].file, Some(file), "wanted the clip's file and offset back, got {:?}", reopened.tracks[0].clips[0].file);
    assert!(reopened.tracks[0].rack.is_empty(), "wanted the MIDI row and the Core smuggled onto an audio track refused on reading, got {:?}", reopened.tracks[0].rack);
    let kept = &reopened.tracks[1].rack;
    assert!(kept.first().is_some_and(Row::is_midi) && kept.iter().any(Row::is_core), "wanted the MIDI track to keep its MIDI row at the top and its Core, got {kept:?}");
    let document = json::parse(&text).expect("wanted the document to parse");
    let first = &document.get("tracks").and_then(Json::as_array).expect("wanted the tracks written")[0];
    assert_eq!(first.get("kind").and_then(Json::as_str), Some("audio"), "wanted the kind spelled on the audio track");
    let offset = first.get("clips").and_then(Json::as_array).and_then(|clips| clips[0].get("wave")).and_then(|wave| wave.get("startSeconds")).and_then(Json::as_f64);
    assert_eq!(offset, Some(1.5), "wanted the start spelled in seconds on the clip's wave, got {offset:?}");
}

#[test]
fn a_join_takes_the_clips_between_and_lays_one_over_the_span_and_a_new_track_is_named_for_its_kind() {
    let mut project = project(vec![clip(0, 4), Clip::audio(4 * TICKS_PER_BEAT, 4 * TICKS_PER_BEAT, String::from("other"), String::from(OTHER))]);
    let bpm = project.bpm;
    let parts = project.tracks[0].take_between(0, 8 * TICKS_PER_BEAT, bpm);
    assert_eq!(parts.len(), 2, "wanted both clips taken from between 0 and 8, got {}", parts.len());
    assert!(project.tracks[0].clips.is_empty(), "wanted the track emptied by the take, got {} clips", project.tracks[0].clips.len());
    let file = parts[0].file.clone();
    project.tracks[0].clips.push(joined(&parts, 0, 8 * TICKS_PER_BEAT, file));
    let joined = &project.tracks[0].clips;
    assert_eq!(joined.len(), 1, "wanted one clip after the join, got {}", joined.len());
    assert_eq!((joined[0].start, joined[0].length, joined[0].file.as_ref().map(|file| file.name.as_str())), (0, 8 * TICKS_PER_BEAT, Some(PATH)), "wanted the joined clip over eight beats holding the file it was given");
    project.add_track(1, Track::audio());
    project.add_track(2, Track::blank());
    assert_eq!((project.tracks[1].name.as_str(), project.tracks[2].name.as_str()), ("AUDIO 3", "MIDI 4"), "wanted new tracks named for their kinds, got {} and {}", project.tracks[1].name, project.tracks[2].name);
}
