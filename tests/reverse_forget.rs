use bp::arp::{Arp, Articulation};
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::core::Params as CoreParams;
use bp::core::params::Envelope;
use bp::fx::params::{Delay, DelayMode};
use bp::fx::{Kind, Params, Slot};
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use std::time::{Duration, Instant};
use support::{BLOCK, FRAMES_PER_BEAT, SAMPLE_RATE, core_layer, midi};

const HUSH: f32 = 1e-9;
const HEARD: f32 = 1e-6;
const SECONDS: f64 = 4.5;

fn delay(reverse: f64) -> Row {
    Row::Effect(Slot { params: Params::Delay(Delay { mode: DelayMode::Clean, time_l_ms: 500.0, time_r_ms: 500.0, feedback: 0.0, forward: 0.0, reverse, ..Delay::DEFAULT }), ..Slot::new(Kind::Delay) })
}

fn song(beat: f32, reverse: f64) -> Project {
    let mut patch = CoreParams::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    let mut project = Project::new();
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(0.1, patch)), delay(reverse)];
    project.tracks[0].clips = vec![Clip { start: 0, length: 12 * TICKS_PER_BEAT, label: String::new(), notes: vec![Note { pitch: 69, start: beat, length: 1.0, velocity: VELOCITY, articulation: Articulation::PLAIN }], file: None }];
    project
}

fn hear(audio: &mut Audio, renderer: &mut Renderer) -> Vec<f32> {
    audio.play(0.0);
    let frames = (SECONDS * SAMPLE_RATE) as usize;
    let mut out = Vec::with_capacity(frames);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    while out.len() < frames {
        let n = BLOCK.min(frames - out.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    audio.stop();
    out
}

fn lead_in(out: &[f32], beat: f32) -> f32 {
    let at = f64::from(beat) * FRAMES_PER_BEAT;
    let (from, to) = ((at - 0.4 * SAMPLE_RATE) as usize, (at - 0.1 * SAMPLE_RATE) as usize);
    out[from..to].iter().map(|x| x * x).sum()
}

fn landed(audio: &mut Audio, renderer: &mut Renderer, beat: f32) -> Vec<f32> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        audio.tick(0.01);
        let heard = hear(audio, renderer);
        if lead_in(&heard, beat) > HEARD {
            return heard;
        }
        assert!(Instant::now() < deadline, "the seer never delivered the echo before beat {beat}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_vision_is_forgotten_when_reverse_returns_to_zero() {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::spawn());
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(&song(4.0, 1.0), None, &[], &[]);
    landed(&mut audio, &mut renderer, 4.0);

    audio.sync(&song(4.0, 0.0), None, &[], &[]);
    audio.sync(&song(7.0, 0.0), None, &[], &[]);
    audio.sync(&song(7.0, 1.0), None, &[], &[]);
    let pending = hear(&mut audio, &mut renderer);
    let stale = lead_in(&pending, 4.0);
    assert!(stale < HUSH, "with the render for the moved note still pending, the old note's echo must stay silent before beat 4; got energy {stale}");
    let early = lead_in(&pending, 7.0);
    assert!(early < HUSH, "nothing has landed for the moved note yet; got energy {early}");

    let fresh = landed(&mut audio, &mut renderer, 7.0);
    let gone = lead_in(&fresh, 4.0);
    assert!(gone < HUSH, "the landed render has no echo for the old note; got energy {gone}");
}
