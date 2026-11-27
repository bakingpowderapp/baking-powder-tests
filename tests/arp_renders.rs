use bp::arp::{self, Arp, Articulation, Join, Lane, Pattern, Step, Style};
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::core::Params;
use bp::core::params::{Envelope, Modulation, Osc};
use bp::division::Division;
use bp::flatten;
use bp::project::{Aim, Clip, DEFAULT_BPM, LaneLfo, Layer, LfoClock, LfoShape, Note, Project, Row, SongNote, TICKS_PER_BEAT, Track};
use support::{AUDIBLE, BLOCK, FRAMES_PER_BEAT, SAMPLE_RATE, arp_print, core_layer, frames, peak_of, struck};

type Keys = [(u8, f32, f32, u8)];
type Printed = (f64, f64, u8, u8, f32, Option<f32>, i8, Join);
type Scene = (&'static str, fn() -> Session);

const BEATS: f64 = 8.0;
const TAIL_SECONDS: f64 = 1.0;
const LEVEL: f64 = 0.5;
const LINE_SWING: f64 = 0.4;
const LINE_GATE: f64 = 0.35;
const LINE_RESTS: [usize; 2] = [5, 13];
const LINE: [(i8, i8, i8, i8, i8, i8, Join); 16] = [
    (0, 0, 0, 100, 40, 0, Join::Play),
    (7, 0, 0, 70, 25, 50, Join::Play),
    (12, 0, -20, 90, 60, -30, Join::Play),
    (10, 0, 0, 80, 70, 80, Join::Tie),
    (0, -1, 0, 100, 35, -100, Join::Play),
    (0, 0, 0, 100, 50, 0, Join::Play),
    (3, 0, 15, 60, 90, 20, Join::Play),
    (5, 0, 0, 85, 45, 60, Join::Slide),
    (0, 1, 0, 100, 30, 0, Join::Play),
    (-5, 0, 0, 75, 100, -50, Join::Play),
    (-5, 0, 0, 75, 55, 100, Join::Tie),
    (2, 0, 30, 95, 20, 10, Join::Play),
    (7, -1, 0, 65, 80, -70, Join::Play),
    (0, 0, 0, 100, 50, 0, Join::Play),
    (12, 0, -40, 100, 50, 40, Join::Play),
    (10, 1, 0, 55, 15, -20, Join::Play),
];
const LINE_KEYS: [(u8, f32, f32, u8); 2] = [(57, 0.0, 4.5, 110), (60, 4.0, 4.0, 90)];
const CHORD_GATE: f64 = 0.6;
const COPIES: usize = 3;
const STACK: [f64; 3] = [12.0, 19.5, 24.0];
const RUN_REST: usize = 3;
const TOP_NOTE: i8 = 16;
const RUN_STEPS: [(i8, i8, i8); 6] = [(arp::RUN, 0, 0), (arp::RUN, 1, 40), (2, 0, -60), (arp::RUN, 0, 0), (arp::RUN, 0, 100), (TOP_NOTE, -1, -20)];
const CHORD_KEYS: [(u8, f32, f32, u8); 7] = [(60, 0.0, 5.0, 100), (64, 0.0, 5.0, 80), (67, 0.75, 4.25, 120), (71, 2.5, 1.0, 90), (62, 5.5, 2.5, 95), (65, 5.5, 2.5, 105), (69, 5.75, 2.25, 115)];
const PROGRESSION: [(u8, f32, f32, u8); 10] = [(57, 0.0, 2.0, 100), (60, 0.0, 2.0, 100), (64, 0.0, 2.0, 100), (55, 2.0, 2.0, 90), (59, 2.0, 2.0, 90), (62, 2.0, 2.0, 90), (65, 2.0, 2.0, 90), (53, 4.0, 3.5, 110), (57, 4.0, 3.5, 110), (60, 4.0, 3.5, 110)];
const FOLLOWER_DISTANCE: f64 = -12.0;
const LANE_AMOUNT: f64 = 0.8;
const LANE_HZ: f64 = 1.5;
const LIVE_KEYS: [(usize, u8, Option<u8>); 9] = [(4_801, 60, Some(110)), (19_237, 64, Some(70)), (33_000, 67, Some(127)), (61_111, 60, None), (75_500, 72, Some(90)), (90_001, 64, None), (90_001, 67, None), (104_000, 72, None), (118_513, 55, Some(100))];
const LIVE_END: usize = 150_000;
const SCENES: [Scene; 7] = [
    ("the STEP line over one Core", || played(&project(vec![line_track()]))),
    ("the arp over a held chord", || played(&project(vec![chord_track(&[keys()])]))),
    ("the follower alone", || followed(true)),
    ("the source and its follower", || followed(false)),
    ("a NOTE-clocked lane", || laned(LfoClock::Note, LfoShape::Saw)),
    ("the arp over live keys", || live(arp_track(chord_arp(), &[keys()]))),
    ("the STEP line over live keys", || live(arp_track(line_arp(), &[lead()]))),
];

const FLAT_LINE: [Printed; 28] = [
    (0.0, 0.12, 57, 110, 0.0, Some(0.0), 0, Join::Play),
    (0.3, 0.35, 64, 77, 0.0, Some(0.5), 7, Join::Play),
    (0.5, 0.8, 69, 99, -20.0, Some(-0.3), 12, Join::Play),
    (0.8, 0.94, 67, 99, 0.0, Some(0.8), 10, Join::Tie),
    (1.0, 1.105, 45, 110, 0.0, Some(-1.0), -12, Join::Play),
    (1.5, 1.8, 60, 66, 15.0, Some(0.2), 3, Join::Play),
    (1.8, 1.89, 62, 66, 0.0, Some(0.6), 5, Join::Slide),
    (2.0, 2.09, 69, 110, 0.0, Some(0.0), 12, Join::Play),
    (2.3, 2.5, 52, 83, 0.0, Some(-0.5), -5, Join::Play),
    (2.5, 2.665, 52, 83, 0.0, Some(1.0), -5, Join::Tie),
    (2.8, 2.84, 59, 105, 30.0, Some(0.1), 2, Join::Play),
    (3.0, 3.24, 52, 72, 0.0, Some(-0.7), -5, Join::Play),
    (3.5, 3.65, 69, 110, -40.0, Some(0.4), 12, Join::Play),
    (3.8, 3.83, 79, 61, 0.0, Some(-0.2), 22, Join::Play),
    (4.0, 4.12, 60, 90, 0.0, Some(0.0), 0, Join::Play),
    (4.3, 4.35, 67, 63, 0.0, Some(0.5), 7, Join::Play),
    (4.5, 4.8, 72, 81, -20.0, Some(-0.3), 12, Join::Play),
    (4.8, 4.94, 70, 81, 0.0, Some(0.8), 10, Join::Tie),
    (5.0, 5.105, 48, 90, 0.0, Some(-1.0), -12, Join::Play),
    (5.5, 5.8, 63, 54, 15.0, Some(0.2), 3, Join::Play),
    (5.8, 5.89, 65, 54, 0.0, Some(0.6), 5, Join::Slide),
    (6.0, 6.09, 72, 90, 0.0, Some(0.0), 12, Join::Play),
    (6.3, 6.5, 55, 68, 0.0, Some(-0.5), -5, Join::Play),
    (6.5, 6.665, 55, 68, 0.0, Some(1.0), -5, Join::Tie),
    (6.8, 6.84, 62, 86, 30.0, Some(0.1), 2, Join::Play),
    (7.0, 7.24, 55, 59, 0.0, Some(-0.7), -5, Join::Play),
    (7.5, 7.65, 72, 90, -40.0, Some(0.4), 12, Join::Play),
    (7.8, 7.83, 82, 50, 0.0, Some(-0.2), 22, Join::Play),
];

fn arp_track(arp: Arp, layers: &[Layer]) -> Track {
    let rack = std::iter::once(struck(arp)).chain(layers.iter().map(|layer| Row::Core(*layer))).collect();
    Track { rack, ..Track::blank() }
}

fn following(source: usize, follower: Track) -> Track {
    Track { follows: Some(source), ..follower }
}

fn printed(arp: Arp, song: &[SongNote]) -> Vec<SongNote> {
    arp_print(song, arp, DEFAULT_BPM)
}

fn core_row(track: &Track) -> usize {
    track.rack.iter().position(Row::is_core).expect("wanted a Core row under the arp")
}

fn line_arp() -> Arp {
    let mut pattern = Pattern::DEFAULT;
    pattern.set_length(LINE.len());
    for (index, &(semitone, octave, cents, velocity, gate, modulation, join)) in LINE.iter().enumerate() {
        pattern.put(index, Step { note: arp::RUN, octave, rest: LINE_RESTS.contains(&index), semitone, cents, velocity, gate, modulation, join });
    }
    Arp { on: true, stepped: true, retrigger: false, rate: Division::Sixteenth, swing: LINE_SWING, gate: LINE_GATE, pattern, ..Arp::DEFAULT }
}

fn chord_arp() -> Arp {
    let mut pattern = Pattern::DEFAULT;
    pattern.set_length(RUN_STEPS.len());
    for (index, &(note, octave, modulation)) in RUN_STEPS.iter().enumerate() {
        pattern.set(Lane::Note, index, note);
        pattern.set(Lane::Octave, index, octave);
        pattern.set(Lane::Mod, index, modulation);
    }
    pattern.set_rest(RUN_REST, true);
    Arp { on: true, retrigger_each_note: true, style: Style::ConDiverge, rate: Division::Twelfth, gate: CHORD_GATE, steps: COPIES, stack: STACK, pattern, ..Arp::DEFAULT }
}

fn source_arp() -> Arp {
    Arp { on: true, style: Style::UpDown, rate: Division::Eighth, gate: 0.8, ..Arp::DEFAULT }
}

fn follower_arp() -> Arp {
    let mut arp = Arp { on: true, retrigger: false, style: Style::Random, rate: Division::Sixteenth, gate: 0.3, steps: 2, ..Arp::DEFAULT };
    arp.set_distance(FOLLOWER_DISTANCE);
    arp
}

fn lead() -> Layer {
    let mut patch = Params::DEFAULT;
    patch.osc1.level = 0.6;
    patch.osc2 = Osc { levels: [0.0, 1.0, 0.0, 0.0], step_share: 0.5, ..Osc::with_level(0.0) };
    patch.modulation = Modulation { level2: 0.8, tune2: 12.0, ..Modulation::DEFAULT };
    patch.amp = Envelope { attack_ms: 2.0, decay_ms: 120.0, sustain: 0.5, release_ms: 40.0, ..Envelope::DEFAULT };
    patch.mono.glide_ms = 60.0;
    core_layer(LEVEL, patch)
}

fn keys() -> Layer {
    let mut patch = Params::DEFAULT;
    patch.osc1 = Osc { levels: [0.4, 0.6, 0.0, 0.0], pulse_width: 0.3, ..Osc::with_level(0.5) };
    patch.osc2 = Osc { levels: [0.0, 0.0, 0.0, 1.0], ..Osc::with_level(0.0) };
    patch.modulation = Modulation { level2: 0.7, tune2: 7.0, ..Modulation::DEFAULT };
    patch.amp = Envelope { attack_ms: 3.0, decay_ms: 150.0, sustain: 0.4, release_ms: 60.0, ..Envelope::DEFAULT };
    patch.filter.on = true;
    patch.filter.cutoff_hz = 1800.0;
    patch.filter.resonance = 0.3;
    core_layer(LEVEL, patch)
}

fn under() -> Layer {
    let mut patch = Params::DEFAULT;
    patch.osc1 = Osc { levels: [0.0, 0.0, 1.0, 0.0], tune: -12.0, ..Osc::with_level(0.8) };
    patch.amp = Envelope { attack_ms: 10.0, decay_ms: 300.0, sustain: 0.7, release_ms: 120.0, ..Envelope::DEFAULT };
    core_layer(LEVEL, patch)
}

fn pluck() -> Layer {
    let mut patch = Params::DEFAULT;
    patch.osc1 = Osc { levels: [0.0, 0.0, 0.5, 0.5], ..Osc::with_level(0.9) };
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 90.0, sustain: 0.0, release_ms: 50.0, ..Envelope::DEFAULT };
    core_layer(LEVEL, patch)
}

