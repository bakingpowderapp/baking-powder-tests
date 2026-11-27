use bp::core::{Core, Params};
use bp::face::Faced;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{iter, panic, thread};
use support::{BLOCK, SAMPLE_RATE};

const HELD_BLOCKS: usize = 47;
const RELEASED_BLOCKS: usize = 47;
const CHORD: [u8; 3] = [48, 64, 79];
const TRAVELS: [f64; 3] = [0.0, 0.5, 1.0];
const RUNAWAY: f32 = 100.0;
const MOST_CELLS: usize = 512;
const CLEAR: usize = 128;

fn render(params: Params) -> Vec<f32> {
    let mut core = Core::new(SAMPLE_RATE, params);
    for midi in CHORD {
        core.note_on(midi, 127);
    }
    let mut out = Vec::with_capacity((HELD_BLOCKS + RELEASED_BLOCKS) * BLOCK * 2);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    for block in 0..HELD_BLOCKS + RELEASED_BLOCKS {
        if block == HELD_BLOCKS {
            for midi in CHORD {
                core.note_off(midi);
            }
        }
        core.render(&mut l, &mut r);
        out.extend_from_slice(&l);
        out.extend_from_slice(&r);
    }
    out
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

#[test]
fn every_core_cell_at_its_extremes_and_middle_renders_finite_audio() {
    let rested = Params::DEFAULT;
    let cells: Vec<(&str, &str)> = (0..MOST_CELLS).filter_map(|index| rested.setting_of(index).map(|(band, param, _)| (band, param))).collect();
    assert!(!cells.is_empty() && (MOST_CELLS - CLEAR..MOST_CELLS).all(|index| rested.setting_of(index).is_none()), "wanted every Core cell inside the first {MOST_CELLS} with the last {CLEAR} of them empty, so none is left out of the sweep, got {} cells", cells.len());
    let settings: Vec<(&str, &str, f64)> = cells.iter().flat_map(|&(band, param)| TRAVELS.iter().map(move |&travel| (band, param, travel))).collect();
    let failures: Vec<String> = side_by_side(&settings, |&(band, param, travel)| {
        let mut params = Params::DEFAULT;
        if !params.set_travel(band, param, travel) {
            params.assign(band, param, travel);
        }
        let out = render(params);
        let non_finite = out.iter().filter(|sample| !sample.is_finite()).count();
        let peak = out.iter().filter(|sample| sample.is_finite()).fold(0.0f32, |peak, sample| peak.max(sample.abs()));
        (non_finite > 0 || peak > RUNAWAY).then(|| format!("{band} / {param} at travel {travel}: {non_finite} non-finite samples, peak {peak}"))
    })
    .into_iter()
    .flatten()
    .collect();
    assert!(failures.is_empty(), "{failures:#?}");
}
