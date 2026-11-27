use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params as CoreParams;
use bp::core::params::Envelope;
use bp::fx::params::{Delay, DelayMode, Reverb};
use bp::fx::{Kind, Params, Slot};
use bp::project::{Aim, Clip, Curve, LaneLfo, LfoField, Node, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{BLOCK, CORE_ROW, SAMPLE_RATE, core_layer, midi};

const KINDS: [Kind; 2] = [Kind::Delay, Kind::Reverb];
const SECONDS: usize = 4;
const TOLERANCE: f32 = 1e-5;
const AUDIBLE: f32 = 1e-4;
const SETTLED: usize = 24_000;
const EFFECT_ROW: usize = CORE_ROW + 1;
type Stereo = [f32; 2];

fn effect(kind: Kind, forward: f64, reverse: f64, wet: f64) -> Row {
    let params = match kind {
        Kind::Delay => Params::Delay(Delay { mode: DelayMode::Clean, time_l_ms: 500.0, time_r_ms: 500.0, feedback: 0.0, forward, reverse, ..Delay::DEFAULT }),
        Kind::Reverb => Params::Reverb(Reverb { decay_s: 0.6, mod_depth: 0.0, forward, reverse, ..Reverb::DEFAULT }),
        _ => unreachable!(),
    };
    Row::Effect(Slot { wet, params, ..Slot::new(kind) })
}

fn song(kind: Kind, forward: f64, reverse: f64, wet: f64) -> Project {
    let mut patch = CoreParams::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(0.1, patch)), effect(kind, forward, reverse, wet)];
    project.tracks[0].clips = vec![Clip {
        start: 0,
        length: 8 * TICKS_PER_BEAT,
        label: String::new(),
        notes: vec![Note { pitch: 69, start: 4.0, length: 2.0, velocity: VELOCITY, articulation: Articulation::PLAIN }],
        file: None,
    }];
    project
}

fn band(kind: Kind) -> &'static str {
    match kind {
        Kind::Delay => "TIME",
        Kind::Reverb => "SPACE",
        _ => unreachable!(),
    }
}

fn curve(kind: Kind, param: &str, levels: &[(i32, f32)]) -> Curve {
    Curve {
        aim: Aim::knob(EFFECT_ROW, band(kind), param),
        lfo: None,
        nodes: levels.iter().map(|&(beat, travel)| Node { tick: beat * TICKS_PER_BEAT, travel, bend: 0.0 }).collect(),
    }
}

