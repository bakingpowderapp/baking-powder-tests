use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::{FilterMode, Params};
use bp::fx::params::{Freak, FreakSlope, Lookahead, MotionMode};
use bp::fx::params::freak::Order;
use bp::fx::{self, Kind, Slot};
use bp::project::{Aim, Bus, Clip, Curve, LaneLfo, Node, Note, Project, Row, TICKS_PER_BAR, TICKS_PER_BEAT, Track, VELOCITY};
use bp::wav::Wave;
use std::sync::Arc;
use std::{panic, thread};
use support::{AUDIBLE, CORE_ROW, FRAMES_PER_BEAT, SAMPLE_RATE, core_layer, effect, half_wet, midi, peak_of, stage};

const LEAD: usize = 1;
const DRUMS: usize = 3;
const BASS: usize = 4;
const KEYS: usize = 5;
const BLOCKS: [usize; 2] = [256, 512];
const HELPERS: [usize; 3] = [1, 2, 3];
const MOST_HELPERS: usize = 3;
const BEATS: f64 = 3.0;
const LOOP: (f64, f64) = (0.5, 1.55);
const LOOP_BEATS: f64 = 3.0;
const LOOP_WRAPS: u16 = 2;
const HITS: &str = "hits";
const HIT_BEATS: i32 = 4;
const SEED: u32 = 0x9E37_79B9;
const WAKE_PERCENT: u32 = 20;
const BUSY_SHARE: usize = 4;
const FORGET_SECONDS: f32 = 1_000.0;
const DRIVE_DB: f64 = 6.0;
const BUS_LEVELS: [f64; 2] = [0.8, 0.9];
const TAPPED: u8 = 84;
const TAP_BLOCK: usize = 8;
const TAP_HELPERS: usize = 8;
const TAP_BEATS: f64 = 1.0;

fn note(pitch: u8, start: f32, length: f32) -> Note {
    Note { pitch, start, length, velocity: VELOCITY, articulation: Articulation::PLAIN }
}

fn notes(pitches: &[u8], start: f32, length: f32) -> Vec<Note> {
    pitches.iter().enumerate().map(|(at, &pitch)| note(pitch, start + 0.25 * at as f32, length)).collect()
}

fn clip(notes: Vec<Note>) -> Clip {
    Clip { start: 0, length: 2 * TICKS_PER_BAR, label: String::new(), notes, file: None }
}

fn wide(unison: usize, detune: f64) -> Params {
    let mut patch = Params::DEFAULT;
    patch.osc1.unison = unison;
    patch.osc1.detune = detune;
    patch.osc2.level = 0.5;
    patch.osc2.levels = [0.0, 1.0, 0.0, 0.0];
    patch.osc2.unison = 3;
    patch.osc2.detune = detune / 2.0;
    patch.filter.on = true;
    patch.filter.mode = FilterMode::Ladder;
    patch.filter.cutoff_hz = 1_800.0;
    patch.filter.resonance = 0.3;
    patch.filter.env_amount = 0.5;
    patch.amp.attack_ms = 3.0;
    patch.amp.release_ms = 120.0;
    patch
}

fn sine() -> Params {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.amp.release_ms = 40.0;
    patch
}

fn core(level: f64, patch: Params) -> Row {
    Row::Core(core_layer(level, patch))
}

fn keyed(kind: Kind, source: usize, pre: bool) -> Row {
    let mut slot = Slot::new(kind);
    if let fx::Params::Sidechain(p) = &mut slot.params {
        p.threshold_db = -40.0;
        p.ratio = 8.0;
        p.lookahead = Lookahead::Ms5;
    }
    if let fx::Params::Space(p) = &mut slot.params {
        p.depth_db = 9.0;
    }
    slot.wire(Some(source), pre);
    Row::Effect(slot)
}

fn linear_split() -> Row {
    let mut freak = Freak { slope: FreakSlope::Linear, oversample: false, ..Freak::DEFAULT };
    for band in &mut freak.bands {
        band.order = Order::Post;
    }
    Row::Effect(Slot { params: fx::Params::Freak(freak), ..Slot::new(Kind::Freak) })
}

fn distorting() -> Row {
    let mut freak = Freak { band_count: 1, drive_db: DRIVE_DB, ..Freak::DEFAULT };
    freak.bands[0].curve = Some(fx::params::Curve::Soft);
    Row::Effect(Slot { params: fx::Params::Freak(freak), ..Slot::new(Kind::Freak) })
}

fn ramp(aim: Aim, from: f32, to: f32) -> Curve {
    Curve { aim, lfo: None, nodes: vec![Node { tick: 0, travel: from, bend: 0.0 }, Node { tick: 3 * TICKS_PER_BEAT, travel: to, bend: 0.3 }, Node { tick: 5 * TICKS_PER_BEAT, travel: from, bend: 0.0 }] }
}

