use bp::arp::{Arp, Articulation};
use bp::core::Params as CoreParams;
use bp::core::params::Envelope;
use bp::fx::params::{Delay, DelayMode};
use bp::fx::{Kind, Params, Slot};
use bp::project::{Clip, Note, Project, Row, VELOCITY};
use support::{AUDIBLE, NO_END, SILENT, channels_apart, core_layer, frames, midi, peak_of, song_render};

const PITCH: u8 = 60;
const LEVEL: f64 = 0.5;
const PLUCK_BEATS: f32 = 0.125;
const ECHO_MS: f64 = 500.0;
const BEFORE_THE_ECHO: f64 = 0.45;
const FOLLOW: f64 = 1.0;
const JUST_ABOVE_ZERO: f64 = 0.001;

fn pluck() -> Row {
    let mut patch = CoreParams::DEFAULT;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 60.0, sustain: 0.0, release_ms: 20.0, ..Envelope::DEFAULT };
    Row::Core(core_layer(LEVEL, patch))
}

fn delay(forward: f64, follow_dry: f64) -> Row {
    let params = Delay { mode: DelayMode::Clean, time_l_ms: ECHO_MS, time_r_ms: ECHO_MS, feedback: 0.0, forward, follow_dry, ..Delay::DEFAULT };
    Row::Effect(Slot { wet: 1.0, params: Params::Delay(params), ..Slot::new(Kind::Delay) })
}

fn heard(delay: Row) -> [Vec<f32>; 2] {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), pluck(), delay];
    let hit = Note { pitch: PITCH, start: 0.0, length: PLUCK_BEATS, velocity: VELOCITY, articulation: Articulation::PLAIN };
    project.tracks[0].clips = vec![Clip::midi(0, String::from("hit"), vec![hit], NO_END)];
    song_render(&project, frames(BEFORE_THE_ECHO))
}

fn loudest(heard: &[Vec<f32>; 2]) -> f32 {
    peak_of(&heard[0]).max(peak_of(&heard[1]))
}

#[test]
fn a_fully_wet_delay_at_forward_zero_lets_the_dry_follow_as_it_does_just_above_zero() {
    let resting = heard(delay(0.0, 0.0));
    assert!(loudest(&resting) < SILENT, "wanted a fully wet delay with FORWARD and FOLLOW DRY at 0 silent before its first echo, got a peak of {}", loudest(&resting));
    let at_zero = heard(delay(0.0, FOLLOW));
    assert!(loudest(&at_zero) > AUDIBLE, "wanted FOLLOW DRY to let the hit through a fully wet delay at FORWARD 0, got a peak of {}", loudest(&at_zero));
    let above = heard(delay(JUST_ABOVE_ZERO, FOLLOW));
    assert_eq!(channels_apart(&at_zero, &above), [None, None], "wanted FORWARD 0 to play the same dry as FORWARD {JUST_ABOVE_ZERO} before the first echo, got the left and right channels apart from these frames");
}
