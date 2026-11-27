use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::project::{Clip, Note, Project, Row, VELOCITY, ticks};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{iter, panic, thread};
use support::{BLOCK, SAMPLE_RATE, SILENT, core_layer, midi};

const LOOP_BEATS: f64 = 4.0;
const LAST_OFFSET: usize = 600;
const OFFSET_STRIDE: usize = 7;
const SEEK_TO: f64 = 2.5;
const BLOCKS_AFTER_SEEK: usize = 20;
const SETTLE_SECONDS: f64 = 0.1;
const LISTEN_SECONDS: f64 = 0.5;
fn project() -> Project {
    let mut patch = Params::DEFAULT;
    patch.amp.attack_ms = 1.0;
    patch.amp.sustain = 1.0;
    patch.amp.release_ms = 1.0;
    let notes = vec![
        Note { pitch: 60, start: 0.0, length: 1.0, velocity: VELOCITY, articulation: Articulation::PLAIN },
        Note { pitch: 64, start: 0.0, length: 1.0, velocity: VELOCITY, articulation: Articulation::PLAIN },
        Note { pitch: 67, start: 0.5, length: 2.0, velocity: VELOCITY, articulation: Articulation::PLAIN },
        Note { pitch: 62, start: 1.0, length: 1.0, velocity: VELOCITY, articulation: Articulation::PLAIN },
        Note { pitch: 65, start: 2.0, length: 1.0, velocity: VELOCITY, articulation: Articulation::PLAIN },
        Note { pitch: 69, start: 3.0, length: 1.0, velocity: VELOCITY, articulation: Articulation::PLAIN },
        Note { pitch: 72, start: 3.5, length: 1.0, velocity: VELOCITY, articulation: Articulation::PLAIN },
    ];
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch))];
    project.tracks[0].clips.push(Clip { start: 0, length: ticks(LOOP_BEATS), label: String::new(), notes, file: None });
    project
}

fn peak_after_stop(project: &Project, blocks: usize, seek: Option<f64>) -> f32 {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, None, &[], &[]);
    audio.set_loop(Some((0.0, LOOP_BEATS)));
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    for _ in 0..blocks {
        renderer.render(&mut l, &mut r);
    }
    if let Some(beat) = seek {
        audio.play(beat);
        for _ in 0..BLOCKS_AFTER_SEEK {
            renderer.render(&mut l, &mut r);
        }
    }
    audio.stop();
    let settle = (SAMPLE_RATE * SETTLE_SECONDS) as usize / BLOCK;
    let listen = (SAMPLE_RATE * LISTEN_SECONDS) as usize / BLOCK;
    let mut peak = 0.0f32;
    for block in 0..settle + listen {
        renderer.render(&mut l, &mut r);
        if block >= settle {
            peak = l.iter().fold(peak, |peak, sample| peak.max(sample.abs()));
        }
    }
    peak
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

fn loud_after(seek: Option<f64>) -> Vec<String> {
    let project = project();
    let stops: Vec<usize> = (1..LAST_OFFSET).step_by(OFFSET_STRIDE).collect();
    let peaks = side_by_side(&stops, |&blocks| peak_after_stop(&project, blocks, seek));
    stops.into_iter().zip(peaks).filter(|&(_, peak)| peak >= SILENT).map(|(blocks, peak)| format!("stopped after {blocks} blocks: peak {peak} half a second on")).collect()
}

#[test]
fn stopping_anywhere_leaves_nothing_sounding() {
    let loud = loud_after(None);
    assert!(loud.is_empty(), "{loud:#?}");
}

#[test]
fn seeking_then_stopping_leaves_nothing_sounding() {
    let loud = loud_after(Some(SEEK_TO));
    assert!(loud.is_empty(), "{loud:#?}");
}
