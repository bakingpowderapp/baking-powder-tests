use bp::arp::{Arp, Articulation};
use bp::audio::Tail;
use bp::core::Params;
use bp::factory::PRESETS;
use bp::fx::{Beat, Clock, Effect, Slot};
use bp::json;
use bp::project::{Clip, Note, Preset, Project, RackTiming, Row, TICKS_PER_BEAT, Timing, Track, VELOCITY};
use bp::wav::Wave;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{iter, panic, thread};
use support::{Onsets, SAMPLE_RATE, WITHIN, beat_frame, bounce, core_layer, midi};

const HEADER: usize = 68;
const SCALE: f32 = 8_388_608.0;
const EXPORT_BLOCK: usize = 4096;
const CLICK: f32 = 0.25;
const CLICK_PCM: i16 = 8_192;
const CLICK_FRAME: usize = 48_000;
const WAVE_FRAMES: usize = 3 * 48_000;
const CLIP_BEATS: i32 = 6;
const SPAN_BEATS: i32 = 3;
const RESPONSE: usize = 4096;
const TOLERANCE: f32 = 1e-4;
const ALIGNED: f32 = 0.1;
const WAVE_NAME: &str = "click.wav";
const INSTRUMENTS: &str = "Instruments/";
const EFFECTS: &str = "Effects/";
const NOTE_BEATS: f32 = 0.25;
const TRACKS: usize = 3;
const BAR: i32 = 4 * TICKS_PER_BEAT;

fn loaded(name: &str, text: &str) -> Preset {
    let json = json::parse(text).unwrap_or_else(|| panic!("wanted {name} to parse as JSON"));
    Preset::from_json(&json).unwrap_or_else(|| panic!("wanted {name} to read as a preset"))
}

fn placed(name: &str, preset: &Preset) -> Option<Track> {
    let mut track = if name.starts_with(INSTRUMENTS) { Track::blank() } else { Track::audio() };
    match preset {
        Preset::Rack(sound) => {
            track.take_sound(sound.clone());
        }
        Preset::Row(row, _) => track.rack = vec![*row],
        Preset::Chords(_) => return None,
    }
    Some(track)
}

fn alone(track: Track) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0] = track;
    project
}

