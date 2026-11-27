use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::fx::{self, Kind, Slot};
use bp::project::{Clip, Project, Row, TICKS_PER_BEAT, Track};
use bp::wav::Wave;
use std::f64::consts::TAU;
use std::sync::Arc;
use support::{BLOCK, FRAMES_PER_BEAT, SAMPLE_RATE, first_difference, peak_of};

const NAME: &str = "tone";
const TONE_HZ: f64 = 220.0;
const LEVEL: f64 = 16_384.0;
const BEATS: i32 = 4;
const FROM_BEAT: f64 = 1.0;
const TO_BEAT: f64 = 3.0;
const SQUEEZED_DB: f64 = 12.0;
const UNTOUCHED_DB: f64 = 0.01;

fn tone(opposed: bool) -> Arc<Wave> {
    let frames = (f64::from(BEATS) * FRAMES_PER_BEAT) as usize;
    let pcm = (0..frames)
        .flat_map(|frame| {
            let left = (LEVEL * (TAU * TONE_HZ * frame as f64 / SAMPLE_RATE).sin()).round() as i16;
            [left, if opposed { -left } else { left }]
        })
        .collect();
    Arc::new(Wave::sound(String::from(NAME), SAMPLE_RATE, 2, pcm))
}

fn compressor(sum: bool) -> Row {
    let mut slot = Slot::new(Kind::Compressor);
    if let fx::Params::Compressor(p) = &mut slot.params {
        p.sum = sum;
        p.auto = false;
        p.threshold_db = -40.0;
        p.ratio = 20.0;
        p.knee_db = 0.0;
        p.attack_ms = 0.1;
    }
    Row::Effect(slot)
}

fn project(rack: Vec<Row>) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0] = Track::audio();
    project.tracks[0].rack = rack;
    project.tracks[0].clips.push(Clip::audio(0, BEATS * TICKS_PER_BEAT, String::from(NAME), String::from(NAME)));
    project
}

fn render(project: &Project, wave: &Arc<Wave>) -> (Vec<f32>, Vec<f32>) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(project, None, &[], std::slice::from_ref(wave));
    audio.play(0.0);
    let frames = (TO_BEAT * FRAMES_PER_BEAT) as usize;
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let (mut left, mut right) = (Vec::with_capacity(frames), Vec::with_capacity(frames));
    while left.len() < frames {
        let n = BLOCK.min(frames - left.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        left.extend_from_slice(&l[..n]);
        right.extend_from_slice(&r[..n]);
    }
    (left, right)
}

fn held(samples: &[f32]) -> f64 {
    f64::from(peak_of(&samples[(FROM_BEAT * FRAMES_PER_BEAT) as usize..]))
}

fn db(level: f64, against: f64) -> f64 {
    20.0 * (level / against).log10()
}

#[test]
fn a_mono_tone_is_squeezed_alike_by_either_detector() {
    let wave = tone(false);
    let (open, _) = render(&project(Vec::new()), &wave);
    let (louder, louder_right) = render(&project(vec![compressor(false)]), &wave);
    let (summed, summed_right) = render(&project(vec![compressor(true)]), &wave);
    let squeeze = db(held(&louder), held(&open));
    assert!(squeeze < -SQUEEZED_DB, "wanted the louder-side detector to squeeze the mono tone {SQUEEZED_DB} dB or more from beat {FROM_BEAT}, got {squeeze:.2} dB");
    for (side, louder, summed) in [("left", &louder, &summed), ("right", &louder_right, &summed_right)] {
        let frame = first_difference(std::slice::from_ref(louder), std::slice::from_ref(summed));
        assert_eq!(frame, None, "wanted the summed detector to squeeze a mono tone exactly as the louder side does, the {side} channel differs");
    }
}

#[test]
fn opposite_sides_pass_the_summed_detector_and_squeeze_under_the_louder_side() {
    let wave = tone(true);
    let (open, open_right) = render(&project(Vec::new()), &wave);
    let (louder, _) = render(&project(vec![compressor(false)]), &wave);
    let (summed, summed_right) = render(&project(vec![compressor(true)]), &wave);
    assert!(held(&open) > 0.25, "the opposed tone is silent, peak {:.4}", held(&open));
    let squeeze = db(held(&louder), held(&open));
    assert!(squeeze < -SQUEEZED_DB, "wanted the louder-side detector to squeeze opposite sides {SQUEEZED_DB} dB or more from beat {FROM_BEAT}, got {squeeze:.2} dB");
    for (side, open, summed) in [("left", &open, &summed), ("right", &open_right, &summed_right)] {
        let change = db(held(summed), held(open));
        assert!(change.abs() < UNTOUCHED_DB, "wanted the summed detector to leave opposite sides untouched in the {side} channel from beat {FROM_BEAT}, got {change:.4} dB");
    }
}
