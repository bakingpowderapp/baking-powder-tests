use bp::arp::{Arp, Articulation, Lane, Pattern};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::division::Division;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{BLOCK, FRAMES_PER_BEAT, SAMPLE_RATE, core_layer, midi};

const C4: u8 = 60;
const STEPS: usize = 4;
const BEATS: usize = 8;
const ATTACK_MS: f64 = 30.0;
const RELEASE_MS: f64 = 100.0;
const BEFORE_FRAMES: usize = 480;
const AFTER_START: usize = 192;
const AFTER_FRAMES: usize = 384;
const RESTARTED_RATIO: f64 = 0.5;
const RAN_ON_RATIO: f64 = 0.9;

fn full_gate_line() -> Pattern {
    let mut pattern = Pattern::DEFAULT;
    pattern.set_length(STEPS);
    for index in 0..STEPS {
        pattern.set(Lane::Gate, index, Lane::Gate.range().1);
    }
    pattern
}

fn project(fresh: bool) -> Project {
    let mut patch = Params::DEFAULT;
    patch.voices = 1;
    patch.fresh = fresh;
    patch.mono.retrigger = true;
    patch.amp.attack_ms = ATTACK_MS;
    patch.amp.decay_ms = 10.0;
    patch.amp.sustain = 1.0;
    patch.amp.release_ms = RELEASE_MS;
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp { on: true, stepped: true, rate: Division::Quarter, sync: true, pattern: full_gate_line(), ..Arp::DEFAULT }), Row::Core(core_layer(0.25, patch))];
    let notes = vec![Note { pitch: C4, start: 0.0, length: BEATS as f32, velocity: VELOCITY, articulation: Articulation::PLAIN }];
    track.clips.push(Clip { start: 0, length: BEATS as i32 * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn render(project: &Project, frames: usize) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(project, None, &[], &[]);
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(frames);
    while out.len() < frames {
        let n = BLOCK.min(frames - out.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    out
}

fn rms(samples: &[f32]) -> f64 {
    (samples.iter().map(|s| f64::from(*s) * f64::from(*s)).sum::<f64>() / samples.len() as f64).sqrt()
}

fn dips(fresh: bool) -> Vec<(usize, f64)> {
    let heard = render(&project(fresh), (BEATS as f64 * FRAMES_PER_BEAT) as usize);
    (1..BEATS - 1)
        .map(|beat| {
            let at = (beat as f64 * FRAMES_PER_BEAT).round() as usize;
            let before = rms(&heard[at - BEFORE_FRAMES..at]);
            let after = rms(&heard[at + AFTER_START..at + AFTER_START + AFTER_FRAMES]);
            (at, after / before)
        })
        .collect()
}

#[test]
fn a_full_gate_step_on_the_same_pitch_restarts_a_fresh_mono_voice() {
    for (at, ratio) in dips(true) {
        assert!(ratio < RESTARTED_RATIO, "wanted the step at frame {at} to start its attack over, the level 4 to 12 ms in under {RESTARTED_RATIO} of the level before it, got {ratio:.2}");
    }
}

#[test]
fn without_a_fresh_voice_the_same_pitch_runs_on() {
    for (at, ratio) in dips(false) {
        assert!(ratio > RAN_ON_RATIO, "wanted the step at frame {at} to run on at its sustain, the level 4 to 12 ms in over {RAN_ON_RATIO} of the level before it, got {ratio:.2}");
    }
}
