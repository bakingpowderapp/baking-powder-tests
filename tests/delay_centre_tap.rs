use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params as CoreParams;
use bp::core::params::Envelope;
use bp::fx::params::{Delay, DelayMode};
use bp::fx::{Kind, Params, Slot};
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{BLOCK, Onsets, SAMPLE_RATE, core_layer, first_difference, frames, midi};

const LEFT_MS: f64 = 300.0;
const RIGHT_MS: f64 = 400.0;
const CENTRE_MS: f64 = 150.0;
const NOTE_BEATS: f32 = 0.1;
const SECONDS: f64 = 0.8;
const WITHIN: usize = 2;

fn plain() -> Delay {
    Delay { mode: DelayMode::Clean, time_l_ms: LEFT_MS, time_r_ms: RIGHT_MS, feedback: 0.0, width: 1.0, ..Delay::DEFAULT }
}

fn row(params: Delay) -> Row {
    Row::Effect(Slot { wet: 0.5, params: Params::Delay(params), ..Slot::new(Kind::Delay) })
}

fn centred(level_c: f64, pan_c: f64) -> Row {
    row(Delay { time_c_ms: CENTRE_MS, level_c, pan_c, ..plain() })
}

fn project(delay: Row) -> Project {
    let mut patch = CoreParams::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    let notes = vec![Note { pitch: 69, start: 0.0, length: NOTE_BEATS, velocity: VELOCITY, articulation: Articulation::PLAIN }];
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch)), delay];
    project.tracks[0].clips = vec![Clip { start: 0, length: 4 * TICKS_PER_BEAT, label: String::new(), notes, file: None }];
    project
}

fn render(project: &Project) -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(project, None, &[], &[]);
    audio.play(0.0);
    let frames = frames(SECONDS);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let (mut left, mut right) = (Vec::new(), Vec::new());
    let mut rendered = 0;
    while rendered < frames {
        let n = BLOCK.min(frames - rendered);
        renderer.render(&mut l[..n], &mut r[..n]);
        left.push(l[..n].to_vec());
        right.push(r[..n].to_vec());
        rendered += n;
    }
    (left, right)
}

fn onsets(blocks: &[Vec<f32>]) -> Vec<usize> {
    let mut onsets = Onsets::default();
    for block in blocks {
        onsets.feed(block);
    }
    onsets.at
}

fn check(side: &str, got: &[usize], echoes_ms: &[f64]) {
    let first = got.first().copied().unwrap_or(0);
    let wanted: Vec<usize> = std::iter::once(first).chain(echoes_ms.iter().map(|ms| first + (ms * 0.001 * SAMPLE_RATE).round() as usize)).collect();
    let matched = got.len() == wanted.len() && got.iter().zip(&wanted).all(|(got, wanted)| got.abs_diff(*wanted) <= WITHIN);
    assert!(matched, "wanted the {side} to sound at frames {wanted:?}, got onsets at {got:?}");
}

#[test]
fn the_centre_tap_echoes_at_its_own_time_on_both_sides() {
    let (left, right) = render(&project(centred(1.0, 0.0)));
    check("left", &onsets(&left), &[CENTRE_MS, LEFT_MS]);
    check("right", &onsets(&right), &[CENTRE_MS, RIGHT_MS]);
}

#[test]
fn the_centre_tap_pans() {
    let (left, right) = render(&project(centred(1.0, -1.0)));
    check("left", &onsets(&left), &[CENTRE_MS, LEFT_MS]);
    check("right", &onsets(&right), &[RIGHT_MS]);
}

#[test]
fn an_unraised_centre_tap_leaves_the_row_as_before() {
    let (left, right) = render(&project(centred(0.0, -1.0)));
    let (plain_l, plain_r) = render(&project(row(plain())));
    assert_eq!(first_difference(&left, &plain_l), None, "wanted a centre tap at level 0 to leave the left as before");
    assert_eq!(first_difference(&right, &plain_r), None, "wanted a centre tap at level 0 to leave the right as before");
}
