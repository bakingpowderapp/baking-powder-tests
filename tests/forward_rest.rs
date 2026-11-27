use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params as CoreParams;
use bp::core::params::Envelope;
use bp::fx::params::{Delay, DelayMode, Reverb};
use bp::fx::{Kind, Params, Slot};
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{BLOCK, CORE_ROW, SAMPLE_RATE, core_layer, midi};

const KINDS: [Kind; 2] = [Kind::Delay, Kind::Reverb];
const SECONDS: f64 = 4.0;
const AUDIBLE: f32 = 1e-4;
const TOLERANCE: f32 = 1e-5;
const SETTLED: usize = 24_000;
const EFFECT_ROW: usize = CORE_ROW + 1;
type Stereo = [f32; 2];

fn effect(kind: Kind, forward: f64) -> Row {
    let params = match kind {
        Kind::Delay => Params::Delay(Delay { mode: DelayMode::Clean, time_l_ms: 500.0, time_r_ms: 500.0, feedback: 0.5, forward, reverse: 0.0, ..Delay::DEFAULT }),
        Kind::Reverb => Params::Reverb(Reverb { decay_s: 2.0, mod_depth: 0.0, forward, reverse: 0.0, ..Reverb::DEFAULT }),
        _ => unreachable!(),
    };
    Row::Effect(Slot { params, ..Slot::new(kind) })
}

fn song(kind: Kind, forward: f64, length: f32) -> Project {
    let mut patch = CoreParams::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    let mut project = Project::new();
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(0.1, patch)), effect(kind, forward)];
    project.tracks[0].clips = vec![Clip { start: 0, length: 8 * TICKS_PER_BEAT, label: String::new(), notes: vec![Note { pitch: 69, start: 4.0, length, velocity: VELOCITY, articulation: Articulation::PLAIN }], file: None }];
    project
}

fn render(mut project: Project, change: Option<(usize, Row)>) -> Vec<Stereo> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(&project, None, &[], &[]);
    audio.play(0.0);
    let frames = (SECONDS * SAMPLE_RATE) as usize;
    let (mut left, mut right) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(frames);
    let mut change = change;
    while out.len() < frames {
        if let Some((at, row)) = change
            && at <= out.len()
        {
            project.tracks[0].rack[EFFECT_ROW] = row;
            audio.sync(&project, None, &[], &[]);
            change = None;
        }
        let until = change.map_or(frames, |(at, _)| at);
        let n = BLOCK.min(frames - out.len()).min(until - out.len());
        renderer.render(&mut left[..n], &mut right[..n]);
        out.extend(left[..n].iter().zip(&right[..n]).map(|(&l, &r)| [l, r]));
    }
    out
}

fn peak(samples: &[Stereo]) -> f32 {
    samples.iter().flatten().fold(0.0f32, |peak, sample| peak.max(sample.abs()))
}

fn same(kind: Kind, actual: &[Stereo], expected: &[Stereo], from: usize, to: usize, reason: &str) {
    let (frame, channel, delta) = (from..to).flat_map(|frame| (0..2).map(move |channel| (frame, channel, (actual[frame][channel] - expected[frame][channel]).abs())))
        .max_by(|a, b| a.2.total_cmp(&b.2)).expect("a nonempty listening window");
    assert!(delta < TOLERANCE, "{kind:?}: {reason}; frame {frame}, channel {channel}: wanted {}, got {}, delta {delta}", expected[frame][channel], actual[frame][channel]);
}

#[test]
fn raising_forward_after_the_note_reveals_the_tail_still_decaying() {
    for kind in KINDS {
        let at = (SAMPLE_RATE * 2.5) as usize;
        let ringing = render(song(kind, 1.0, 0.5), None);
        let woken = render(song(kind, 0.0, 0.5), Some((at, effect(kind, 1.0))));
        let (from, to) = (at + SETTLED, ringing.len());
        let tail = peak(&ringing[from..to]);
        assert!(tail > AUDIBLE, "{kind:?}: with forward up all along the note's tail should ring at frames {from}..{to}, peak {tail}");
        same(kind, &woken, &ringing, from, to, "forward raised after the note should reveal the tail still decaying, as with forward up all along");
    }
}

#[test]
fn forward_rising_with_the_note_settles_into_the_tail_of_an_effect_that_never_rested() {
    for kind in KINDS {
        let at = (SAMPLE_RATE * 2.0) as usize;
        let awake = render(song(kind, 1.0, 2.0), None);
        let woken = render(song(kind, 0.0, 2.0), Some((at, effect(kind, 1.0))));
        let (from, to) = (at + SETTLED, awake.len());
        let tail = peak(&awake[from..to]);
        assert!(tail > AUDIBLE, "{kind:?}: the tail should be audible at frames {from}..{to}, peak {tail}");
        same(kind, &woken, &awake, from, to, "forward raised as the note starts should settle into the same tail as forward up all along");
    }
}
