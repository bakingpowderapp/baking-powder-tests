use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params as CoreParams;
use bp::core::params::Envelope;
use bp::fx::params::{Reverb, Tank};
use bp::fx::{Kind, Params, Slot};
use bp::project::{Aim, Clip, Curve, Node, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{iter, panic, thread};
use support::{BLOCK, CORE_ROW, SAMPLE_RATE, beat_frame, core_layer, midi};

const PITCH: u8 = 60;
const PLUCK_BEATS: f32 = 0.125;
const LEVEL: f64 = 0.5;
const ROW: usize = CORE_ROW + 1;
const HELD_BEATS: usize = 8;
const ARRIVED_BEATS: usize = 9;
const LANE_BEATS: usize = 10;
const SWITCH_BEATS: usize = 2;
const FOLLOWED_BEATS: usize = 6;
const SYNC_BEATS: usize = 7;
const PLAIN_DB: f64 = -20.0;
const FOLLOWED_DB: f64 = -40.0;

struct Lane {
    band: &'static str,
    name: &'static str,
    moved: fn(&mut Reverb),
}

struct Change {
    what: &'static str,
    setup: fn(&mut Reverb),
    change: fn(&mut Reverb),
}

const PLATE_LANES: [Lane; 5] = [
    Lane { band: "SPACE", name: "DECAY", moved: |r| r.decay_s = 0.4 },
    Lane { band: "SPACE", name: "PRE-DELAY", moved: |r| r.pre_delay_ms = 200.0 },
    Lane { band: "SPACE", name: "SIZE", moved: |r| r.size = 0.3 },
    Lane { band: "SPACE", name: "WIDTH", moved: |r| r.width = 0.0 },
    Lane { band: "FILTER", name: "DAMPING", moved: |r| r.damping_hz = 1200.0 },
];

const HALL_LANES: [Lane; 3] = [
    Lane { band: "SPACE", name: "DECAY", moved: |r| r.decay_s = 0.4 },
    Lane { band: "SPACE", name: "SIZE", moved: |r| r.size = 0.3 },
    Lane { band: "FILTER", name: "DAMPING", moved: |r| r.damping_hz = 1200.0 },
];

const DECAYS: [Change; 5] = [
    Change { what: "the decay", setup: |_| {}, change: |r| r.decay_s = 0.5 },
    Change { what: "the low decay", setup: |_| {}, change: |r| r.low_decay = 0.25 },
    Change { what: "the high decay", setup: |r| r.high_crossover_hz = 1500.0, change: |r| r.high_decay = 0.25 },
    Change { what: "the low crossover", setup: |r| r.low_decay = 0.25, change: |r| r.low_crossover_hz = 1500.0 },
    Change { what: "the high crossover", setup: |r| r.high_decay = 0.25, change: |r| r.high_crossover_hz = 1000.0 },
];

const TAILS: [Change; 4] = [
    Change { what: "the diffusion", setup: |_| {}, change: |r| r.diffusion = 0.0 },
    Change { what: "the damping", setup: |_| {}, change: |r| r.damping_hz = 1200.0 },
    Change { what: "the dispersion", setup: |_| {}, change: |r| r.disperse = 1.0 },
    Change { what: "the tail fall", setup: |r| (r.fade_after_s, r.fade_db_per_s) = (0.1, Reverb::MIN_FADE_DB_PER_S), change: |r| r.fade_db_per_s = Reverb::MAX_FADE_DB_PER_S },
];

const TONES: [Change; 3] = [
    Change { what: "the low cut", setup: |_| {}, change: |r| r.filter1.hz = 900.0 },
    Change { what: "the high cut", setup: |_| {}, change: |r| r.filter2.hz = 2500.0 },
    Change { what: "a tap's side", setup: |r| r.tap1_level = 1.0, change: |r| r.tap1_side = 1.0 },
];

const DUCKS: [Change; 2] = [
    Change { what: "the duck threshold", setup: |r| r.ducking = 1.0, change: |r| r.threshold_db = 0.0 },
    Change { what: "the duck release", setup: |r| r.ducking = 1.0, change: |r| r.duck_release_ms = 2000.0 },
];

fn plate() -> Reverb {
    Reverb::DEFAULT
}

fn hall() -> Reverb {
    Reverb { tank: Tank::Hall, ..Reverb::DEFAULT }
}

fn song(reverb: Reverb, curves: Vec<Curve>, beats: usize) -> Project {
    let mut patch = CoreParams::DEFAULT;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 120.0, sustain: 0.0, release_ms: 40.0, ..Envelope::DEFAULT };
    let notes = (0..beats).map(|beat| Note { pitch: PITCH, start: beat as f32, length: PLUCK_BEATS, velocity: VELOCITY, articulation: Articulation::PLAIN }).collect();
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(LEVEL, patch)), Row::Effect(Slot { params: Params::Reverb(reverb), ..Slot::new(Kind::Reverb) })];
    track.clips.push(Clip { start: 0, length: beats as i32 * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    track.automation.curves = curves;
    project
}

fn lane(band: &str, name: &str, nodes: &[(usize, f32)]) -> Curve {
    let nodes = nodes.iter().map(|&(beat, travel)| Node { tick: beat as i32 * TICKS_PER_BEAT, travel, bend: 0.0 }).collect();
    Curve { aim: Aim::knob(ROW, band, name), lfo: None, nodes }
}

fn travel(reverb: Reverb, band: &str, name: &str) -> f32 {
    Params::Reverb(reverb).travel_of(band, name).unwrap_or_else(|| panic!("wanted {band} {name} automatable")) as f32
}

fn render(first: &Project, then: Option<&Project>, beats: usize) -> Vec<[f32; 2]> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(first, None, &[], &[]);
    audio.play(0.0);
    let (frames, switch) = (beat_frame(beats), beat_frame(SWITCH_BEATS));
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(frames);
    while out.len() < frames {
        if out.len() == switch
            && let Some(then) = then
        {
            audio.sync(then, None, &[], &[]);
        }
        let until = if out.len() < switch { switch } else { frames };
        let n = BLOCK.min(until - out.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend(l[..n].iter().zip(&r[..n]).map(|(&l, &r)| [l, r]));
    }
    out
}

fn parting(a: &[[f32; 2]], b: &[[f32; 2]]) -> Option<usize> {
    a.iter().zip(b).position(|(x, y)| x.map(f32::to_bits) != y.map(f32::to_bits))
}

fn rms(frames: impl Iterator<Item = [f32; 2]>) -> f64 {
    let (sum, count) = frames.flatten().fold((0.0, 0), |(sum, count), sample| (sum + f64::from(sample) * f64::from(sample), count + 1));
    (sum / f64::from(count.max(1))).sqrt()
}

fn gap(a: &[[f32; 2]], b: &[[f32; 2]]) -> f64 {
    rms(a.iter().zip(b).map(|(x, y)| [x[0] - y[0], x[1] - y[1]]))
}

fn db(over: f64, under: f64) -> f64 {
    20.0 * (over / under).log10()
}

fn side_by_side<T: Sync, R: Send>(jobs: &[T], work: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let workers = thread::available_parallelism().map_or(1, NonZeroUsize::get).min(jobs.len());
    let mut done: Vec<(usize, R)> = thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    iter::from_fn(|| {
                        let at = next.fetch_add(1, Ordering::Relaxed);
                        jobs.get(at).map(|job| (at, work(job)))
                    })
                    .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|handle| handle.join().unwrap_or_else(|panic| panic::resume_unwind(panic))).collect()
    });
    done.sort_unstable_by_key(|&(at, _)| at);
    done.into_iter().map(|(_, result)| result).collect()
}