fn hits() -> Arc<Wave> {
    let frames = (f64::from(HIT_BEATS) * FRAMES_PER_BEAT) as usize;
    let mut state = SEED;
    let mut pcm = Vec::with_capacity(2 * frames);
    for frame in 0..frames {
        let since_hit = frame % (FRAMES_PER_BEAT as usize / 2);
        let decay = (-(since_hit as f64) / 1_800.0).exp();
        for _ in 0..2 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            let noise = f64::from(state as i32) / f64::from(i32::MAX);
            pcm.push((noise * decay * 20_000.0).round() as i16);
        }
    }
    Arc::new(Wave::sound(String::from(HITS), SAMPLE_RATE, 2, pcm))
}

fn project() -> Project {
    let mut group = Track::group();
    group.rack = vec![effect(Kind::Compressor), effect(Kind::Eq), effect(Kind::Utility)];
    group.automation.curves = vec![ramp(Aim::Gain, 0.75, 0.6)];
    let mut lead = Track::blank();
    lead.inside = true;
    lead.rack = vec![midi(Arp::DEFAULT), core(0.6, wide(7, 22.0)), Row::Effect(stage(MotionMode::Chorus)), half_wet(Kind::Delay), half_wet(Kind::Reverb), keyed(Kind::Sidechain, KEYS, false), core(0.4, wide(4, 12.0)), Row::Effect(stage(MotionMode::Phaser)), core(0.5, sine()), Row::Bus(Bus { levels: BUS_LEVELS, ..Bus::DEFAULT }), core(0.3, wide(2, 8.0)), Row::Effect(stage(MotionMode::Flanger)), half_wet(Kind::Delay)];
    lead.sends[1] = 0.3;
    lead.clips.push(clip(notes(&[60, 64, 67, 71], 0.0, 3.5)));
    lead.automation.curves = vec![ramp(Aim::knob(CORE_ROW, "FILTER", "CUTOFF"), 0.3, 0.8), ramp(Aim::Level(8), 0.9, 0.4)];
    let mut pad = Track::blank();
    pad.inside = true;
    pad.rack = vec![midi(Arp::DEFAULT), core(0.5, wide(5, 30.0)), distorting(), core(0.4, sine()), effect(Kind::Eq)];
    pad.clips.push(clip(notes(&[48, 55, 62], 1.0, 4.0)));
    pad.automation.curves = vec![ramp(Aim::Gain, 0.7, 0.5)];
    let mut drums = Track::audio();
    drums.rack = vec![effect(Kind::Eq), effect(Kind::Compressor), half_wet(Kind::Utility)];
    drums.sends[0] = 0.4;
    drums.clips.push(Clip::audio(0, HIT_BEATS * TICKS_PER_BEAT, String::new(), String::from(HITS)));
    let mut bass = Track::blank();
    let mut mono = wide(3, 6.0);
    mono.voices = 1;
    bass.rack = vec![midi(Arp::DEFAULT), core(0.6, mono), core(0.5, sine()), keyed(Kind::Sidechain, DRUMS, true), linear_split()];
    bass.sends[1] = 0.2;
    bass.clips.push(clip(vec![note(36, 0.0, 1.5), note(43, 1.5, 1.0), note(41, 2.5, 2.0), note(36, 4.5, 1.0)]));
    bass.automation.lfos = vec![LaneLfo { amount: 0.5, ..LaneLfo::blank(Aim::knob(CORE_ROW, "FILTER", "CUTOFF")) }];
    let mut keys = Track::blank();
    keys.rack = vec![midi(Arp::DEFAULT), core(0.5, wide(6, 15.0)), keyed(Kind::Space, DRUMS, false), effect(Kind::Reverb)];
    keys.sends[0] = 0.2;
    keys.clips.push(clip(notes(&[72, 76, 79], 0.5, 2.0)));
    keys.automation.curves = vec![ramp(Aim::Wet(3), 0.2, 0.9)];
    let mut project = Project::new();
    project.tracks = vec![group, lead, pad, drums, bass, keys];
    project.sends[0].rack = vec![effect(Kind::Reverb)];
    project.sends[1].rack = vec![half_wet(Kind::Delay), keyed(Kind::Sidechain, LEAD, false)];
    project.master.rack = vec![keyed(Kind::Sidechain, BASS, false), effect(Kind::Utility)];
    project
}

struct Take {
    left: Vec<f32>,
    right: Vec<f32>,
    loads: Vec<u32>,
    wraps: u16,
}

fn render(project: &Project, block: usize, helpers: usize, range: Option<(f64, f64)>, beats: f64) -> Take {
    render_told(project, block, helpers, range, beats, |_, _| {})
}

fn tap(audio: &Audio, block: usize) {
    if block.is_multiple_of(2) {
        audio.note_on(LEAD, TAPPED, VELOCITY);
    } else {
        audio.note_off(LEAD, TAPPED);
    }
}

