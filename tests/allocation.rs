use bp::arp::{Arp, Articulation};
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::core::Params;
use bp::division::Division;
use bp::fx::params::{Freak, FreakSlope, MotionMode};
use bp::fx::params::freak::Order;
use bp::fx::{self, Kind, Slot};
use bp::project::{Aim, Bus, Clip, LaneLfo, Layer, LfoClock, LfoShape, Node, Note, Project, Row, TICKS_PER_BEAT, Track, VELOCITY};
use bp::wav::Wave;
use std::sync::Arc;
use support::heap::heap_calls;
use support::{BLOCK, CHORD, CORE_ROW, SAMPLE_RATE, core, core_layer, effect, midi, stage};

const WARM_UP: usize = 64;
const MEASURED: usize = 512;
const AFTER_EACH_EDIT: usize = 16;
const SPLICE: usize = 16;
const LOOP_BEATS: f64 = 4.0;
const WAVE_NAME: &str = "pulse.wav";
const WAVE_FRAMES: usize = 2 * 48_000;
const PULSE_EVERY: usize = 24_000;
const PULSE: usize = 2_400;
const PULSE_LEVEL: i16 = 8_192;
const SHORT_MS: f64 = 1.5;
const LONG_MS: f64 = 5.0;
const SHORT: usize = 71;
const LONG: usize = 239;
const LINEAR: usize = 431;

type Edit = fn(&mut Project, &mut Audio);

fn counted(renderer: &mut Renderer, blocks: usize, l: &mut [f32], r: &mut [f32]) -> usize {
    heap_calls(|| {
        for _ in 0..blocks {
            renderer.render(l, r);
        }
    })
}

fn counter_live() {
    let control = heap_calls(|| drop(std::hint::black_box(Vec::<u8>::with_capacity(1024))));
    assert!(control > 0, "the counter is not live");
}

fn limiter(lookahead_ms: f64) -> Row {
    let mut slot = Slot::new(Kind::Utility);
    if let fx::Params::Utility(utility) = &mut slot.params {
        utility.limit = true;
        utility.lookahead_ms = lookahead_ms;
    }
    Row::Effect(slot)
}

fn linear_squeeze() -> Row {
    let mut freak = Freak { slope: FreakSlope::Linear, oversample: false, ..Freak::DEFAULT };
    for band in &mut freak.bands {
        band.order = Order::Post;
    }
    Row::Effect(Slot { params: fx::Params::Freak(freak), ..Slot::new(Kind::Freak) })
}

fn pulses() -> Arc<Wave> {
    let pcm: Vec<i16> = (0..WAVE_FRAMES).map(|frame| if frame % PULSE_EVERY < PULSE { PULSE_LEVEL } else { 0 }).collect();
    Arc::new(Wave::sound(String::from(WAVE_NAME), SAMPLE_RATE, 1, pcm))
}

fn latent(lookahead_ms: f64) -> Project {
    let mut dry = Track::audio();
    dry.clips.push(Clip::audio(0, LOOP_BEATS as i32 * TICKS_PER_BEAT, String::from("pulse"), String::from(WAVE_NAME)));
    let mut limited = Track::blank();
    limited.rack = vec![midi(Arp::DEFAULT), core(1.0), limiter(lookahead_ms)];
    limited.clips.push(Clip::chord(0, String::new(), &CHORD));
    let mut split = Track::blank();
    split.rack = vec![midi(Arp::DEFAULT), core(1.0), linear_squeeze()];
    split.clips.push(Clip::chord(0, String::new(), &CHORD));
    let mut project = Project::new();
    project.tracks = vec![dry, limited, split];
    project
}