fn heard(base: Reverb, lanes: &[Lane]) {
    let (moves, heard) = (beat_frame(HELD_BEATS), beat_frame(ARRIVED_BEATS));
    let tank = base.tank.name();
    let songs: Vec<[Project; 2]> = lanes
        .iter()
        .map(|Lane { band, name, moved }| {
            let mut swept = base;
            moved(&mut swept);
            let (from, to) = (travel(base, band, name), travel(swept, band, name));
            [song(base, vec![lane(band, name, &[(0, from)])], LANE_BEATS), song(base, vec![lane(band, name, &[(0, from), (HELD_BEATS, from), (ARRIVED_BEATS, to)])], LANE_BEATS)]
        })
        .collect();
    let renders = side_by_side(songs.as_flattened(), |project| render(project, None, LANE_BEATS));
    for (Lane { band, name, .. }, [held, late]) in lanes.iter().zip(renders.as_chunks::<2>().0) {
        let parts = parting(held, late);
        assert!(parts.is_some_and(|frame| frame >= moves), "{band} {name} on the {tank}: wanted the held and late lanes alike up to frame {moves}, where the late one starts to move, and apart after it; they part at frame {parts:?}");
        let apart = db(gap(&held[heard..], &late[heard..]), rms(held[heard..].iter().copied()));
        assert!(apart > PLAIN_DB, "{band} {name} on the {tank}: wanted the moved knob plainly heard from frame {heard}, the renders differ by {apart:.1} dB of the held one, wanted over {PLAIN_DB} dB");
    }
}

