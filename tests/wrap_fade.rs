use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::project::{Clip, Project, TICKS_PER_BEAT, Track};
use bp::wav::Wave;
use std::sync::Arc;
use support::{BLOCK, FRAMES_PER_BEAT, SAMPLE_RATE};

const NAME: &str = "held";
const QUARTER: i16 = 8_192;
const LEVEL: f32 = 0.25;
const WAVE_SECONDS: usize = 10;
const CLIP_BEATS: i32 = 16;
const FADE: usize = 64;
const BLOCKS_TO_THE_WRAP: usize = 188;
const WRAP: usize = BLOCKS_TO_THE_WRAP * BLOCK - 1;
const AROUND: usize = 2 * FADE;
const BLOCKS: usize = 200;
const STEP_ROOM: f32 = 2.0;

fn project() -> Project {
    let mut track = Track::audio();
    track.clips.push(Clip::audio(0, CLIP_BEATS * TICKS_PER_BEAT, String::from(NAME), String::from(NAME)));
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0] = track;
    project
}

#[test]
fn a_clip_cut_by_a_wrap_one_frame_before_the_blocks_end_crosses_without_a_tick() {
    let held = Arc::new(Wave::sound(String::from(NAME), SAMPLE_RATE, 1, vec![QUARTER; WAVE_SECONDS * SAMPLE_RATE as usize]));
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(&project(), None, &[], &[held]);
    audio.set_loop(Some((0.0, WRAP as f64 / FRAMES_PER_BEAT)));
    audio.play(0.0);
    let mut out = Vec::with_capacity(BLOCKS * BLOCK);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    for _ in 0..BLOCKS {
        renderer.render(&mut l, &mut r);
        out.extend_from_slice(&l);
    }
    assert_eq!(audio.position().wraps, 1, "wanted the loop to wrap once, {WRAP} frames in");
    let before = out[WRAP - AROUND];
    assert!((before - LEVEL).abs() < LEVEL / FADE as f32, "wanted the held clip at {LEVEL} before the wrap, got {before}");
    let steepest = out[WRAP - AROUND..WRAP + AROUND].windows(2).map(|pair| (pair[1] - pair[0]).abs()).fold(0.0f32, f32::max);
    let allowed = STEP_ROOM * LEVEL / FADE as f32;
    assert!(steepest < allowed, "wanted the clip leaving at the wrap, a frame before the block's end, to fade over {FADE} frames under the clip coming in, no step over {allowed}, got a step of {steepest}");
}
