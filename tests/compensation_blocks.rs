use bp::audio::vision::Seer;
use bp::audio::Audio;
use bp::fx::params::{Freak, FreakSlope};
use bp::fx::{Kind, Params, Slot};
use bp::project::{Clip, Project, Row, TICKS_PER_BEAT, Timing, Track};
use bp::wav::Wave;
use std::sync::Arc;
use support::{SAMPLE_RATE, beat_frame};

const NAME: &str = "clicks";
const CLICK: i16 = 8_192;
const SUMMED: f32 = 0.5;
const WITHIN: f32 = 1e-4;
const ROUNDING: f32 = 1e-9;
const EARLY: usize = 1_000;
const BEATS: usize = 8;
const CLIP_BEATS: i32 = 8;
const SIZES: [usize; 6] = [1, 63, 64, 65, 256, 4096];
const REFERENCE: usize = 256;
const SECONDS: usize = 2;
const LOOP_END: usize = 60_037;
const LOOP_SECONDS: usize = 2;
const UTILITY: usize = 71;
const RATES: [(f64, usize, usize); 2] = [(44_100.0, 65, 396), (96_000.0, 143, 862)];

fn utility() -> Row {
    let mut slot = Slot::new(Kind::Utility);
    if let Params::Utility(p) = &mut slot.params {
        p.limit = true;
        p.hard = false;
        p.ceiling_db = 0.0;
        p.lookahead_ms = 1.5;
    }
    Row::Effect(slot)
}

fn linear_crossover() -> Row {
    Row::Effect(Slot { params: Params::Freak(Freak { slope: FreakSlope::Linear, oversample: false, ..Freak::DEFAULT }), ..Slot::new(Kind::Freak) })
}

fn wave(rate: f64, clicks: &[usize]) -> Arc<Wave> {
    let frames = (BEATS + 1) * (rate * 60.0 / bp::project::DEFAULT_BPM) as usize;
    let mut pcm = vec![0i16; frames];
    for &frame in clicks {
        pcm[frame] = CLICK;
    }
    Arc::new(Wave::sound(String::from(NAME), rate, 1, pcm))
}

fn project(racks: &[Vec<Row>]) -> Project {
    let mut project = Project::new();
    project.tracks = racks
        .iter()
        .map(|rack| {
            let mut track = Track::audio();
            track.clips.push(Clip::audio(0, CLIP_BEATS * TICKS_PER_BEAT, String::from(NAME), String::from(NAME)));
            track.rack = rack.clone();
            track
        })
        .collect();
    project
}

fn render(project: &Project, waves: &[Arc<Wave>], rate: f64, size: usize, range: Option<(f64, f64)>, frames: usize) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(rate, size, Seer::Inline);
    audio.sync(project, None, &[], waves);
    audio.set_loop(range);
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; size], vec![0.0; size]);
    let mut out = Vec::with_capacity(frames + size);
    while out.len() < frames {
        renderer.render(&mut l, &mut r);
        out.extend_from_slice(&l);
    }
    out.truncate(frames);
    out
}

fn same_at_every_size(what: &str, project: &Project, waves: &[Arc<Wave>], range: Option<(f64, f64)>, frames: usize) {
    let reference = render(project, waves, SAMPLE_RATE, REFERENCE, range, frames);
    assert!(reference.iter().any(|sample| *sample != 0.0), "{what}: the rendering at {REFERENCE} frames is silent");
    for size in SIZES {
        let out = render(project, waves, SAMPLE_RATE, size, range, frames);
        let divergence = out.iter().zip(&reference).position(|(a, b)| (a - b).abs() > ROUNDING);
        let worst = out.iter().zip(&reference).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        assert_eq!(divergence, None, "{what}: wanted blocks of {size} to render as blocks of {REFERENCE} over {frames} frames within {ROUNDING}, got a difference at frame {divergence:?}, worst delta {worst}");
    }
}

#[test]
fn block_size_never_changes_a_latent_project() {
    let clicks: Vec<usize> = (0..BEATS).map(|beat| beat_frame(beat) + EARLY).collect();
    let waves = [wave(SAMPLE_RATE, &clicks)];
    let project = project(&[Vec::new(), vec![utility()]]);
    let output = Timing::of(&project, SAMPLE_RATE).output;
    assert_eq!(output, UTILITY, "wanted the Utility to make the output latency {UTILITY}, got {output}");
    same_at_every_size("straight", &project, &waves, None, SECONDS * SAMPLE_RATE as usize);
}

#[test]
fn a_loop_end_inside_a_block_renders_the_same_at_every_size() {
    let clicks: Vec<usize> = (0..BEATS).map(|beat| beat_frame(beat) + EARLY).collect();
    let waves = [wave(SAMPLE_RATE, &clicks)];
    let project = project(&[Vec::new(), vec![utility()]]);
    for size in SIZES.into_iter().filter(|&size| size > 1) {
        assert_ne!(LOOP_END % size, 0, "wanted the loop end at frame {LOOP_END} inside a block of {size}");
    }
    let range = Some((0.0, LOOP_END as f64 / (SAMPLE_RATE * 60.0 / bp::project::DEFAULT_BPM)));
    same_at_every_size("looped", &project, &waves, range, LOOP_SECONDS * SAMPLE_RATE as usize);
}

#[test]
fn the_joins_hold_at_forty_four_one_and_ninety_six() {
    for (rate, utility_lo, freak_lo) in RATES {
        let waves = [wave(rate, &[EARLY])];
        for (name, row, expected) in [("Utility", utility(), utility_lo), ("Freak", linear_crossover(), freak_lo)] {
            let project = project(&[Vec::new(), vec![row]]);
            let output = Timing::of(&project, rate).output;
            assert_eq!(output, expected, "wanted the {name} to report {expected} at {rate}, got {output}");
            let out = render(&project, &waves, rate, REFERENCE, None, rate as usize);
            let at = EARLY + expected;
            assert!((out[at] - SUMMED).abs() <= WITHIN, "at {rate} wanted the dry and the {name} clicks summing to {SUMMED} at frame {at}, got {}", out[at]);
            let stray = out.iter().enumerate().find(|(frame, sample)| *frame != at && sample.abs() > WITHIN).map(|(frame, &sample)| (frame, sample));
            assert_eq!(stray, None, "at {rate} wanted nothing but the one summed click at frame {at}, got {stray:?}");
        }
    }
}