fn left(wav: &[u8]) -> Vec<f32> {
    wav[HEADER..].as_chunks::<3>().0.iter().map(|b| (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / SCALE).step_by(2).collect()
}

fn click_wave() -> Arc<Wave> {
    let mut pcm = vec![0i16; WAVE_FRAMES];
    pcm[CLICK_FRAME] = CLICK_PCM;
    Arc::new(Wave::sound(String::from(WAVE_NAME), SAMPLE_RATE, 1, pcm))
}

fn clicked(rows: Vec<Row>) -> Project {
    let mut track = Track::audio();
    track.rack = rows;
    track.clips.push(Clip::audio(0, CLIP_BEATS * TICKS_PER_BEAT, String::from("click"), String::from(WAVE_NAME)));
    alone(track)
}

fn exported(project: &Project, wave: &Arc<Wave>) -> Option<Vec<f32>> {
    bounce(project, std::slice::from_ref(wave), (0, SPAN_BEATS * TICKS_PER_BEAT), Tail::Cut).map(|wav| left(&wav))
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

fn through_effect(rows: &[Row], bpm: f64) -> Vec<f32> {
    let mut chain: Vec<(Slot, Effect)> = rows
        .iter()
        .filter_map(|row| match row {
            Row::Effect(slot) if !slot.bypass => Some((*slot, Effect::new(slot.kind(), SAMPLE_RATE, EXPORT_BLOCK))),
            _ => None,
        })
        .collect();
    for (_, unit) in &mut chain {
        unit.restart();
    }
    let frames = beat_frame(SPAN_BEATS as usize);
    let frames_per_beat = SAMPLE_RATE * 60.0 / bpm;
    let mut out = Vec::with_capacity(frames);
    let (mut l, mut r) = (vec![0.0f32; EXPORT_BLOCK], vec![0.0f32; EXPORT_BLOCK]);
    let mut start = 0;
    while start < frames {
        let n = EXPORT_BLOCK.min(frames - start);
        l[..n].fill(0.0);
        r[..n].fill(0.0);
        if (start..start + n).contains(&CLICK_FRAME) {
            l[CLICK_FRAME - start] = CLICK;
            r[CLICK_FRAME - start] = CLICK;
        }
        let clock = Clock { sample_rate: SAMPLE_RATE, bpm, beat: Some(Beat { anchor: 0.0, since: start as i64, frames_per_beat }) };
        for (slot, unit) in &mut chain {
            unit.process(&mut l[..n], &mut r[..n], slot, &clock);
        }
        out.extend_from_slice(&l[..n]);
        start += n;
    }
    out
}

fn plain_core() -> Row {
    let mut patch = Params::DEFAULT;
    patch.amp.attack_ms = 1.0;
    patch.amp.release_ms = 1.0;
    patch.amp.sustain = 1.0;
    Row::Core(core_layer(1.0, patch))
}

fn rack_figures(label: &str, rack: &RackTiming, into: &mut Vec<(String, usize)>) {
    into.push((format!("{label} latency"), rack.latency));
    for (row, timing) in rack.rows.iter().enumerate() {
        into.push((format!("{label} row {row} latency"), timing.latency));
        into.push((format!("{label} row {row} before"), timing.before));
        into.push((format!("{label} row {row} signal"), timing.signal));
        for fold in &timing.folds {
            into.push((format!("{label} row {row} fold lane"), fold.lane));
            into.push((format!("{label} row {row} fold sum"), fold.sum));
        }
    }
}

fn figures(timing: &Timing) -> Vec<(String, usize)> {
    let mut all = vec![(String::from("output"), timing.output), (String::from("returns in"), timing.returns_in), (String::from("master in"), timing.master_in)];
    for (index, track) in timing.tracks.iter().enumerate() {
        all.push((format!("track {index} source"), track.source));
        all.push((format!("track {index} pre"), track.pre));
        all.push((format!("track {index} latency"), track.latency));
        all.push((format!("track {index} to master"), track.to_master));
        all.push((format!("track {index} to returns"), track.to_returns));
        all.push((format!("track {index} to group"), track.to_group));
        all.push((format!("track {index} uncompensated"), track.uncompensated));
        rack_figures(&format!("track {index} rack"), &track.rack, &mut all);
    }
    for (send, timing) in timing.returns.iter().enumerate() {
        rack_figures(&format!("return {send}"), timing, &mut all);
    }
    for (send, figure) in timing.return_to_master.iter().enumerate() {
        all.push((format!("return {send} to master"), *figure));
    }
    rack_figures("master", &timing.master, &mut all);
    all
}

#[test]
fn every_effects_preset_exports_as_its_rows_run_through_effect_shifted_by_the_reported_latency() {
    let wave = click_wave();
    let clicks: Vec<(&str, Project)> = PRESETS
        .iter()
        .filter(|(name, _)| name.starts_with(EFFECTS))
        .map(|(name, text)| {
            let name = name.trim_end_matches(".json");
            let track = placed(name, &loaded(name, text)).unwrap_or_else(|| panic!("wanted {name} to be a rack or a row"));
            (name, clicked(track.rack))
        })
        .collect();
    let (exports, directs) = thread::scope(|scope| {
        let exports = scope.spawn(|| clicks.iter().map(|(_, project)| exported(project, &wave)).collect::<Vec<_>>());
        let directs = side_by_side(&clicks, |(_, project)| through_effect(&project.tracks[0].rack, project.bpm));
        (exports.join().unwrap_or_else(|panic| panic::resume_unwind(panic)), directs)
    });
    let mut unlike = Vec::new();
    let mut checked = 0;
    for (((name, project), export), direct) in clicks.iter().zip(exports).zip(directs) {
        let rows = &project.tracks[0].rack;
        let latency = Timing::of(project, SAMPLE_RATE).output;
        let summed: usize = rows.iter().map(|row| row.latency(SAMPLE_RATE)).sum();
        assert_eq!(latency, summed, "wanted {name}'s plan to report the sum of its rows' latencies");
        let export = export.expect("wanted the click to export");
        let (worst, at) = (CLICK_FRAME..CLICK_FRAME + RESPONSE).map(|n| ((export[n] - direct[n + latency]).abs(), n)).fold((0.0f32, CLICK_FRAME), |best, next| if next.0 > best.0 { next } else { best });
        if worst > TOLERANCE {
            unlike.push(format!("{name} shifted by {latency}: worst delta {worst} at frame {at}, export {} against direct {}", export[at], direct[at + latency]));
        }
        if latency > 0 && export[CLICK_FRAME].abs() < ALIGNED {
            unlike.push(format!("{name}: wanted the click back on frame {CLICK_FRAME} after the export's trim of {latency}, got {} there", export[CLICK_FRAME]));
        }
        checked += 1;
    }
    assert!(checked > 0, "wanted effects presets in the factory");
    assert!(unlike.is_empty(), "wanted every effects preset's export to equal its rows run through Effect shifted by the reported latency within {TOLERANCE} over the click's first {RESPONSE} frames, got {unlike:?}");
}

#[test]
fn three_dry_cores_plan_nothing_and_export_their_notes_on_the_grid() {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.add_midi_track(1);
    project.add_midi_track(2);
    assert_eq!(project.tracks.len(), TRACKS, "wanted three MIDI tracks");
    for (index, track) in project.tracks.iter_mut().enumerate() {
        track.rack = vec![midi(Arp::DEFAULT), plain_core()];
        track.clips.push(Clip { start: 0, length: BAR, label: String::new(), notes: vec![Note { pitch: 60, start: (index + 1) as f32, length: NOTE_BEATS, velocity: VELOCITY, articulation: Articulation::PLAIN }], file: None });
    }
    let timing = Timing::of(&project, SAMPLE_RATE);
    let nonzero: Vec<(String, usize)> = figures(&timing).into_iter().filter(|(_, figure)| *figure > 0).collect();
    assert!(nonzero.is_empty(), "wanted every figure of the plan at zero for three dry Cores, got {nonzero:?}");
    let wav = bounce(&project, &[], (0, BAR), Tail::Cut).expect("wanted the three notes to export");
    let mut onsets = Onsets::default();
    onsets.feed(&left(&wav));
    let wanted: Vec<usize> = (1..=TRACKS).map(beat_frame).collect();
    assert_eq!(onsets.at.len(), wanted.len(), "wanted one onset per note at frames {wanted:?}, got {:?}", onsets.at);
    for (got, want) in onsets.at.iter().zip(&wanted) {
        assert!(got.abs_diff(*want) <= WITHIN, "wanted a note's onset at frame {want} within {WITHIN}, got {got}; onsets {:?}", onsets.at);
    }
}