fn render_told(project: &Project, block: usize, helpers: usize, range: Option<(f64, f64)>, beats: f64, tell: impl Fn(&Audio, usize)) -> Take {
    let frames = (beats * FRAMES_PER_BEAT) as usize;
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, block, Seer::Inline);
    renderer.patient();
    renderer.start_workers(helpers, || None);
    audio.sync(project, None, &[], &[hits()]);
    audio.set_loop(range);
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; block], vec![0.0; block]);
    let (mut left, mut right) = (Vec::with_capacity(frames), Vec::with_capacity(frames));
    let mut loads = Vec::with_capacity(frames / block + 1);
    while left.len() < frames {
        let n = block.min(frames - left.len());
        tell(&audio, loads.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        left.extend_from_slice(&l[..n]);
        right.extend_from_slice(&r[..n]);
        audio.tick(FORGET_SECONDS);
        loads.push(audio.load());
    }
    Take { left, right, loads, wraps: audio.position().wraps }
}

fn at_once<T: Sync, R: Send>(jobs: &[T], work: impl Fn(&T) -> R + Sync) -> Vec<R> {
    thread::scope(|scope| {
        let handles: Vec<_> = jobs.iter().map(|job| scope.spawn(|| work(job))).collect();
        handles.into_iter().map(|handle| handle.join().unwrap_or_else(|panic| panic::resume_unwind(panic))).collect()
    })
}

fn expect_same(what: &str, alone: &Take, pooled: &Take) {
    for (side, wanted, got) in [("left", &alone.left, &pooled.left), ("right", &alone.right, &pooled.right)] {
        if let Some(frame) = wanted.iter().zip(got).position(|(a, b)| a.to_bits() != b.to_bits()) {
            panic!("{what}: wanted the {side} channel as the single-thread render plays it, first differs at frame {frame}: wanted {} got {}", wanted[frame], got[frame]);
        }
        assert_eq!(got.len(), wanted.len(), "{what}: wanted {} frames on the {side}, got {}", wanted.len(), got.len());
    }
}

#[test]
fn helpers_play_the_stress_project_as_one_thread_does() {
    let project = project();
    let alone = at_once(&BLOCKS, |&block| render(&project, block, 0, None, BEATS));
    for (block, alone) in BLOCKS.into_iter().zip(&alone) {
        assert!(peak_of(&alone.left) > AUDIBLE, "wanted the project heard at {block} frames, got a peak of {}", peak_of(&alone.left));
        for helpers in HELPERS {
            let pooled = render(&project, block, helpers, None, BEATS);
            expect_same(&format!("{helpers} helpers at {block} frames"), alone, &pooled);
        }
        let busy = alone.loads.iter().filter(|&&load| load > WAKE_PERCENT).count();
        assert!(busy * BUSY_SHARE >= alone.loads.len(), "wanted at least one block in {BUSY_SHARE} of the single-thread render at {block} frames over {WAKE_PERCENT}% load, so the helpers wake for it, got {busy} of {}: make the project heavier", alone.loads.len());
    }
}

#[test]
fn helpers_play_a_wrapping_loop_as_one_thread_does() {
    let project = project();
    let alone = at_once(&BLOCKS, |&block| render(&project, block, 0, Some(LOOP), LOOP_BEATS));
    for (block, alone) in BLOCKS.into_iter().zip(&alone) {
        assert_eq!(alone.wraps, LOOP_WRAPS, "wanted the loop from beat {} to {} to wrap {LOOP_WRAPS} times in {LOOP_BEATS} beats at {block} frames, got {}", LOOP.0, LOOP.1, alone.wraps);
        let wrap = (LOOP.1 * FRAMES_PER_BEAT) as usize;
        assert!(peak_of(&alone.left[wrap..]) > AUDIBLE, "wanted the loop heard after it wraps at frame {wrap}, got a peak of {}", peak_of(&alone.left[wrap..]));
        let pooled = render(&project, block, MOST_HELPERS, Some(LOOP), LOOP_BEATS);
        assert_eq!(pooled.wraps, LOOP_WRAPS, "wanted the pooled loop to wrap {LOOP_WRAPS} times at {block} frames, got {}", pooled.wraps);
        expect_same(&format!("a wrapping loop with {MOST_HELPERS} helpers at {block} frames"), alone, &pooled);
    }
}

#[test]
fn helpers_take_each_message_on_the_block_it_is_sent() {
    let project = project();
    let alone = at_once(&[true, false], |&tapped| render_told(&project, TAP_BLOCK, 0, None, TAP_BEATS, |audio, block| if tapped { tap(audio, block) }));
    let (tapped, plain) = (&alone[0], &alone[1]);
    assert!(tapped.left.iter().zip(&plain.left).any(|(tapped, plain)| tapped.to_bits() != plain.to_bits()), "wanted the key tapped on and off block by block heard over the song, so a message taken a block late would show");
    let pooled = render_told(&project, TAP_BLOCK, TAP_HELPERS, None, TAP_BEATS, tap);
    expect_same(&format!("a key tapped on or off before every block of {TAP_BLOCK} frames, {TAP_HELPERS} helpers"), tapped, &pooled);
}
