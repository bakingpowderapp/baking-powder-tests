use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::core::params::Osc;
use bp::fx::params::{Freak, FreakSlope};
use bp::fx::{self, Kind, Slot};
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{iter, panic, thread};
use support::{AUDIBLE, BLOCK, LEVEL, SAMPLE_RATE, core_layer, frames, peak_of, struck};

const SECONDS: f64 = 3.0;
const CLIP_BEATS: i32 = 6;
const STACK: usize = 3;
const DETUNE_CENTS: f64 = 14.0;
const NOTES: [(u8, f32, f32, u8); 6] = [(36, 0.0, 1.0, 127), (60, 1.5, 1.5, 40), (64, 1.5, 1.5, 40), (67, 1.5, 1.5, 40), (84, 3.25, 0.5, 100), (48, 4.0, 1.0, 90)];
const SLOPES: [FreakSlope; 2] = [FreakSlope::Linkwitz, FreakSlope::Linear];
const OVERSAMPLING: [bool; 2] = [true, false];
const BANDS_ON: [bool; 2] = [true, false];
const SIDES: [&str; 2] = ["left", "right"];

type Heard = [Vec<f32>; 2];

fn material() -> Row {
    let mut patch = Params::DEFAULT;
    patch.osc1 = Osc { unison: STACK, detune: DETUNE_CENTS, ..Osc::with_level(1.0) };
    Row::Core(core_layer(LEVEL, patch))
}

fn song(rows: &[Row]) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = [struck(Arp::DEFAULT), material()].into_iter().chain(rows.iter().copied()).collect();
    let notes = NOTES.iter().map(|&(pitch, start, length, velocity)| Note { pitch, start, length, velocity, articulation: Articulation::PLAIN }).collect();
    project.tracks[0].clips.push(Clip { start: 0, length: CLIP_BEATS * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn heard(project: &Project) -> Heard {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, None, &[], &[]);
    audio.play(0.0);
    let end = frames(SECONDS);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out: Heard = [Vec::with_capacity(end + BLOCK), Vec::with_capacity(end + BLOCK)];
    while out[0].len() < end {
        renderer.render(&mut l, &mut r);
        out[0].extend_from_slice(&l);
        out[1].extend_from_slice(&r);
    }
    out.map(|mut side| {
        side.truncate(end);
        side
    })
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
    done.sort_unstable_by_key(|(at, _)| *at);
    done.into_iter().map(|(_, result)| result).collect()
}

fn wire(slope: FreakSlope, oversample: bool, on: bool) -> Slot {
    let mut freak = Freak { slope, oversample, ..Freak::DEFAULT };
    for band in &mut freak.bands {
        band.on = on;
    }
    Slot { params: fx::Params::Freak(freak), ..Slot::new(Kind::Freak) }
}

fn word(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

fn first_stray(got: &Heard, input: &Heard, late: usize) -> Option<(usize, &'static str, f32, f32)> {
    (0..got[0].len()).find_map(|frame| {
        SIDES.iter().enumerate().find_map(|(side, name)| {
            let wanted = frame.checked_sub(late).map_or(0.0, |at| input[side][at]);
            let sample = got[side][frame];
            (sample.to_bits() != wanted.to_bits()).then_some((frame, *name, wanted, sample))
        })
    })
}

#[test]
fn a_neutral_row_is_its_input_late_by_its_reported_latency() {
    assert!(wire(FreakSlope::Linkwitz, true, true) == Slot::new(Kind::Freak), "wanted the 24 slope, oversampled, every band on, to be the row at its defaults");
    let cases: Vec<(String, Slot)> = SLOPES
        .iter()
        .flat_map(|&slope| OVERSAMPLING.iter().flat_map(move |&oversample| BANDS_ON.map(|on| (format!("{} slope, oversample {}, every band {}", slope.name(), word(oversample), word(on)), wire(slope, oversample, on)))))
        .collect();
    let projects: Vec<Project> = iter::once(song(&[])).chain(cases.iter().map(|(_, slot)| song(&[Row::Effect(*slot)]))).collect();
    let mut renders = side_by_side(&projects, heard).into_iter();
    let input = renders.next().expect("wanted the material rendered without the row");
    let loudest = peak_of(&input[0]).max(peak_of(&input[1]));
    assert!(loudest > AUDIBLE, "wanted the material audible over {AUDIBLE}, got a peak of {loudest}");
    let strays: Vec<String> = cases
        .iter()
        .zip(renders)
        .filter_map(|((label, slot), got)| {
            let late = slot.latency(SAMPLE_RATE);
            first_stray(&got, &input, late).map(|(frame, side, wanted, sample)| format!("{label}: frame {frame} on the {side} wanted {wanted}, the input {late} frames earlier, got {sample}"))
        })
        .collect();
    assert!(strays.is_empty(), "wanted every neutral row bit for bit its input late by its reported latency over {SECONDS} s, got {strays:#?}");
}
