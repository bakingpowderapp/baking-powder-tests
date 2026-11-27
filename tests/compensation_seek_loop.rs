use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::fx::params::{Freak, FreakSlope};
use bp::fx::{Kind, Params, Slot};
use bp::project::{Clip, Project, Row, TICKS_PER_BEAT, Track};
use bp::wav::Wave;
use std::sync::Arc;
use support::{BLOCK, FRAMES_PER_BEAT, SAMPLE_RATE, SILENT, beat_frame};

const NAME: &str = "clicks";
const CLICK: i16 = 8_192;
const SUMMED: f32 = 0.5;
const WITHIN: f32 = 1e-3;
const FADE: f32 = 64.0;
const EARLY: usize = 1_000;
const BEATS: usize = 40;
const LIN: usize = 431;
const SEEK_AT: usize = 49_100;
const LAST_BEFORE: usize = SEEK_AT - 1;
const LANDING: usize = 238_000;
const SOON_AFTER: usize = 100;
const PLAY_ON: usize = 60_000;
const LOOP_FRAMES: usize = 300;
const LOOP_SECONDS: usize = 3;
const STOPPED_FOR: usize = 4_800;
const RESTARTED_FOR: usize = 3_000;

fn linear_crossover() -> Row {
    Row::Effect(Slot { params: Params::Freak(Freak { slope: FreakSlope::Linear, oversample: false, ..Freak::DEFAULT }), ..Slot::new(Kind::Freak) })
}

fn beat_clicks() -> Vec<usize> {
    (0..BEATS).map(|beat| beat_frame(beat) + EARLY).collect()
}

fn clicks(at: &[usize]) -> Arc<Wave> {
    let mut pcm = vec![0i16; beat_frame(BEATS + 1)];
    for &frame in at {
        pcm[frame] = CLICK;
    }
    Arc::new(Wave::sound(String::from(NAME), SAMPLE_RATE, 1, pcm))
}

fn project() -> Project {
    let mut project = Project::new();
    project.tracks = [Vec::new(), vec![linear_crossover()]]
        .into_iter()
        .map(|rack| {
            let mut track = Track::audio();
            track.clips.push(Clip::audio(0, BEATS as i32 * TICKS_PER_BEAT, String::from(NAME), String::from(NAME)));
            track.rack = rack;
            track
        })
        .collect();
    project
}

fn render(renderer: &mut Renderer, frames: usize, out: &mut Vec<f32>) {
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut done = 0;
    while done < frames {
        let n = BLOCK.min(frames - done);
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
        done += n;
    }
}

fn beat_of(frame: usize) -> f64 {
    frame as f64 / FRAMES_PER_BEAT
}

fn loud(out: &[f32]) -> Vec<(usize, f32)> {
    out.iter().enumerate().filter(|(_, sample)| sample.abs() > WITHIN).map(|(frame, &sample)| (frame, sample)).collect()
}

fn expect_summed_clicks(what: &str, out: &[f32], from: usize, expected: &[usize]) {
    let heard = loud(&out[from..]);
    let frames: Vec<usize> = heard.iter().map(|(frame, _)| from + frame).collect();
    assert_eq!(frames, expected, "{what}: wanted summed clicks at exactly the frames {expected:?}, got sound over {WITHIN} at {frames:?}");
    for (frame, sample) in heard {
        assert!((sample - SUMMED).abs() <= WITHIN, "{what}: wanted both tracks' clicks coinciding at frame {} as {SUMMED}, got {sample}", from + frame);
    }
}

#[test]
fn a_seek_while_playing_plays_out_the_last_lo_frames_then_the_new_position_aligned() {
    let mut at = beat_clicks();
    at.extend([LAST_BEFORE, LANDING, LANDING + SOON_AFTER]);
    let waves = [clicks(&at)];
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(&project(), None, &[], &waves);
    let lo = audio.timing().output;
    assert_eq!(lo, LIN, "wanted the LIN Freak to make the output latency {LIN}, got {lo}");
    audio.play(0.0);
    let mut out = Vec::with_capacity(SEEK_AT + PLAY_ON);
    render(&mut renderer, SEEK_AT, &mut out);
    audio.play(beat_of(LANDING));
    render(&mut renderer, PLAY_ON, &mut out);
    let played_out = beat_frame(2) + EARLY + lo;
    let edge = SEEK_AT + lo;
    let expected = [EARLY + lo, beat_frame(1) + EARLY + lo, played_out, LAST_BEFORE + lo, edge + SOON_AFTER, SEEK_AT + beat_frame(10) + EARLY - LANDING + lo, SEEK_AT + beat_frame(11) + EARLY - LANDING + lo, SEEK_AT + beat_frame(12) + EARLY - LANDING + lo];
    let first_new = SUMMED / FADE;
    assert!((out[edge] - first_new).abs() <= WITHIN, "wanted the new position's first frame at output frame {edge}, exactly {lo} after the seek at {SEEK_AT}, under the clip's fade-in at {first_new}, got {}", out[edge]);
    out[edge] = 0.0;
    expect_summed_clicks("seek", &out, 0, &expected);
}

#[test]
fn a_loop_shorter_than_lo_wraps_seamlessly_with_both_tracks_coinciding() {
    let waves = [clicks(&beat_clicks())];
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(&project(), None, &[], &waves);
    let lo = audio.timing().output;
    assert!(LOOP_FRAMES < lo, "wanted a loop of {LOOP_FRAMES} frames shorter than the output latency, got a latency of {lo}");
    let click = beat_frame(1) + EARLY;
    let (start, end) = (click - LOOP_FRAMES / 2, click + LOOP_FRAMES / 2);
    audio.set_loop(Some((beat_of(start), beat_of(end))));
    audio.play(0.0);
    let total = LOOP_SECONDS * SAMPLE_RATE as usize;
    let mut out = Vec::with_capacity(total);
    render(&mut renderer, total, &mut out);
    let wraps = audio.position().wraps;
    let laps = (total - end) / LOOP_FRAMES;
    assert!(usize::from(wraps) >= laps, "wanted the {LOOP_FRAMES} frame loop to wrap at least {laps} times in {LOOP_SECONDS} seconds, got {wraps}");
    let mut expected = vec![EARLY + lo];
    expected.extend((0..).map(|lap| click + lo + lap * LOOP_FRAMES).take_while(|&frame| frame < total));
    expect_summed_clicks("loop", &out, 0, &expected);
}

#[test]
fn play_from_stop_resets_the_lines_so_the_first_lo_frames_are_silent() {
    let waves = [clicks(&beat_clicks())];
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(&project(), None, &[], &waves);
    let lo = audio.timing().output;
    audio.play(0.0);
    let mut out = Vec::with_capacity(SEEK_AT + STOPPED_FOR + RESTARTED_FOR);
    render(&mut renderer, SEEK_AT, &mut out);
    audio.stop();
    render(&mut renderer, STOPPED_FOR, &mut out);
    audio.play(0.0);
    let restarted = out.len();
    render(&mut renderer, RESTARTED_FOR, &mut out);
    let again = &out[restarted..];
    let early = again[..lo].iter().position(|sample| sample.abs() > SILENT);
    assert_eq!(early, None, "wanted the first {lo} frames after play from stop silent, got sound at frame {early:?}");
    expect_summed_clicks("play from stop", again, 0, &[EARLY + lo]);
}
