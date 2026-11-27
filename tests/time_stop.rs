use bp::arp::{Arp, Articulation};
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::core::Params as Patch;
use bp::core::params::Envelope;
use bp::fx::params::Time;
use bp::fx::{Kind, Params, Slot};
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{AUDIBLE, BLOCK, SAMPLE_RATE, core_layer, frames, peak_of, struck};

const A3: u8 = 57;
const HELD_BEATS: i32 = 24;
const STOP_MS: f64 = 9_000.0;
const PLAYING_SECONDS: f64 = 1.0;
const STOPPING_SECONDS: f64 = 8.5;
const STEADY_FROM_SECONDS: f64 = 0.5;
const STEP_ROOM: f32 = 2.0;

fn sine() -> Patch {
    let mut patch = Patch::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.phase_random = 0.0;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, velocity: 0.0, ..Envelope::DEFAULT };
    patch
}

fn project(play: bool) -> Project {
    let time = Slot { params: Params::Time(Time { play, stop_ms: STOP_MS, ..Time::DEFAULT }), ..Slot::new(Kind::Time) };
    let notes = vec![Note { pitch: A3, start: 0.0, length: HELD_BEATS as f32, velocity: VELOCITY, articulation: Articulation::PLAIN }];
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![struck(Arp::DEFAULT), Row::Core(core_layer(1.0, sine())), Row::Effect(time)];
    project.tracks[0].clips.push(Clip { start: 0, length: HELD_BEATS * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn heard(renderer: &mut Renderer, seconds: f64) -> Vec<f32> {
    let total = frames(seconds);
    let mut out = Vec::with_capacity(total);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    while out.len() < total {
        let n = BLOCK.min(total - out.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    out
}

fn steepest(out: &[f32]) -> f32 {
    out.windows(2).map(|pair| (pair[1] - pair[0]).abs()).fold(0.0f32, f32::max)
}

#[test]
fn a_long_stop_slows_the_tone_without_a_jump() {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(&project(true), None, &[], &[]);
    audio.play(0.0);
    let playing = heard(&mut renderer, PLAYING_SECONDS);
    audio.sync(&project(false), None, &[], &[]);
    let stopping = heard(&mut renderer, STOPPING_SECONDS);
    let steady = &playing[frames(STEADY_FROM_SECONDS)..];
    assert!(peak_of(steady) > AUDIBLE, "wanted the tone heard before the stop, got a peak of {}", peak_of(steady));
    let (at_speed, slowing) = (steepest(steady), steepest(&stopping));
    assert!(slowing < STEP_ROOM * at_speed, "wanted a {STOP_MS} ms stop to slow the tone smoothly, no step between samples over {STEP_ROOM} times the tone's own {at_speed} at full speed, got a step of {slowing}");
}
