use bp::arp::{Arp, Articulation};
use bp::audio::vision::Seer;
use bp::audio::{Audio, Tail};
use bp::core::Params;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{AUDIBLE, BLOCK, FRAMES_PER_BEAT, SAMPLE_RATE, SILENT, WINDOW, beat_frame, core_layer, midi, peak_of};

const PITCH: u8 = 60;
const NOTE_START: f32 = 0.0;
const NOTE_LENGTH: f32 = 8.0;
const CLIP_LENGTH: i32 = 8 * TICKS_PER_BEAT;
const ATTACK_MS: f64 = 1.0;
const ATTACK_FRAMES: usize = (ATTACK_MS * SAMPLE_RATE / 1_000.0) as usize;
const FROM_BEAT: i32 = 4;
const SPAN_BEATS: i32 = 4;
const TEN_MS_FRAMES: usize = 480;
const SOUNDING: (f64, f64) = (7.25, 7.9);
const RELEASED: (f64, f64) = (8.25, 9.25);
const LOOP: (f64, f64) = (2.0, 6.0);
const WRAPS: u16 = 3;
const HEADER: usize = 68;
const BYTES_PER_FRAME: usize = 6;
const SCALE: f32 = 8_388_608.0;
const FADE: usize = 64;

fn project() -> Project {
    let mut patch = Params::DEFAULT;
    patch.amp.attack_ms = ATTACK_MS;
    patch.amp.sustain = 1.0;
    patch.amp.release_ms = 1.0;
    let notes = vec![Note { pitch: PITCH, start: NOTE_START, length: NOTE_LENGTH, velocity: VELOCITY, articulation: Articulation::PLAIN }];
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch))];
    project.tracks[0].clips.push(Clip { start: 0, length: CLIP_LENGTH, label: String::new(), notes, file: None });
    project
}

fn frames(beats: f64) -> usize {
    (beats * FRAMES_PER_BEAT).round() as usize
}

fn play(range: Option<(f64, f64)>, from: f64, total: usize) -> (Vec<f32>, u16) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(&project(), None, &[], &[]);
    audio.set_loop(range);
    audio.play(from);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(total);
    while out.len() < total {
        let n = BLOCK.min(total - out.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    (out, audio.position().wraps)
}

fn first_quiet_window(samples: &[f32], from: usize, to: usize) -> Option<(usize, f32)> {
    (from..to - WINDOW).step_by(WINDOW).chain([to - WINDOW]).map(|at| (at, peak_of(&samples[at..at + WINDOW]))).find(|&(_, peak)| peak <= AUDIBLE)
}

fn wrap_frame(lap: u16) -> usize {
    frames(LOOP.1 + f64::from(lap) * (LOOP.1 - LOOP.0))
}

fn span() -> (i32, i32) {
    (FROM_BEAT * TICKS_PER_BEAT, (FROM_BEAT + SPAN_BEATS) * TICKS_PER_BEAT)
}

fn left(wav: &[u8]) -> Vec<f32> {
    wav[HEADER..].as_chunks::<BYTES_PER_FRAME>().0.iter().map(|frame| (i32::from_le_bytes([0, frame[0], frame[1], frame[2]]) >> 8) as f32 / SCALE).collect()
}

#[test]
fn playing_from_inside_a_held_note_sounds_it_within_ten_ms_and_releases_it_at_its_end() {
    let from = f64::from(FROM_BEAT);
    let (out, _) = play(None, from, frames(RELEASED.1 - from));
    let heard = peak_of(&out[..TEN_MS_FRAMES]);
    assert!(heard > AUDIBLE, "wanted the note held from beat {NOTE_START} heard within frames 0 to {TEN_MS_FRAMES} of playback from beat {from}, got a peak of {heard}");
    let (start, end) = (frames(SOUNDING.0 - from), frames(SOUNDING.1 - from));
    let sounding = peak_of(&out[start..end]);
    assert!(sounding > AUDIBLE, "wanted the note still sounding over frames {start} to {end}, beats {} to {}, got a peak of {sounding}", SOUNDING.0, SOUNDING.1);
    let after = frames(RELEASED.0 - from);
    let released = peak_of(&out[after..]);
    assert!(released < SILENT, "wanted silence from frame {after}, beat {}, after the note's end at beat {}, got a peak of {released}", RELEASED.0, NOTE_START + NOTE_LENGTH);
}

#[test]
fn a_note_held_across_the_loop_start_sounds_through_the_ten_ms_after_every_wrap() {
    let (out, wraps) = play(Some(LOOP), 0.0, wrap_frame(WRAPS - 1) + TEN_MS_FRAMES);
    assert_eq!(wraps, WRAPS, "wanted the loop from beat {} to {} played from beat 0 to wrap {WRAPS} times in {} frames, got {wraps}", LOOP.0, LOOP.1, out.len());
    let at: Vec<usize> = (0..WRAPS).map(wrap_frame).collect();
    let quiet: Vec<(usize, f32)> = at.iter().filter_map(|&wrap| first_quiet_window(&out, wrap, wrap + TEN_MS_FRAMES)).collect();
    assert!(quiet.is_empty(), "wanted the note held from beat {NOTE_START} to {} sounding through the {TEN_MS_FRAMES} frames after each wrap at frames {at:?}, every {WINDOW} frames peaking over {AUDIBLE}, got quiet windows as (frame, peak) {quiet:?}", NOTE_START + NOTE_LENGTH);
}

#[test]
fn a_cut_bounce_starting_inside_a_held_note_sounds_from_frame_48_to_the_cut() {
    let wav = support::bounce(&project(), &[], span(), Tail::Cut).expect("wanted the span to bounce");
    let samples = left(&wav);
    let song = beat_frame(SPAN_BEATS as usize);
    assert_eq!(samples.len(), song, "wanted the bounce of beats {FROM_BEAT} to {} {song} frames long, got {}", FROM_BEAT + SPAN_BEATS, samples.len());
    let quiet = first_quiet_window(&samples, ATTACK_FRAMES, song - FADE);
    assert_eq!(quiet, None, "wanted the note held from beat {NOTE_START} sounding from frame {ATTACK_FRAMES}, one {ATTACK_MS} ms attack into the bounce from beat {FROM_BEAT}, to the cut's fade at frame {}, every {WINDOW} frames peaking over {AUDIBLE}, got a quiet window as (frame, peak) {quiet:?}", song - FADE);
}
