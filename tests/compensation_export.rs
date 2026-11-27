use bp::arp::{Arp, Articulation};
use bp::audio::Tail;
use bp::core::Params as CoreParams;
use bp::fx::params::{Freak, FreakSlope};
use bp::fx::{Kind, Params, Slot};
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BAR, TICKS_PER_BEAT, Timing, Track, VELOCITY};
use bp::wav::Wave;
use std::sync::Arc;
use support::{SAMPLE_RATE, SILENT, beat_frame, core_layer, midi};

const NAME: &str = "clicks";
const CLICK: i16 = 8_192;
const LEVEL: f32 = 0.25;
const WITHIN: f32 = 1e-4;
const QUANTIZED: f32 = 1e-6;
const HEADER: usize = 68;
const BYTES_PER_FRAME: usize = 6;
const SCALE: f32 = 8_388_608.0;
const FADE: usize = 64;
const EARLY: usize = 1_000;
const BEFORE_END: usize = 100;
const SPAN_BEATS: i32 = 4;
const CLIP_BEATS: i32 = 8;
const WAVE_SECONDS: usize = 10;
const LIN: usize = 431;
const POLISH: usize = 976;
const POLISH_LOOKAHEAD_MS: f64 = 2.4;
const TAIL_AT_LEAST: usize = 48_000;
const CORE_LEVEL: f64 = 0.25;
const NOTE_BEAT: f32 = 1.0;

fn utility(ms: f64) -> Row {
    let mut slot = Slot::new(Kind::Utility);
    if let Params::Utility(p) = &mut slot.params {
        p.limit = true;
        p.hard = false;
        p.ceiling_db = 0.0;
        p.lookahead_ms = ms;
    }
    Row::Effect(slot)
}

fn linear_crossover() -> Row {
    Row::Effect(Slot { params: Params::Freak(Freak { slope: FreakSlope::Linear, oversample: false, ..Freak::DEFAULT }), ..Slot::new(Kind::Freak) })
}

fn clicks(at: &[usize]) -> Arc<Wave> {
    let mut pcm = vec![0i16; WAVE_SECONDS * SAMPLE_RATE as usize];
    for &frame in at {
        pcm[frame] = CLICK;
    }
    Arc::new(Wave::sound(String::from(NAME), SAMPLE_RATE, 1, pcm))
}

fn audio_track(rack: Vec<Row>) -> Track {
    let mut track = Track::audio();
    track.clips.push(Clip::audio(0, CLIP_BEATS * TICKS_PER_BEAT, String::from(NAME), String::from(NAME)));
    track.rack = rack;
    track
}

fn project(tracks: Vec<Track>) -> Project {
    let mut project = Project::new();
    project.tracks = tracks;
    project
}

fn left(wav: &[u8]) -> Vec<f32> {
    wav[HEADER..].as_chunks::<BYTES_PER_FRAME>().0.iter().map(|frame| (i32::from_le_bytes([0, frame[0], frame[1], frame[2]]) >> 8) as f32 / SCALE).collect()
}

fn export(project: &Project, waves: &[Arc<Wave>], span: (i32, i32), tail: Tail) -> Vec<f32> {
    left(&support::bounce(project, waves, (span.0 * TICKS_PER_BEAT, span.1 * TICKS_PER_BEAT), tail).expect("wanted the project to export"))
}

fn stray(out: &[f32], clicks: &[usize]) -> Option<(usize, f32)> {
    out.iter().enumerate().find(|(frame, sample)| !clicks.contains(frame) && sample.abs() > SILENT).map(|(frame, &sample)| (frame, sample))
}

fn first_sound(out: &[f32]) -> Option<usize> {
    out.iter().position(|sample| sample.abs() > SILENT)
}

fn worst_delta(a: &[f32], b: &[f32]) -> (usize, f32) {
    a.iter().zip(b).enumerate().map(|(frame, (x, y))| (frame, (x - y).abs())).fold((0, 0.0), |worst, candidate| if candidate.1 > worst.1 { candidate } else { worst })
}

fn song_clicks() -> (usize, Vec<usize>) {
    let song = beat_frame(SPAN_BEATS as usize);
    (song, vec![EARLY, beat_frame(1) + EARLY, song - BEFORE_END, song - 1])
}

#[test]
fn the_cut_export_of_a_latent_track_is_the_span_long_with_every_click_in_place() {
    let (song, at) = song_clicks();
    let waves = [clicks(&at)];
    let project = project(vec![audio_track(vec![linear_crossover()])]);
    let output = Timing::of(&project, SAMPLE_RATE).output;
    assert_eq!(output, LIN, "wanted the LIN Freak wire to set the output latency to {LIN}, got {output}");
    let out = export(&project, &waves, (0, SPAN_BEATS), Tail::Cut);
    assert_eq!(out.len(), song, "wanted the cut export exactly {song} frames long, got {}", out.len());
    let early = first_sound(&out[..EARLY]);
    assert_eq!(early, None, "wanted the first {EARLY} frames silent before the first click, got sound at frame {early:?}");
    for frame in [EARLY, beat_frame(1) + EARLY, song - BEFORE_END] {
        assert!((out[frame] - LEVEL).abs() <= WITHIN, "wanted the click at frame {frame} at {LEVEL} in the file, got {}", out[frame]);
    }
    let faded = LEVEL / FADE as f32;
    assert!((out[song - 1] - faded).abs() <= WITHIN, "wanted the span's final frame's click at {faded} under the cut fade's last step, got {}", out[song - 1]);
    let stray = stray(&out, &at);
    assert_eq!(stray, None, "wanted nothing but the clicks in the cut export, got {stray:?}");
}

