use bp::arp::{Arp, MidiRow};
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::core::Params;
use bp::core::params::{Envelope, Osc};
use bp::fx::Kind;
use bp::project::{Project, Row, VELOCITY};
use support::{AUDIBLE, BLOCK, CHORD, CORE_ROW, SAMPLE_RATE, SILENT, amplitude, core_layer, effect, first_difference, midi, peak};

const BEFORE: usize = 40;
const HELD: usize = 80;
const RUNG_OUT: usize = 60;
const NEXT: usize = 40;
const KEY: u8 = 60;
const FIFTH: i8 = 7;
const LEVEL: f64 = 0.2;
const MIDI_ROW: usize = 0;
const CLEAR: f64 = 4.0;
const STEPS: [(usize, usize); 4] = [(1, 2), (1, 4), (2, 1), (4, 2)];

struct Session {
    project: Project,
    audio: Audio,
    renderer: Renderer,
}

fn patch() -> Params {
    let mut patch = Params::DEFAULT;
    patch.osc1 = Osc { levels: [0.0, 0.0, 0.0, 1.0], ..Osc::with_level(1.0) };
    patch.amp = Envelope { attack_ms: 2.0, decay_ms: 50.0, sustain: 0.8, release_ms: 20.0, ..Envelope::DEFAULT };
    patch
}

fn timed(mut row: Row, times: usize) -> Row {
    row.set_times(times);
    row
}

fn hz(key: u8) -> f64 {
    440.0 * ((f64::from(key) - 69.0) / 12.0).exp2()
}

impl Session {
    fn open(rack: Vec<Row>, keys: &[u8]) -> Session {
        let mut project = Project::new();
        project.tracks.truncate(1);
        project.tracks[0].rack = rack;
        project.seed();
        let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
        renderer.patient();
        renderer.start_workers(0, || None);
        audio.sync(&project, None, &[], &[]);
        let mut session = Session { project, audio, renderer };
        session.strike(keys);
        session.audio.play(0.0);
        session
    }

    fn strike(&mut self, keys: &[u8]) {
        for &key in keys {
            self.audio.note_on(0, key, VELOCITY);
        }
    }

    fn lift(&mut self, keys: &[u8]) {
        for &key in keys {
            self.audio.note_off(0, key);
        }
    }

    fn step(&mut self, row: usize, times: usize) {
        self.project.tracks[0].rack[row].set_times(times);
        self.audio.sync(&self.project, None, &[], &[]);
    }

    fn add_above(&mut self, row: usize, added: Row) {
        self.project.tracks[0].rack.insert(row, added);
        self.audio.sync(&self.project, None, &[], &[]);
    }

    fn render(&mut self, blocks: usize) -> Vec<Vec<f32>> {
        let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
        (0..blocks)
            .map(|_| {
                self.renderer.render(&mut l, &mut r);
                l.clone()
            })
            .collect()
    }
}

fn cores(times: usize) -> Vec<Row> {
    vec![midi(Arp::DEFAULT), timed(Row::Core(core_layer(LEVEL, patch())), times)]
}

fn moved(times: usize) -> Vec<Row> {
    vec![Row::Midi(MidiRow { times, transpose: FIFTH, ..MidiRow::DEFAULT }), Row::Core(core_layer(LEVEL, patch()))]
}

fn held_through(rack: fn(usize) -> Vec<Row>, row: usize, keys: &[u8], from: usize, to: Option<usize>) -> (Session, Vec<Vec<f32>>) {
    let mut session = Session::open(rack(from), keys);
    let mut out = session.render(BEFORE);
    if let Some(to) = to {
        session.step(row, to);
    }
    out.extend(session.render(HELD));
    (session, out)
}

fn next_note(session: &mut Session, held: &[u8]) -> Vec<f32> {
    session.lift(held);
    session.render(RUNG_OUT);
    session.strike(&[KEY]);
    session.render(NEXT).concat()
}

#[test]
fn a_held_chord_sounds_on_sample_for_sample_when_a_cores_times_is_stepped_up_or_down() {
    for (from, to) in STEPS {
        let (_, stepped) = held_through(cores, CORE_ROW, &CHORD, from, Some(to));
        let (_, untouched) = held_through(cores, CORE_ROW, &CHORD, from, None);
        assert!(peak(&untouched[BEFORE..]) > AUDIBLE, "wanted the chord held on a Core at {from} still sounding, so the comparison proves something");
        assert_eq!(first_difference(&stepped, &untouched), None, "wanted the chord held on a Core stepped from {from} to {to} to sound on as if nothing was pressed, first block apart");
    }
}