fn clip(keys: &Keys) -> Clip {
    let notes = keys.iter().map(|&(pitch, start, length, velocity)| Note { pitch, start, length, velocity, articulation: Articulation::PLAIN }).collect();
    Clip { start: 0, length: BEATS as i32 * TICKS_PER_BEAT, label: String::new(), notes, file: None }
}

fn playing(track: Track, keys: &Keys) -> Track {
    Track { clips: vec![clip(keys)], ..track }
}

fn line_track() -> Track {
    playing(arp_track(line_arp(), &[lead()]), &LINE_KEYS)
}

fn chord_track(layers: &[Layer]) -> Track {
    playing(arp_track(chord_arp(), layers), &CHORD_KEYS)
}

fn project(tracks: Vec<Track>) -> Project {
    let mut project = Project::new();
    project.tracks = tracks;
    project
}

struct Session {
    audio: Audio,
    renderer: Renderer,
    left: Vec<f32>,
    right: Vec<f32>,
}

impl Session {
    fn open(project: &Project) -> Session {
        let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
        renderer.patient();
        renderer.start_workers(0, || None);
        audio.sync(project, None, &[], &[]);
        Session { audio, renderer, left: Vec::new(), right: Vec::new() }
    }

    fn until(&mut self, end: usize) {
        let (mut l, mut r) = ([0.0; BLOCK], [0.0; BLOCK]);
        while self.left.len() < end {
            let n = BLOCK.min(end - self.left.len());
            self.renderer.render(&mut l[..n], &mut r[..n]);
            self.left.extend_from_slice(&l[..n]);
            self.right.extend_from_slice(&r[..n]);
        }
    }

