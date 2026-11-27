use bp::arp::Arp;
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::core::Params;
use bp::fx::params::Suppressor;
use bp::fx::{self, Kind, Slot};
use bp::project::{Host, Project, Row, VELOCITY};
use bp::spectrum::{CURVE, Scope, curve_hz};
use support::{BLOCK, BLOCKS, CORE_ROW, LEVEL, SAMPLE_RATE, core_layer, midi};

const TRACK: usize = 0;
const ROW: usize = CORE_ROW + 1;
const MIDDLE_C: u8 = 60;
const MIDDLE_C_HZ: f64 = 261.63;
const DEPTH: f64 = 8.0;
const TICK_SECONDS: f32 = 0.05;
const CUT_DB: f32 = 2.0;
const QUIET_DB: f32 = 0.5;
const OCTAVES_AWAY: f64 = 2.0;

fn sine_core() -> Row {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    Row::Core(core_layer(LEVEL, patch))
}

fn suppressing() -> Row {
    let mut params = Suppressor::DEFAULT;
    params.bands[0].depth = DEPTH;
    Row::Effect(Slot { params: fx::Params::Suppressor(params), ..Slot::new(Kind::Suppressor) })
}

fn band_of(hz: f64) -> usize {
    (0..CURVE).min_by(|a, b| (curve_hz(*a) - hz).abs().total_cmp(&(curve_hz(*b) - hz).abs())).expect("wanted a band nearest the note")
}

fn played() -> (Audio, Renderer) {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[TRACK].rack = vec![midi(Arp::DEFAULT), sine_core(), suppressing()];
    let (mut audio, renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(&project, None, &[], &[]);
    audio.aim(Scope::Insert, Host::Track(TRACK).insert(ROW));
    audio.note_on(TRACK, MIDDLE_C, VELOCITY);
    audio.play(0.0);
    (audio, renderer)
}

#[test]
fn the_pictures_cuts_sit_on_the_note_and_nowhere_else() {
    let (mut audio, mut renderer) = played();
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    for _ in 0..BLOCKS {
        renderer.render(&mut l, &mut r);
    }
    audio.tick(TICK_SECONDS);
    let cuts = *audio.cuts(Scope::Insert);
    let at_note = band_of(MIDDLE_C_HZ);
    assert!(cuts[at_note] > CUT_DB, "wanted the band at {:.0} Hz cut by more than {CUT_DB} dB under a sine at middle C, got {:.2} dB", curve_hz(at_note), cuts[at_note]);
    for hz in [MIDDLE_C_HZ / OCTAVES_AWAY.exp2(), MIDDLE_C_HZ * OCTAVES_AWAY.exp2()] {
        let away = band_of(hz);
        assert!(cuts[away] < QUIET_DB, "wanted the band at {:.0} Hz, two octaves from the note, left within {QUIET_DB} dB, got {:.2} dB", curve_hz(away), cuts[away]);
    }
    let deepest = cuts.iter().copied().fold(0.0f32, f32::max);
    assert!(deepest == cuts[at_note] || (deepest - cuts[at_note]).abs() < QUIET_DB, "wanted the deepest cut at the note, got {deepest:.2} dB elsewhere against {:.2} dB at the note", cuts[at_note]);
}

#[test]
fn a_row_that_is_not_aimed_at_shows_no_cuts() {
    let (mut audio, mut renderer) = played();
    audio.aim(Scope::Insert, Host::Track(TRACK).insert(CORE_ROW));
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    for _ in 0..BLOCKS {
        renderer.render(&mut l, &mut r);
    }
    audio.tick(TICK_SECONDS);
    let cuts = *audio.cuts(Scope::Insert);
    assert!(cuts.iter().all(|cut| *cut == 0.0), "wanted no cuts shown for the Core row, got {cuts:?}");
}