#[test]
fn the_next_note_plays_at_a_cores_new_times() {
    for (from, to) in STEPS {
        let (mut stepped, _) = held_through(cores, CORE_ROW, &CHORD, from, Some(to));
        let (mut fresh, _) = held_through(cores, CORE_ROW, &CHORD, to, None);
        let (mut untouched, _) = held_through(cores, CORE_ROW, &CHORD, from, None);
        let level = |session: &mut Session| amplitude(&next_note(session, &CHORD), hz(KEY));
        let (now, wanted, before) = (level(&mut stepped), level(&mut fresh), level(&mut untouched));
        assert!(wanted > f64::from(AUDIBLE) && (now - wanted).abs() < (wanted - before).abs() / CLEAR, "wanted the note struck after a step from {from} to {to} as loud as on a Core that stood at {to} all along, {wanted:.4}, and not as at {from}, {before:.4}, got {now:.4}");
    }
}

#[test]
fn a_copy_on_its_way_out_is_dropped_once_what_it_holds_has_rung_out() {
    let (mut session, _) = held_through(cores, CORE_ROW, &CHORD, 2, Some(1));
    assert!(!session.audio.settled(true), "wanted the Core's second copy kept while the chord it holds still sounds");
    session.lift(&CHORD);
    let tail = session.render(RUNG_OUT);
    assert!(peak(&tail[RUNG_OUT / 2..]) < SILENT, "wanted the released chord rung out, so the copy has nothing left to hold");
    assert!(session.audio.settled(true), "wanted the copy on its way out told as finished once silent");
    assert!(!session.audio.settled(true), "wanted nothing more to drop once the finished copy was told");
}

#[test]
fn a_row_added_above_carries_along_the_copy_on_its_way_out() {
    let through = |to: Option<usize>| {
        let (mut session, mut out) = held_through(cores, CORE_ROW, &CHORD, 2, to);
        session.add_above(CORE_ROW, effect(Kind::Utility));
        out.extend(session.render(HELD));
        (session, out)
    };
    let ((mut stepped, heard), (_, untouched)) = (through(Some(1)), through(None));
    assert!(peak(&untouched[BEFORE + HELD..]) > AUDIBLE, "wanted the chord still sounding after the row is added, so the comparison proves something");
    assert_eq!(first_difference(&heard, &untouched), None, "wanted the chord held on a Core stepped from 2 to 1 to sound on when a row is then added above the Core, as it does with the Core left at 2, first block apart");
    stepped.lift(&CHORD);
    let tail = stepped.render(RUNG_OUT);
    assert!(peak(&tail[RUNG_OUT / 2..]) < SILENT, "wanted the released chord rung out");
    assert!(stepped.audio.settled(true), "wanted the copy on its way out told as finished in its new place once silent");
}

#[test]
fn a_held_note_keeps_its_pitch_when_a_midi_rows_times_is_stepped_and_the_next_note_takes_the_new_count() {
    for (from, to) in [(1, 2), (2, 1)] {
        let (mut stepped, held) = held_through(moved, MIDI_ROW, &[KEY], from, Some(to));
        let (_, untouched) = held_through(moved, MIDI_ROW, &[KEY], from, None);
        assert!(peak(&untouched[BEFORE..]) > AUDIBLE, "wanted the note held under a MIDI row at {from} still sounding");
        assert_eq!(first_difference(&held, &untouched), None, "wanted the note held under a MIDI row stepped from {from} to {to} to sound on at the pitch it went on at, first block apart");
        let next = next_note(&mut stepped, &[KEY]);
        let (old, new) = (hz(KEY + FIFTH as u8 * from as u8), hz(KEY + FIFTH as u8 * to as u8));
        let (at_old, at_new) = (amplitude(&next, old), amplitude(&next, new));
        assert!(at_new > f64::from(AUDIBLE) && at_new > CLEAR * at_old, "wanted the next note moved up {to} times by the row, sounding at {new:.1} Hz and not {old:.1} Hz, got {at_new:.4} against {at_old:.4}");
    }
}
