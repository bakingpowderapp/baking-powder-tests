use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::fx::{self, Kind, Slot};
use bp::fx::params::Reverb;
use bp::json;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{BLOCK, SAMPLE_RATE, core_layer, midi};

const C5: u8 = 72;
const NOTE_BEATS: f32 = 0.1;
const SHORT_ROOM_S: f64 = 0.03;
const LONGER_ROOM_S: f64 = 0.3;
const SMALL_SIZE: f64 = 0.25;
const EARLY_WINDOW: (f64, f64) = (0.10, 0.15);
const LATE_WINDOW: (f64, f64) = (0.20, 0.25);
const FALLEN_DB: f64 = 30.0;
const QUIETER_DB: f64 = 12.0;

fn reverb(decay_s: f64) -> Row {
    let mut slot = Slot::new(Kind::Reverb);
    slot.wet = 1.0;
    if let fx::Params::Reverb(p) = &mut slot.params {
        p.decay_s = decay_s;
        p.pre_delay_ms = 0.0;
        p.early_level = 0.0;
        p.size = SMALL_SIZE;
    }
    Row::Effect(slot)
}

fn project(decay_s: f64) -> Project {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.amp.attack_ms = 1.0;
    patch.amp.release_ms = 5.0;
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(0.25, patch)), reverb(decay_s)];
    let notes = vec![Note { pitch: C5, start: 0.0, length: NOTE_BEATS, velocity: VELOCITY, articulation: Articulation::PLAIN }];
    track.clips.push(Clip { start: 0, length: 4 * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn render(project: &Project, seconds: f64) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(project, None, &[], &[]);
    audio.play(0.0);
    let frames = (seconds * SAMPLE_RATE) as usize;
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(frames);
    while out.len() < frames {
        let n = BLOCK.min(frames - out.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    out
}

fn window_db(heard: &[f32], window: (f64, f64)) -> f64 {
    let (from, to) = ((window.0 * SAMPLE_RATE) as usize, (window.1 * SAMPLE_RATE) as usize);
    let rms = (heard[from..to].iter().map(|s| f64::from(*s) * f64::from(*s)).sum::<f64>() / (to - from) as f64).sqrt();
    20.0 * rms.max(1e-12).log10()
}

#[test]
fn a_30_ms_room_at_a_small_size_falls_away_within_a_tenth_of_a_second() {
    let short = render(&project(SHORT_ROOM_S), LATE_WINDOW.1);
    let (early, late) = (window_db(&short, EARLY_WINDOW), window_db(&short, LATE_WINDOW));
    assert!(early - late > FALLEN_DB, "wanted the {SHORT_ROOM_S} s room's tail to fall at least {FALLEN_DB} dB between the windows at {EARLY_WINDOW:?} s and {LATE_WINDOW:?} s, got {early:.1} dB then {late:.1} dB");
    let longer = render(&project(LONGER_ROOM_S), LATE_WINDOW.1);
    let longer_late = window_db(&longer, LATE_WINDOW);
    assert!(longer_late - late > QUIETER_DB, "wanted the {SHORT_ROOM_S} s room's late window at least {QUIETER_DB} dB under the {LONGER_ROOM_S} s room's, got {late:.1} dB against {longer_late:.1} dB");
}

#[test]
fn the_decay_floor_saves_at_30_ms() {
    assert!((Reverb::MIN_DECAY_S - SHORT_ROOM_S).abs() < 1e-12, "wanted the reverb's decay floor at {SHORT_ROOM_S} s, got {}", Reverb::MIN_DECAY_S);
    let written = reverb(SHORT_ROOM_S).to_json().write();
    let parsed = json::parse(&written).unwrap_or_else(|| panic!("wanted the reverb row to write valid JSON, got {written}"));
    let Some(Row::Effect(slot)) = Row::from_json(&parsed) else { panic!("wanted the reverb row to read back as an effect from {written}") };
    let fx::Params::Reverb(p) = slot.params else { panic!("wanted a reverb back from {written}") };
    assert!((p.decay_s - SHORT_ROOM_S).abs() < 1e-9, "wanted decayS {SHORT_ROOM_S} kept through the save, got {}", p.decay_s);
}
