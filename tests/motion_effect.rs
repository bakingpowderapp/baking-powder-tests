use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::core::params::Osc;
use bp::fx::params::{FreakSlope, MOTION_BANDS, Motion};
use bp::fx::{self, Kind, Slot};
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, Track};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{iter, panic, thread};
use support::{AUDIBLE, BLOCK, LEVEL, SAMPLE_RATE, core_layer, frames, midi, peak_of};

const SECONDS: f64 = 3.0;
const CLIP_BEATS: i32 = 6;
const STACK: usize = 3;
const DETUNE_CENTS: f64 = 14.0;
const NOTES: [(u8, f32, f32, u8); 6] = [(36, 0.0, 1.0, 127), (60, 1.5, 1.5, 40), (64, 1.5, 1.5, 40), (67, 1.5, 1.5, 40), (84, 3.25, 0.5, 100), (48, 4.0, 1.0, 90)];
const LINEAR_LATE: usize = 431;
const SLOPES: [(FreakSlope, usize); 4] = [(FreakSlope::TwoPole, 0), (FreakSlope::Linkwitz, 0), (FreakSlope::Butterworth, 0), (FreakSlope::Linear, LINEAR_LATE)];
const ONE_BAND: usize = 1;
const BANDS_ON: [bool; 2] = [true, false];
const SIDES: [&str; 2] = ["left", "right"];

type Heard = [Vec<f32>; 2];

struct Parting {
    frame: usize,
    side: &'static str,
    one: f32,
    other: f32,
}

fn material() -> Row {
    let mut patch = Params::DEFAULT;
    patch.osc1 = Osc { unison: STACK, detune: DETUNE_CENTS, ..Osc::with_level(1.0) };
    Row::Core(core_layer(LEVEL, patch))
}

fn alone(mut track: Track) -> Project {
    let notes = NOTES.iter().map(|&(pitch, start, length, velocity)| Note { pitch, start, length, velocity, articulation: Articulation::PLAIN }).collect();
    track.clips.push(Clip { start: 0, length: CLIP_BEATS * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0] = track;
    project
}

fn song(rows: &[Row]) -> Project {
    let mut track = Track::blank();
    track.rack = [midi(Arp::DEFAULT), material()].into_iter().chain(rows.iter().copied()).collect();
    alone(track)
}

fn heard(project: &Project) -> Heard {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
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
    let workers = thread::available_parallelism().map_or(1, NonZeroUsize::get).min(jobs.len()).max(1);
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

fn audible(input: &Heard) {
    let loudest = peak_of(&input[0]).max(peak_of(&input[1]));
    assert!(loudest > AUDIBLE, "wanted the material audible over {AUDIBLE}, got a peak of {loudest}");
}

fn first_parting(one: &Heard, other: &Heard, late: usize) -> Option<Parting> {
    (0..other[0].len()).find_map(|frame| {
        SIDES.iter().enumerate().find_map(|(side, name)| {
            let wanted = frame.checked_sub(late).map_or(0.0, |at| one[side][at]);
            let got = other[side][frame];
            (got.to_bits() != wanted.to_bits()).then_some(Parting { frame, side: name, one: wanted, other: got })
        })
    })
}

fn neutral(slope: FreakSlope, band_count: usize, on: bool) -> Slot {
    let mut motion = Motion { slope, band_count, ..Motion::DEFAULT };
    for band in &mut motion.bands {
        band.on = on;
    }
    Slot { params: fx::Params::Motion(motion), ..Slot::new(Kind::Motion) }
}

fn word(on: bool) -> &'static str {
    if on { "on" } else { "off" }
}

#[test]
fn a_neutral_row_is_its_input_late_by_its_reported_latency_on_every_slope() {
    assert!(neutral(FreakSlope::Linkwitz, MOTION_BANDS, true) == Slot::new(Kind::Motion), "wanted the 24 slope on {MOTION_BANDS} bands, every band on, to be the row at its defaults");
    let mut cases: Vec<(String, Slot, usize)> = SLOPES.iter().flat_map(|&(slope, late)| BANDS_ON.map(|on| (format!("{} slope, {MOTION_BANDS} bands, every band {}", slope.name(), word(on)), neutral(slope, MOTION_BANDS, on), late))).collect();
    cases.push((String::from("linear slope, one band"), neutral(FreakSlope::Linear, ONE_BAND, true), 0));
    let projects: Vec<Project> = iter::once(song(&[])).chain(cases.iter().map(|(_, slot, _)| song(&[Row::Effect(*slot)]))).collect();
    let renders = side_by_side(&projects, heard);
    let (input, rest) = renders.split_first().expect("wanted the material rendered without the row");
    audible(input);
    let strays: Vec<String> = cases
        .iter()
        .zip(rest)
        .filter_map(|((label, slot, late), got)| {
            let reported = slot.latency(SAMPLE_RATE);
            if reported != *late {
                return Some(format!("{label}: wanted a report of {late} frames at {SAMPLE_RATE} Hz, got {reported}"));
            }
            first_parting(input, got, reported).map(|at| format!("{label}: frame {} on the {} wanted {}, the input {reported} frames earlier, got {}", at.frame, at.side, at.one, at.other))
        })
        .collect();
    assert!(strays.is_empty(), "wanted every neutral Motion row bit for bit its input late by its reported latency over {SECONDS} s, got {strays:#?}");
}
