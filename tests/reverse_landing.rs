use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params as CoreParams;
use bp::core::params::Envelope;
use bp::fx::params::{Delay, DelayMode};
use bp::fx::{Kind, Params, Slot};
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{BLOCK, SAMPLE_RATE, core_layer, midi};

const SECONDS: f64 = 3.0;
const TOLERANCE: f32 = 1e-5;
const AUDIBLE: f32 = 1e-4;

fn song() -> Project {
    let mut patch = CoreParams::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    let delay = Row::Effect(Slot { params: Params::Delay(Delay { mode: DelayMode::Clean, time_l_ms: 500.0, time_r_ms: 500.0, feedback: 0.0, forward: 0.0, reverse: 1.0, ..Delay::DEFAULT }), ..Slot::new(Kind::Delay) });
    let mut project = Project::new();
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(0.1, patch)), delay];
    project.tracks[0].clips = vec![Clip { start: 0, length: 8 * TICKS_PER_BEAT, label: String::new(), notes: vec![Note { pitch: 69, start: 4.0, length: 2.0, velocity: VELOCITY, articulation: Articulation::PLAIN }], file: None }];
    project
}

fn render(mut project: Project, relanding: Option<usize>) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(&project, None, &[], &[]);
    audio.play(0.0);
    let frames = (SECONDS * SAMPLE_RATE) as usize;
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(frames);
    let mut relanding = relanding;
    while out.len() < frames {
        if relanding == Some(out.len()) {
            project.tracks[0].clips.push(Clip { start: 40, length: 4, label: String::new(), notes: Vec::new(), file: None });
            audio.sync(&project, None, &[], &[]);
            relanding = None;
        }
        let until = relanding.unwrap_or(frames);
        let n = BLOCK.min(frames - out.len()).min(until - out.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    out
}

#[test]
fn a_vision_landing_mid_echo_keeps_its_level() {
    let expected = render(song(), None);
    let at = (SAMPLE_RATE * 1.75) as usize;
    let heard = render(song(), Some(at));
    let (from, to) = ((SAMPLE_RATE * 1.6) as usize, (SAMPLE_RATE * 2.0) as usize);
    let peak = expected[from..to].iter().fold(0.0f32, |peak, x| peak.max(x.abs()));
    assert!(peak > AUDIBLE, "the echo before the note should be audible at frames {from}..{to}, peak {peak}");
    let (frame, delta) = (from..to).map(|frame| (frame, (heard[frame] - expected[frame]).abs())).max_by(|a, b| a.1.total_cmp(&b.1)).expect("a listening window");
    assert!(delta < TOLERANCE, "a vision landing at frame {at} must carry on at its level; frame {frame}: wanted {}, got {}, delta {delta}", expected[frame], heard[frame]);
}