    fn heard(&self, what: &str) {
        let peak = peak_of(&self.left).max(peak_of(&self.right));
        assert!(peak > AUDIBLE && peak < 1.0, "wanted {what} audible and under full scale, got a peak of {peak}");
    }

    fn parted(&self, other: &Session) -> (Option<usize>, Option<usize>) {
        (apart(&self.left, &other.left), apart(&self.right, &other.right))
    }
}

fn played(project: &Project) -> Session {
    let mut session = Session::open(project);
    session.audio.play(0.0);
    session.until((BEATS * FRAMES_PER_BEAT).round() as usize + frames(TAIL_SECONDS));
    session
}

fn live(track: Track) -> Session {
    let mut session = Session::open(&project(vec![track]));
    for (frame, key, struck) in LIVE_KEYS {
        session.until(frame);
        match struck {
            Some(velocity) => session.audio.note_on(0, key, velocity),
            None => session.audio.note_off(0, key),
        }
    }
    session.until(LIVE_END);
    session
}

fn followed(muted: bool) -> Session {
    let source = playing(arp_track(source_arp(), &[lead()]), &PROGRESSION);
    let follower = following(0, arp_track(follower_arp(), &[pluck()]));
    played(&project(vec![Track { mute: muted, ..source }, follower]))
}