fn render(mut project: Project, changes: &[(usize, Row)]) -> Vec<Stereo> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(&project, None, &[], &[]);
    audio.play(0.0);
    let frames = SECONDS * SAMPLE_RATE as usize;
    let mut changes = changes.iter().peekable();
    let (mut left, mut right) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(frames);
    while out.len() < frames {
        while let Some(&&(at, row)) = changes.peek() {
            if at > out.len() {
                break;
            }
            project.tracks[0].rack[EFFECT_ROW] = row;
            audio.sync(&project, None, &[], &[]);
            changes.next();
        }
        let until_change = changes.peek().map_or(frames, |(at, _)| *at);
        let n = BLOCK.min(frames - out.len()).min(until_change - out.len());
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
fn the_two_levels_scale_the_forward_and_reverse_audio_independently() {
    for kind in KINDS {
        let forward = render(song(kind, 1.0, 0.0, 1.0), &[]);
        let reverse = render(song(kind, 0.0, 1.0, 1.0), &[]);
        assert!(peak(&forward) > AUDIBLE, "{kind:?}: the forward reference should be audible");
        assert!(peak(&reverse) > AUDIBLE, "{kind:?}: the reverse reference should be audible");
        for (fwd, rev) in [(0.25, 0.0), (0.0, 0.6), (0.35, 0.65), (1.0, 1.0)] {
            let heard = render(song(kind, fwd, rev, 1.0), &[]);
            let expected: Vec<Stereo> = forward.iter().zip(&reverse).map(|(f, r)| std::array::from_fn(|channel| f[channel] * fwd as f32 + r[channel] * rev as f32)).collect();
            same(kind, &heard, &expected, SETTLED, heard.len(), &format!("FWD {fwd} and REV {rev} should add their independent signals"));
        }
    }
}

#[test]
fn wet_dry_adds_the_dry_signal_once_when_both_levels_are_up() {
    for kind in KINDS {
        let mut bare = song(kind, 1.0, 0.0, 1.0);
        bare.tracks[0].rack.pop();
        let dry = render(bare, &[]);
        let wet = render(song(kind, 0.35, 0.65, 1.0), &[]);
        let heard = render(song(kind, 0.35, 0.65, 0.4), &[]);
        let expected: Vec<Stereo> = dry.iter().zip(&wet).map(|(d, w)| std::array::from_fn(|channel| 0.6 * d[channel] + 0.4 * w[channel])).collect();
        same(kind, &heard, &expected, SETTLED, heard.len(), "40% wet should leave exactly 60% dry");
        let dry_only = render(song(kind, 1.0, 1.0, 0.0), &[]);
        same(kind, &dry_only, &dry, SETTLED, dry.len(), "zero wet should be the unaffected source");
    }
}

#[test]
fn zero_levels_leave_only_the_dry_share() {
    for kind in KINDS {
        let mut bare = song(kind, 0.0, 0.0, 1.0);
        bare.tracks[0].rack.pop();
        let dry = render(bare, &[]);
        for wet in [0.0, 0.4, 1.0] {
            let heard = render(song(kind, 0.0, 0.0, wet), &[]);
            let expected: Vec<Stereo> = dry.iter().map(|frame| frame.map(|sample| sample * (1.0 - wet) as f32)).collect();
            same(kind, &heard, &expected, SETTLED, heard.len(), "zero direction levels should silence only the effect signals");
        }
    }
}

#[test]
fn reverse_automation_is_ready_before_the_note_with_the_base_level_at_zero() {
    for kind in KINDS {
        let mut automated = song(kind, 0.0, 0.0, 1.0);
        automated.tracks[0].automation.curves.push(curve(kind, "REVERSE", &[(0, 0.0), (2, 0.0), (3, 0.6)]));
        let heard = render(automated, &[]);
        let expected = render(song(kind, 0.0, 0.6, 1.0), &[]);
        let (from, to) = ((SAMPLE_RATE * 1.7) as usize, (SAMPLE_RATE * 1.9) as usize);
        assert!(peak(&expected[from..to]) > AUDIBLE, "{kind:?}: wanted an audible reverse lead-in at frames {from}..{to}");
        same(kind, &heard, &expected, from, to, "the automated reverse should already be audible before the source note");
    }
}

#[test]
fn forward_automation_does_not_change_the_reverse_level() {
    for kind in KINDS {
        let mut automated = song(kind, 1.0, 0.65, 1.0);
        automated.tracks[0].automation.curves.push(curve(kind, "FORWARD", &[(0, 0.35)]));
        let heard = render(automated, &[]);
        let expected = render(song(kind, 0.35, 0.65, 1.0), &[]);
        same(kind, &heard, &expected, SETTLED, heard.len(), "forward automation should leave reverse at 65%");
    }
}

#[test]
fn an_active_reverse_lfo_prepares_audio_with_the_base_level_at_zero() {
    for kind in KINDS {
        let mut modulated = song(kind, 0.0, 0.0, 1.0);
        modulated.tracks[0].automation.lfos.push(LaneLfo { amount: 1.0, ..LaneLfo::blank(Aim::knob(EFFECT_ROW, band(kind), "REVERSE")) });
        let heard = render(modulated, &[]);
        let (from, to) = ((SAMPLE_RATE * 1.7) as usize, (SAMPLE_RATE * 1.9) as usize);
        let peak = peak(&heard[from..to]);
        assert!(peak > AUDIBLE, "{kind:?}: reverse LFO should be audible at frames {from}..{to} before the note, peak {peak}");
    }
}

#[test]
fn a_reverse_lfo_curve_prepares_audio_without_a_stored_lfo() {
    for kind in KINDS {
        let mut automated = song(kind, 0.0, 0.0, 1.0);
        let mut centre = curve(kind, "REVERSE", &[(0, 0.6)]);
        centre.lfo = Some(LfoField::Center);
        automated.tracks[0].automation.curves.push(centre);
        let heard = render(automated, &[]);
        let expected = render(song(kind, 0.0, 0.6, 1.0), &[]);
        same(kind, &heard, &expected, SETTLED, heard.len(), "an LFO centre curve should wake reverse at 60%");
    }
}

#[test]
fn raising_reverse_during_playback_uses_the_new_level() {
    for kind in KINDS {
        let changes = [
            (SAMPLE_RATE as usize, effect(kind, 0.0, 0.25, 1.0)),
            ((SAMPLE_RATE * 1.4) as usize, effect(kind, 0.0, 0.6, 1.0)),
        ];
        let heard = render(song(kind, 0.0, 0.0, 1.0), &changes);
        let expected = render(song(kind, 0.0, 0.6, 1.0), &[]);
        same(kind, &heard, &expected, (SAMPLE_RATE * 1.7) as usize, heard.len(), "raising REV from zero and then to 60% should produce the same reverse audio");
    }
}
