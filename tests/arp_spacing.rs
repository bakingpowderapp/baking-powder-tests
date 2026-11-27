use bp::arp::Arp;
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::division::Division;
use bp::project::{Project, Row};
use support::{FRAMES_PER_BEAT, GAP, SAMPLE_RATE, SILENT, core_layer, midi};

const SECONDS: f64 = 4.0;
const RATE: Division = Division::ThirtySecondDotted;
const BUFFERS: [usize; 3] = [2048, 512, 256];
const WITHIN: usize = 2;

fn percussive_arp() -> Project {
    let mut patch = Params::DEFAULT;
    patch.amp.attack_ms = 1.0;
    patch.amp.decay_ms = 10.0;
    patch.amp.sustain = 0.0;
    patch.amp.release_ms = 1.0;
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp { on: true, rate: RATE, ..Arp::DEFAULT }), Row::Core(core_layer(1.0, patch))];
    project
}

fn onsets_while_holding(project: &Project, block: usize) -> Vec<usize> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, block, Seer::Inline);
    audio.sync(project, None, &[], &[]);
    audio.note_on(0, 60, 127);
    let (mut l, mut r) = (vec![0.0; block], vec![0.0; block]);
    let (mut frame, mut quiet, mut at) = (0, GAP, Vec::new());
    while frame < (SECONDS * SAMPLE_RATE) as usize {
        renderer.render(&mut l, &mut r);
        for &sample in &l {
            if sample.abs() < SILENT {
                quiet += 1;
            } else {
                if quiet >= GAP {
                    at.push(frame);
                }
                quiet = 0;
            }
            frame += 1;
        }
    }
    at
}

fn histogram(intervals: &[usize]) -> String {
    let mut counted: Vec<(usize, usize)> = Vec::new();
    for &interval in intervals {
        match counted.iter_mut().find(|(seen, _)| *seen == interval) {
            Some((_, count)) => *count += 1,
            None => counted.push((interval, 1)),
        }
    }
    counted.sort_unstable();
    counted.iter().map(|(interval, count)| format!("{interval} ×{count}")).collect::<Vec<_>>().join(", ")
}

#[test]
fn dotted_thirty_second_arp_keeps_its_spacing_at_any_buffer() {
    let project = percussive_arp();
    let step = (RATE.beats() * FRAMES_PER_BEAT).round() as usize;
    let uneven: Vec<String> = BUFFERS
        .iter()
        .filter_map(|&block| {
            let onsets = onsets_while_holding(&project, block);
            let intervals: Vec<usize> = onsets.windows(2).map(|pair| pair[1] - pair[0]).collect();
            let off = intervals.iter().any(|interval| interval.abs_diff(step) > WITHIN);
            (intervals.len() < 4 || off).then(|| format!("buffer {block}: steps of {} frames", histogram(&intervals)))
        })
        .collect();
    assert!(uneven.is_empty(), "wanted every step {step} frames apart; {}", uneven.join("; "));
}