fn edits() -> Vec<(&'static str, Edit)> {
    vec![
        ("turning a Core knob", |project, _| {
            if let Row::Core(layer) = &mut project.tracks[0].rack[CORE_ROW] {
                layer.patch.filter.cutoff_hz = 900.0;
            }
        }),
        ("turning an effect knob", |project, _| project.tracks[0].rack[2] = Row::Effect(Slot { wet: 0.3, ..Slot::new(Kind::Delay) })),
        ("adding a row", |project, _| project.tracks[0].rack.push(effect(Kind::Reverb))),
        ("swapping a row's kind", |project, _| project.tracks[0].rack[2] = Row::Effect(stage(MotionMode::Chorus))),
        ("moving a row", |project, _| {
            let row = project.tracks[0].rack.remove(3);
            project.tracks[0].rack.insert(2, row);
        }),
        ("removing a row", |project, _| {
            project.tracks[0].rack.remove(2);
        }),
        ("adding a master row", |project, _| project.master.rack.push(effect(Kind::Compressor))),
        ("adding a send row and a send", |project, _| {
            project.sends[0].rack.push(effect(Kind::Reverb));
            project.tracks[0].sends[0] = 0.5;
        }),
        ("adding a core", |project, _| project.tracks[0].rack.push(Row::Core(Layer::DEFAULT))),
        ("muting and soloing", |project, _| {
            project.tracks[1].mute = true;
            project.tracks[0].solo = true;
        }),
        ("moving the faders", |project, _| {
            project.tracks[0].gain = 0.5;
            project.master.gain = 0.8;
        }),
        ("changing the tempo", |project, _| project.bpm = 97.0),
        ("placing a note", |project, _| project.tracks[0].clips[0].notes.push(Note { pitch: 72, start: 1.0, length: 0.5, velocity: VELOCITY, articulation: Articulation::PLAIN })),
        ("drawing automation", |project, _| project.tracks[0].automation.set_node(&Aim::knob(CORE_ROW, "FILTER", "CUTOFF"), None, Node { tick: 2 * TICKS_PER_BEAT, travel: 0.25, bend: 0.0 })),
        ("adding an LFO", |project, _| project.tracks[0].automation.lfos.push(LaneLfo { amount: 0.5, ..LaneLfo::blank(Aim::knob(CORE_ROW, "OSC 1", "LEVEL")) })),
        ("adding a return LFO", |project, _| project.sends[0].automation.lfos.push(LaneLfo { amount: 0.5, ..LaneLfo::blank(Aim::knob(0, "SPACE", "DECAY")) })),
        ("adding a track", |project, _| {
            project.add_track(2, Track::blank());
        }),
        ("removing a track", |project, _| {
            project.remove_track(2);
        }),
        ("turning the metronome on", |project, _| project.metronome = true),
        ("seeking", |_, audio| audio.play(2.0)),
        ("changing the loop", |_, audio| audio.set_loop(Some((1.0, 3.0)))),
        ("playing a key", |_, audio| audio.note_on(0, 55, VELOCITY)),
        ("lifting a key", |_, audio| audio.note_off(0, 55)),
        ("stopping", |_, audio| audio.stop()),
        ("playing again", |_, audio| audio.play(0.0)),
    ]
}

#[test]
fn the_render_path_never_touches_the_heap() {
    let mut project = Project::new();
    project.tracks[0].rack = vec![midi(Arp { on: true, rate: Division::Sixteenth, ..Arp::DEFAULT }), Row::Core(core_layer(1.0, Params::DEFAULT)), Row::Effect(Slot::new(Kind::Delay)), Row::Effect(Slot::new(Kind::Reverb)), Row::Core(Layer::DEFAULT), Row::Effect(stage(MotionMode::Chorus))];
    project.tracks[0].clips.push(Clip::chord(0, String::new(), &[60, 64, 67]));
    project.tracks[1].sends[0] = 0.5;
    project.sends[0].rack = vec![Row::Effect(Slot::new(Kind::Reverb))];
    project.master.rack = vec![Row::Effect(Slot::new(Kind::Eq)), Row::Effect(Slot::new(Kind::Compressor))];
    project.tracks[0].automation.lfos.push(LaneLfo { amount: 0.5, sync: false, rate_hz: 1.0, ..LaneLfo::blank(Aim::knob(CORE_ROW, "OSC 1", "LEVEL")) });
    project.tracks[0].automation.lfos.push(LaneLfo { amount: 0.5, sync: false, rate_hz: 2.0, ..LaneLfo::blank(Aim::knob(CORE_ROW, "OSC 1", "LEVEL")) });
    project.tracks[0].automation.lfos.push(LaneLfo { amount: 0.5, shape: LfoShape::Random, clock: LfoClock::Note, ..LaneLfo::blank(Aim::knob(CORE_ROW, "OSC 2", "LEVEL")) });
    project.metronome = true;
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(&project, None, &[], &[]);
    audio.set_loop(Some((0.0, 4.0)));
    audio.note_on(1, 48, 127);
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    for _ in 0..WARM_UP {
        renderer.render(&mut l, &mut r);
    }
    let during = counted(&mut renderer, MEASURED, &mut l, &mut r);
    counter_live();
    assert_eq!(during, 0, "the render path touched the heap {during} times in {MEASURED} blocks");
}

