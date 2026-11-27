use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params as CoreParams;
use bp::core::params::Envelope;
use bp::fx::params::Space;
use bp::fx::{Kind, Params, Slot};
use bp::project::{Aim, Clip, Curve, Node, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{iter, panic, thread};
use support::{BLOCK, CORE_ROW, SAMPLE_RATE, beat_frame, core_layer, midi};

const PITCH: u8 = 69;
const PLUCK_BEATS: f32 = 0.125;
const LEVEL: f64 = 0.5;
const KEY: usize = 0;
const CARVED: usize = 1;
const ROW: usize = CORE_ROW + 1;
const HELD_BEATS: usize = 8;
const ARRIVED_BEATS: usize = 9;
const LANE_BEATS: usize = 10;
const SWITCH_BEATS: usize = 2;
const FOLLOWED_BEATS: usize = 4;
const SYNC_BEATS: usize = 5;
const PLAIN_DB: f64 = -20.0;
const FOLLOWED_DB: f64 = -40.0;

struct Lane {
    band: &'static str,
    name: &'static str,
    moved: fn(&mut Space),
}

struct Change {
    what: &'static str,
    setup: fn(&mut Space),
    change: fn(&mut Space),
}

const LANES: [Lane; 3] = [
    Lane { band: "CUT", name: "DEPTH", moved: |s| s.depth_db = 30.0 },
    Lane { band: "RANGE", name: "HIGH", moved: |s| s.high_hz = 200.0 },
    Lane { band: "CUT", name: "RECOVERY", moved: |s| s.recovery_ms = 1000.0 },
];

const RANGES: [Change; 2] = [
    Change { what: "the key's high edge", setup: |s| s.high_hz = 200.0, change: |s| s.high_hz = 20_000.0 },
    Change { what: "the key's low edge", setup: |s| s.low_hz = 2000.0, change: |s| s.low_hz = 20.0 },
];

const CUTS: [Change; 4] = [
    Change { what: "a depth from nothing", setup: |s| s.depth_db = 0.0, change: |s| s.depth_db = 24.0 },
    Change { what: "a depth to nothing", setup: |_| {}, change: |s| s.depth_db = 0.0 },
    Change { what: "the Q", setup: |s| s.q = 1.0, change: |s| s.q = 16.0 },
    Change { what: "the recovery", setup: |_| {}, change: |s| s.recovery_ms = 1000.0 },
];

fn song(space: Space, curves: Vec<Curve>, beats: usize) -> Project {
    let mut sine = CoreParams::DEFAULT;
    sine.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    let mut pluck = CoreParams::DEFAULT;
    pluck.amp = Envelope { attack_ms: 1.0, decay_ms: 120.0, sustain: 0.0, release_ms: 40.0, ..Envelope::DEFAULT };
    let plucks = (0..beats).map(|beat| Note { pitch: PITCH, start: beat as f32, length: PLUCK_BEATS, velocity: VELOCITY, articulation: Articulation::PLAIN }).collect();
    let held = vec![Note { pitch: PITCH, start: 0.0, length: beats as f32, velocity: VELOCITY, articulation: Articulation::PLAIN }];
    let mut carve = Slot { params: Params::Space(space), ..Slot::new(Kind::Space) };
    carve.wire(Some(KEY), false);
    let length = beats as i32 * TICKS_PER_BEAT;
    let mut project = Project::new();
    project.tracks.truncate(2);
    project.tracks[KEY].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(LEVEL, pluck))];
    project.tracks[KEY].clips.push(Clip { start: 0, length, label: String::new(), notes: plucks, file: None });
    project.tracks[CARVED].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(LEVEL, sine)), Row::Effect(carve)];
    project.tracks[CARVED].clips.push(Clip { start: 0, length, label: String::new(), notes: held, file: None });
    project.tracks[CARVED].automation.curves = curves;
    project
}

fn lane(band: &str, name: &str, nodes: &[(usize, f32)]) -> Curve {
    let nodes = nodes.iter().map(|&(beat, travel)| Node { tick: beat as i32 * TICKS_PER_BEAT, travel, bend: 0.0 }).collect();
    Curve { aim: Aim::knob(ROW, band, name), lfo: None, nodes }
}

fn travel(space: Space, band: &str, name: &str) -> f32 {
    Params::Space(space).travel_of(band, name).unwrap_or_else(|| panic!("wanted {band} {name} automatable")) as f32
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

fn followed(changes: &[Change]) {
    let (switch, heard) = (beat_frame(SWITCH_BEATS), beat_frame(FOLLOWED_BEATS));
    let mut runs: Vec<(Space, Option<Space>)> = Vec::new();
    let picks: Vec<[usize; 3]> = changes
        .iter()
        .map(|Change { setup, change, .. }| {
            let mut before = Space::DEFAULT;
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
        assert!(parts.is_some_and(|frame| frame >= switch), "{what}: wanted a change synced at frame {switch} to leave every frame before it alone and to be heard after it; the renders part at frame {parts:?}");
        let (kept, switched, fresh) = (&kept[heard..], &switched[heard..], &fresh[heard..]);
        let change_db = db(gap(kept, fresh), rms(fresh.iter().copied()));
        assert!(change_db > PLAIN_DB, "{what}: wanted the change plainly audible, the old and new settings differ by {change_db:.1} dB of the new one from frame {heard}, wanted over {PLAIN_DB} dB");
        let left_db = db(gap(switched, fresh), gap(kept, fresh));
        assert!(left_db < FOLLOWED_DB, "{what}: wanted the audio from frame {heard} to match a render with the new setting from the start, it differs by {left_db:.1} dB of the old setting's difference, wanted under {FOLLOWED_DB} dB");
    }
}

#[test]
fn a_lane_that_moves_after_four_settled_seconds_is_heard() {
    let (moves, heard) = (beat_frame(HELD_BEATS), beat_frame(ARRIVED_BEATS));
    let songs: Vec<[Project; 2]> = LANES
        .iter()
        .map(|Lane { band, name, moved }| {
            let mut swept = Space::DEFAULT;
            moved(&mut swept);
            let (from, to) = (travel(Space::DEFAULT, band, name), travel(swept, band, name));
            [song(Space::DEFAULT, vec![lane(band, name, &[(0, from)])], LANE_BEATS), song(Space::DEFAULT, vec![lane(band, name, &[(0, from), (HELD_BEATS, from), (ARRIVED_BEATS, to)])], LANE_BEATS)]
        })
        .collect();
    let renders = side_by_side(songs.as_flattened(), |project| render(project, None, LANE_BEATS));
    for (Lane { band, name, .. }, [held, late]) in LANES.iter().zip(renders.as_chunks::<2>().0) {
        let parts = parting(held, late);
        assert!(parts.is_some_and(|frame| frame >= moves), "{band} {name}: wanted the held and late lanes alike up to frame {moves}, where the late one starts to move, and apart after it; they part at frame {parts:?}");
        let apart = db(gap(&held[heard..], &late[heard..]), rms(held[heard..].iter().copied()));
        assert!(apart > PLAIN_DB, "{band} {name}: wanted the moved knob plainly heard from frame {heard}, the renders differ by {apart:.1} dB of the held one, wanted over {PLAIN_DB} dB");
    }
}

#[test]
fn a_key_range_synced_after_the_space_settles_is_followed() {
    followed(&RANGES);
}

#[test]
fn a_cut_synced_after_the_space_settles_is_followed() {
    followed(&CUTS);
}