fn followed(base: Reverb, changes: &[Change]) {
    let (switch, heard) = (beat_frame(SWITCH_BEATS), beat_frame(FOLLOWED_BEATS));
    let tank = base.tank.name();
    let mut runs: Vec<(Reverb, Option<Reverb>)> = Vec::new();
    let picks: Vec<[usize; 3]> = changes
        .iter()
        .map(|Change { setup, change, .. }| {
            let mut before = base;
            setup(&mut before);
            let mut after = before;
            change(&mut after);
            [(before, None), (before, Some(after)), (after, None)].map(|run| {
                runs.iter().position(|&listed| listed == run).unwrap_or_else(|| {
                    runs.push(run);
                    runs.len() - 1
                })
            })
        })
        .collect();
    let renders = side_by_side(&runs, |&(first, then)| render(&song(first, Vec::new(), SYNC_BEATS), then.map(|then| song(then, Vec::new(), SYNC_BEATS)).as_ref(), SYNC_BEATS));
    for (Change { what, .. }, [kept, switched, fresh]) in changes.iter().zip(picks) {
        let (kept, switched, fresh) = (&renders[kept], &renders[switched], &renders[fresh]);
        let parts = parting(kept, switched);
        assert!(parts.is_some_and(|frame| frame >= switch), "{what} on the {tank}: wanted a change synced at frame {switch} to leave every frame before it alone and to be heard after it; the renders part at frame {parts:?}");
        let (kept, switched, fresh) = (&kept[heard..], &switched[heard..], &fresh[heard..]);
        let change_db = db(gap(kept, fresh), rms(fresh.iter().copied()));
        assert!(change_db > PLAIN_DB, "{what} on the {tank}: wanted the change plainly audible, the old and new settings differ by {change_db:.1} dB of the new one from frame {heard}, wanted over {PLAIN_DB} dB");
        let left_db = db(gap(switched, fresh), gap(kept, fresh));
        assert!(left_db < FOLLOWED_DB, "{what} on the {tank}: wanted the audio from frame {heard} to match a render with the new setting from the start, it differs by {left_db:.1} dB of the old setting's difference, wanted under {FOLLOWED_DB} dB");
    }
}

#[test]
fn a_plate_lane_that_moves_after_four_settled_seconds_is_heard() {
    heard(plate(), &PLATE_LANES);
}

#[test]
fn a_hall_lane_that_moves_after_four_settled_seconds_is_heard() {
    heard(hall(), &HALL_LANES);
}

#[test]
fn a_decay_synced_after_the_plate_settles_is_followed() {
    followed(plate(), &DECAYS);
}

#[test]
fn a_decay_synced_after_the_hall_settles_is_followed() {
    followed(hall(), &DECAYS);
}

#[test]
fn a_tail_setting_synced_after_the_plate_settles_is_followed() {
    followed(plate(), &TAILS);
}

#[test]
fn a_tail_setting_synced_after_the_hall_settles_is_followed() {
    followed(hall(), &TAILS);
}

#[test]
fn a_tone_synced_after_the_plate_settles_is_followed() {
    followed(plate(), &TONES);
}

#[test]
fn a_duck_setting_synced_after_the_plate_settles_is_followed() {
    followed(plate(), &DUCKS);
}