#[test]
fn no_kind_of_row_touches_the_heap_on_the_render_path() {
    let racks = Kind::ALL.into_iter().map(|kind| (kind.label(), vec![effect(kind)])).chain([("a bus row between two effects", vec![effect(Kind::Delay), Row::Bus(Bus::DEFAULT), effect(Kind::Reverb)])]);
    let mut offenders = Vec::new();
    let mut tried = 0;
    for (name, rows) in racks {
        let mut project = Project::new();
        project.tracks[0].rack = [midi(Arp::DEFAULT), core(1.0)].into_iter().chain(rows).collect();
        project.tracks[0].clips.push(Clip::chord(0, String::new(), &CHORD));
        let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
        audio.sync(&project, None, &[], &[]);
        audio.set_loop(Some((0.0, LOOP_BEATS)));
        audio.play(0.0);
        let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
        for _ in 0..WARM_UP {
            renderer.render(&mut l, &mut r);
        }
        audio.reclaim();
        let during = counted(&mut renderer, MEASURED, &mut l, &mut r);
        if during > 0 {
            offenders.push(format!("{name}: {during} heap calls in {MEASURED} blocks"));
        }
        tried += 1;
    }
    counter_live();
    assert!(tried > Kind::ALL.len() && offenders.is_empty(), "the render path touched the heap under {} of the {tried} racks tried, one for each kind of effect and one with a bus row:\n{}", offenders.len(), offenders.join("\n"));
}

#[test]
fn the_render_path_never_touches_the_heap_with_rings_at_rest_or_across_a_plan_swap() {
    let wave = pulses();
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(&latent(SHORT_MS), None, &[], std::slice::from_ref(&wave));
    assert_eq!((audio.timing().tracks[1].latency, audio.timing().tracks[2].latency, audio.timing().output), (SHORT, LINEAR, LINEAR), "wanted the limiter at {SHORT}, the linear-phase split at {LINEAR} and the output at {LINEAR}");
    audio.set_loop(Some((0.0, LOOP_BEATS)));
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    for _ in 0..WARM_UP {
        renderer.render(&mut l, &mut r);
    }
    audio.reclaim();
    let resting = counted(&mut renderer, MEASURED, &mut l, &mut r);
    counter_live();
    assert_eq!(resting, 0, "the render path touched the heap {resting} times in {MEASURED} blocks with the rings at rest");
    audio.reclaim();
    audio.sync(&latent(LONG_MS), None, &[], std::slice::from_ref(&wave));
    assert_eq!(audio.timing().tracks[1].latency, LONG, "wanted the limiter's look-ahead of {LONG_MS} ms planned at {LONG}");
    let swapping = counted(&mut renderer, SPLICE, &mut l, &mut r);
    audio.reclaim();
    assert_eq!(swapping, 0, "the render path touched the heap {swapping} times in the {SPLICE} blocks that installed the new plan and ran its splice");
    let settled = counted(&mut renderer, MEASURED, &mut l, &mut r);
    audio.reclaim();
    assert_eq!(settled, 0, "the render path touched the heap {settled} times in {MEASURED} blocks under the new plan");
}

#[test]
fn editing_the_project_as_it_plays_never_touches_the_heap_on_the_render_path() {
    let mut project = Project::new();
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(Layer::DEFAULT), effect(Kind::Delay)];
    project.tracks[0].clips.push(Clip::chord(0, String::new(), &[60, 64, 67]));
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(&project, None, &[], &[]);
    audio.set_loop(Some((0.0, 4.0)));
    audio.note_on(1, 48, VELOCITY);
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    for _ in 0..WARM_UP {
        renderer.render(&mut l, &mut r);
    }
    let edits = edits();
    let total = edits.len();
    let mut offenders = Vec::new();
    for (name, edit) in edits {
        edit(&mut project, &mut audio);
        audio.sync(&project, None, &[], &[]);
        let during = counted(&mut renderer, AFTER_EACH_EDIT, &mut l, &mut r);
        audio.reclaim();
        if during > 0 {
            offenders.push(format!("{name}: {during} heap calls in the {AFTER_EACH_EDIT} blocks after it"));
        }
    }
    counter_live();
    assert!(offenders.is_empty(), "the render path touched the heap after {} of {total} edits:\n{}", offenders.len(), offenders.join("\n"));
}
