use bp::arp::Arp;
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::core::params::Envelope;
use bp::division::Division;
use bp::project::{Aim, LaneLfo, LfoShape, Project, Row, VELOCITY};
use support::{AUDIBLE, BLOCK, CORE_ROW, Onsets, SAMPLE_RATE, WINDOW, core_layer, midi, peak_of};

const KEY: u8 = 69;
const STEP_SECONDS: f64 = 0.125;
const STEP_FRAMES: usize = (STEP_SECONDS * SAMPLE_RATE) as usize;
const STEPS: usize = 16;
const LEAD_SECONDS: f64 = STEP_SECONDS * 0.75;
const TAIL_SECONDS: f64 = 0.05;
const HALF_GATE: f64 = 0.5;
const LATE: usize = WINDOW * 11;
const FLAT: f32 = 0.03;
const HELD_STEPS: usize = 8;

fn patch() -> Params {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.osc1.level = 0.5;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    patch
}

fn line_arp(gate: f64) -> Arp {
    Arp { on: true, sync: false, time_ms: STEP_SECONDS * 1000.0, gate, ..Arp::DEFAULT }
}

fn project(lfo: LaneLfo, gate: f64) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(line_arp(gate)), Row::Core(core_layer(1.0, patch()))];
    track.automation.lfos.push(lfo);
    project
}

fn line(project: &Project) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(project, None, &[], &[]);
    let mut out = Vec::new();
    let mut render = |out: &mut Vec<f32>, seconds: f64| {
        let end = out.len() + (seconds * SAMPLE_RATE).round() as usize;
        let (mut left, mut right) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
        while out.len() < end {
            let n = BLOCK.min(end - out.len());
            renderer.render(&mut left[..n], &mut right[..n]);
            out.extend_from_slice(&left[..n]);
        }
    };
    render(&mut out, LEAD_SECONDS);
    audio.note_on(0, KEY, VELOCITY);
    render(&mut out, STEPS as f64 * STEP_SECONDS + TAIL_SECONDS);
    out
}

fn first_step(out: &[f32]) -> usize {
    let mut onsets = Onsets::default();
    onsets.feed(out);
    let key = (LEAD_SECONDS * SAMPLE_RATE) as usize;
    let first = onsets.at.first().copied().unwrap_or(0);
    assert!(first >= key && first <= key + BLOCK, "wanted the arp line's first step within a block of the key at frame {key}, got onsets at {:?}", onsets.at);
    first
}

fn measured(out: &[f32], first: usize, offsets: &[usize]) -> Vec<Vec<f32>> {
    let peaks: Vec<Vec<f32>> = (0..STEPS)
        .map(|step| {
            offsets
                .iter()
                .map(|offset| {
                    let from = first + step * STEP_FRAMES + offset;
                    peak_of(&out[from..from + WINDOW])
                })
                .collect()
        })
        .collect();
    let loudest = peaks.iter().flatten().copied().fold(0.0, f32::max);
    assert!(loudest > AUDIBLE, "wanted the arp line heard through the windows measured from frame {first}, got a loudest window of {loudest}");
    peaks.iter().map(|step| step.iter().map(|peak| peak / loudest).collect()).collect()
}

#[test]
fn a_random_lane_on_the_song_clock_holds_its_draw_through_the_steps() {
    let whole = LaneLfo { amount: 1.0, shape: LfoShape::Random, sync: true, division: Division::Bar, ..LaneLfo::blank(Aim::knob(CORE_ROW, "OSC 1", "LEVEL")) };
    let out = line(&project(whole, HALF_GATE));
    let first = first_step(&out);
    let held: Vec<f32> = measured(&out, first, &[LATE]).iter().map(|window| window[0]).collect();
    for (step, level) in held.iter().enumerate().take(HELD_STEPS).skip(1) {
        let start = first + step * STEP_FRAMES;
        assert!((level - held[0]).abs() <= FLAT, "wanted step {step} at frame {start} to hold step 0's level {:.3}, one whole-note cycle covering the first {HELD_STEPS} steps, got {level:.3}", held[0]);
    }
}