fn laned(clock: LfoClock, shape: LfoShape) -> Session {
    let mut track = chord_track(&[keys()]);
    let row = core_row(&track);
    track.automation.lfos = vec![LaneLfo { amount: LANE_AMOUNT, shape, sync: false, rate_hz: LANE_HZ, clock, ..LaneLfo::blank(Aim::knob(row, "OSC 1", "LEVEL")) }];
    played(&project(vec![track]))
}

fn summed(a: &[f32], b: &[f32]) -> Vec<f32> {
    a.iter().zip(b).map(|(x, y)| x + y).collect()
}

fn apart(got: &[f32], wanted: &[f32]) -> Option<usize> {
    got.iter().zip(wanted).position(|(x, y)| x.to_bits() != y.to_bits()).or_else(|| (got.len() != wanted.len()).then(|| got.len().min(wanted.len())))
}

fn spelled(note: &SongNote) -> Printed {
    let Articulation { cents, modulation, shift, join } = note.articulation;
    (note.start, note.end, note.pitch, note.velocity, cents, modulation, shift, join)
}

#[test]
fn every_arp_scene_is_heard_and_renders_the_same_twice() {
    for (what, scene) in SCENES {
        let (first, second) = (scene(), scene());
        first.heard(what);
        let parted = first.parted(&second);
        assert!(parted == (None, None), "wanted {what} to render bit for bit the same twice, got the left and right channels apart from frames {parted:?}");
    }
}

#[test]
fn one_arp_over_two_cores_sounds_as_each_core_alone_summed() {
    let layered = played(&project(vec![chord_track(&[keys(), under()])]));
    layered.heard("one arp over two Cores in one rack");
    let alone: Vec<Session> = [keys(), under()].iter().map(|layer| played(&project(vec![chord_track(&[*layer])]))).collect();
    let parted = (apart(&layered.left, &summed(&alone[0].left, &alone[1].left)), apart(&layered.right, &summed(&alone[0].right, &alone[1].right)));
    assert!(parted == (None, None), "wanted two Cores under one arp to sound bit for bit as each Core alone summed, got the left and right channels apart from frames {parted:?}");
}

#[test]
fn flattening_the_step_line_prints_its_notes() {
    let got: Vec<Printed> = printed(line_arp(), &line_track().song_notes()).iter().map(spelled).collect();
    let lines: Vec<String> = got.iter().map(|(start, end, pitch, velocity, cents, modulation, shift, join)| format!("    ({start:?}, {end:?}, {pitch}, {velocity}, {cents:?}, {modulation:?}, {shift}, Join::{join:?}),")).collect();
    assert!(got == FLAT_LINE, "wanted FLAT to print the STEP line's notes, got these {} notes as fixture lines:\n{}", got.len(), lines.join("\n"));
}

#[test]
fn the_flattened_line_plays_back_as_the_arp_did() {
    let line = line_track();
    let notes = printed(line_arp(), &line.song_notes());
    let flat = Track { clips: flatten::print(&line.clips, &notes), ..arp_track(Arp { on: false, ..line_arp() }, &[lead()]) };
    let (arped, flattened) = (played(&project(vec![line])), played(&project(vec![flat])));
    arped.heard("the STEP line through its arp");
    let parted = flattened.parted(&arped);
    assert!(parted == (None, None), "wanted FLAT's notes, played back with the arp off, to sound bit for bit as the STEP line does through its arp, got the left and right channels apart from frames {parted:?}");
}