#[test]
fn the_rings_export_keeps_the_span_and_runs_on_into_its_tail() {
    let (song, at) = song_clicks();
    let waves = [clicks(&at)];
    let project = project(vec![audio_track(vec![linear_crossover()])]);
    let cut = export(&project, &waves, (0, SPAN_BEATS), Tail::Cut);
    let rings = export(&project, &waves, (0, SPAN_BEATS), Tail::Rings);
    assert!(rings.len() >= song + TAIL_AT_LEAST, "wanted the rings export to run at least {TAIL_AT_LEAST} frames past the span's {song}, got {} frames", rings.len());
    let divergence = cut[..song - FADE].iter().zip(&rings).position(|(a, b)| a.to_bits() != b.to_bits());
    assert_eq!(divergence, None, "wanted the rings export to hold the span's content sample for sample before the cut fade, got a difference at frame {divergence:?}");
    assert!((rings[song - 1] - LEVEL).abs() <= WITHIN, "wanted the final frame's click unfaded at {LEVEL} under rings, got {}", rings[song - 1]);
    let tail = first_sound(&rings[song..]).map(|frame| song + frame);
    assert_eq!(tail, None, "wanted the tail of a project of clicks silent, got sound at frame {tail:?}");
}

#[test]
fn a_span_starting_mid_song_keeps_the_click_at_n_minus_the_span_start() {
    let (song, at) = song_clicks();
    let waves = [clicks(&at)];
    let project = project(vec![audio_track(vec![linear_crossover()])]);
    let out = export(&project, &waves, (1, 1 + SPAN_BEATS), Tail::Cut);
    assert_eq!(out.len(), song, "wanted the mid-song export exactly {song} frames long, got {}", out.len());
    let start = beat_frame(1);
    let expected = [EARLY, song - BEFORE_END - start, song - 1 - start];
    for frame in expected {
        assert!((out[frame] - LEVEL).abs() <= WITHIN, "wanted the click at song frame {} at file frame {frame} at {LEVEL}, got {}", frame + start, out[frame]);
    }
    let stray = stray(&out, &expected);
    assert_eq!(stray, None, "wanted nothing but the span's clicks in the mid-song export, got {stray:?}");
}

#[test]
fn a_project_with_no_latent_row_exports_as_before() {
    let (song, at) = song_clicks();
    let waves = [clicks(&at)];
    let dry = export(&project(vec![audio_track(Vec::new())]), &waves, (0, SPAN_BEATS), Tail::Cut);
    let latent = export(&project(vec![audio_track(vec![linear_crossover()])]), &waves, (0, SPAN_BEATS), Tail::Cut);
    assert_eq!(dry.len(), song, "wanted the dry export exactly {song} frames long, got {}", dry.len());
    for frame in [EARLY, beat_frame(1) + EARLY, song - BEFORE_END] {
        assert!((dry[frame] - LEVEL).abs() <= WITHIN, "wanted the dry export's click at frame {frame} at {LEVEL}, got {}", dry[frame]);
    }
    let stray = stray(&dry, &at);
    assert_eq!(stray, None, "wanted nothing but the clicks in the dry export, got {stray:?}");
    let (frame, delta) = worst_delta(&dry, &latent);
    assert!(delta <= QUANTIZED, "wanted the latent export to match the dry one within {QUANTIZED}, got a difference of {delta} at frame {frame}");
}

#[test]
fn a_core_note_through_a_976_sample_rack_exports_with_its_onset_on_the_beat() {
    let mut patch = CoreParams::DEFAULT;
    patch.amp.attack_ms = 1.0;
    patch.amp.release_ms = 1.0;
    let core = Row::Core(core_layer(CORE_LEVEL, patch));
    let mut dry = Track::blank();
    dry.rack = vec![midi(Arp::DEFAULT), core];
    dry.clips.push(Clip { start: 0, length: TICKS_PER_BAR, label: String::new(), notes: vec![Note { pitch: 60, start: NOTE_BEAT, length: 1.0, velocity: VELOCITY, articulation: Articulation::PLAIN }], file: None });
    let mut latent = dry.clone();
    latent.rack = vec![midi(Arp::DEFAULT), core, linear_crossover(), linear_crossover(), utility(POLISH_LOOKAHEAD_MS)];
    let latent = project(vec![latent]);
    let output = Timing::of(&latent, SAMPLE_RATE).output;
    assert_eq!(output, POLISH, "wanted two LIN Freaks and a {POLISH_LOOKAHEAD_MS} ms Utility to report {POLISH}, got {output}");
    let dry_out = export(&project(vec![dry]), &[], (0, SPAN_BEATS), Tail::Cut);
    let latent_out = export(&latent, &[], (0, SPAN_BEATS), Tail::Cut);
    let beat = beat_frame(NOTE_BEAT as usize);
    let (dry_onset, latent_onset) = (first_sound(&dry_out), first_sound(&latent_out));
    assert!(dry_onset.is_some_and(|onset| onset.abs_diff(beat) <= 1), "wanted the dry Core's onset at frame {beat} within one frame, got {dry_onset:?}");
    assert!(latent_onset.is_some_and(|onset| onset.abs_diff(beat) <= 1), "wanted the onset through the {POLISH} sample rack at frame {beat} within one frame, got {latent_onset:?}");
    assert_eq!(latent_out.len(), dry_out.len(), "wanted both exports the same length, got {} and {}", latent_out.len(), dry_out.len());
    let (frame, delta) = worst_delta(&dry_out, &latent_out);
    assert!(delta <= QUANTIZED, "wanted the rack's export to equal the dry Core's within {QUANTIZED}, got a difference of {delta} at frame {frame}");
}
