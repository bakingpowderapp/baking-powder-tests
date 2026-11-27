use bp::app::App;
use bp::arp::{Arp, Articulation, Style};
use bp::audio::{self, Audio};
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::core::params::{Envelope, Osc};
use bp::division::Division;
use bp::factory::PRESETS;
use bp::flatten::{self, Flattened};
use bp::fx::{Kind, Slot};
use bp::json;
use bp::project::{Bus, Clip, DEFAULT_BPM, Document, Host, Note, Preset, Project, Row, TICKS_PER_BEAT};
use support::{AUDIBLE, BLOCK, FRAMES_PER_BEAT, SAMPLE_RATE, call, core_layer, frames, midi, peak_of, remembered, song_render, struck, unseeded, worst_delta};

const BEATS: f64 = 4.0;
const TAIL_SECONDS: f64 = 0.5;
const KEYS: [(u8, f32, f32, u8); 3] = [(60, 0.0, 4.0, 100), (64, 0.0, 4.0, 90), (67, 1.0, 3.0, 110)];
const LEVEL: f64 = 0.3;
const LEVELS: [f64; 2] = [0.5, 0.25];
const ALIKE: f32 = 1e-5;
const FACTORY_ARP_ROWS: usize = 27;
const WHOLE_PATTERN_BEATS: f64 = 16.0;
const SHORT_BEATS: f64 = 8.0;
const OTHER_BPM: f64 = 97.0;
const LOOP: (f64, f64) = (2.0, 6.0);
const LOOP_OFF_THE_BEAT: (f64, f64) = (1.5, 6.5);
const START: f64 = 2.0;
const START_OFF_THE_BEAT: f64 = 1.5;
const HELD: [(u8, f32, f32, u8); 3] = [(60, 0.0, 16.0, 100), (64, 0.0, 16.0, 90), (67, 1.0, 15.0, 110)];
const MOVING: [(u8, f32, f32, u8); 12] = [
    (60, 0.0, 2.25, 100),
    (64, 0.0, 2.25, 90),
    (67, 0.25, 2.0, 110),
    (65, 2.25, 2.35, 95),
    (69, 2.3, 2.3, 85),
    (72, 2.25, 2.0, 105),
    (67, 4.5, 2.25, 100),
    (71, 4.6, 2.0, 80),
    (74, 4.5, 2.5, 120),
    (57, 6.75, 1.25, 90),
    (60, 6.8, 1.2, 100),
    (64, 6.75, 1.0, 70),
];
const SINGLE: [(u8, f32, f32, u8); 8] = [(60, 0.0, 0.75, 40), (62, 0.5, 1.0, 127), (67, 2.0, 0.4, 70), (55, 3.1, 1.7, 100), (72, 4.0, 0.3, 20), (64, 6.0, 1.5, 90), (65, 6.75, 1.0, 110), (48, 7.3, 0.7, 64)];

fn above() -> Arp {
    Arp { on: true, style: Style::Up, rate: Division::Eighth, gate: 0.75, ..Arp::DEFAULT }
}

fn below() -> Arp {
    Arp { on: true, style: Style::Down, rate: Division::Sixteenth, gate: 0.5, steps: 2, ..Arp::DEFAULT }
}

fn flat(arp: Arp) -> Row {
    struck(Arp { on: false, ..arp })
}

fn layer(saw: f64, sine: f64, decay_ms: f64) -> Row {
    let mut patch = Params::DEFAULT;
    patch.osc1 = Osc { levels: [saw, 0.0, 0.0, sine], ..Osc::with_level(0.8) };
    patch.amp = Envelope { attack_ms: 2.0, decay_ms, sustain: 0.4, release_ms: 20.0, ..Envelope::DEFAULT };
    Row::Core(core_layer(LEVEL, patch))
}

fn high() -> Row {
    layer(0.0, 1.0, 200.0)
}

fn low() -> Row {
    layer(1.0, 0.0, 90.0)
}

fn bus(levels: [f64; 2]) -> Row {
    Row::Bus(Bus { levels, ..Bus::DEFAULT })
}

fn playing(rack: Vec<Row>) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = rack;
    let notes = KEYS.iter().map(|&(pitch, start, length, velocity)| Note { pitch, start, length, velocity, articulation: Articulation::PLAIN }).collect();
    project.tracks[0].clips.push(Clip { start: 0, length: BEATS as i32 * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    let text = project.to_json().write();
    Project::from_json(&json::parse(&text).expect("wanted the song to parse")).expect("wanted the song to read")
}

fn open(project: &Project) -> App {
    let session = format!("{{\"project\":{},\"keyboard\":false,\"palette\":false}}", project.to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.take_requests();
    app
}

fn flattened(app: &mut App, row: usize) -> String {
    let reply = call(app, "write", &format!(r#"{{"path":"tracks/0/rack/{row}","flatten":true}}"#));
    assert!(!reply.failed, "wanted flatten taken at tracks/0/rack/{row}, got {}", reply.text);
    reply.text
}

fn refused(app: &mut App, path: &str) -> String {
    let reply = call(app, "write", &format!(r#"{{"path":"{path}","flatten":true}}"#));
    assert!(reply.failed, "wanted flatten refused at {path}, got {}", reply.text);
    reply.text
}

fn operation(reply: &str) -> &str {
    reply.lines().skip_while(|line| *line != "Operations:").nth(1).map_or("", str::trim)
}

fn printed(project: &Project, track: usize) -> usize {
    project.tracks[track].clips.iter().map(|clip| clip.notes.len()).sum()
}

fn racks(project: &Project) -> Vec<Vec<Row>> {
    project.tracks.iter().map(|track| unseeded(&track.rack)).collect()
}

fn clips(project: &Project) -> Vec<Vec<Clip>> {
    project.tracks.iter().map(|track| track.clips.clone()).collect()
}

fn end() -> usize {
    (BEATS * FRAMES_PER_BEAT).round() as usize + frames(TAIL_SECONDS)
}

fn sounds_as_before(landed: &Project, before: &Project, what: &str) {
    let (got, wanted) = (song_render(landed, end()), song_render(before, end()));
    let peak = peak_of(&got[0]).max(peak_of(&got[1]));
    assert!(peak > AUDIBLE, "wanted {what} heard after the flatten, got a peak of {peak}");
    let worst = worst_delta(&got[0], &wanted[0]).max(worst_delta(&got[1], &wanted[1]));
    assert!(worst < ALIKE, "wanted {what} to sound as the rack did before the flatten within {ALIKE}, got a worst delta of {worst}");
}

#[test]
fn flatten_at_the_first_midi_row_prints_what_it_plays_as_flatten_on_its_face_does() {
    let project = playing(vec![midi(above()), high()]);
    let mut wanted = project.clone();
    assert_eq!(flatten::row(&mut wanted, 0, 0, &audio::sent(&project)), Flattened::Printed, "wanted the row's own flatten to print in place");
    let mut app = open(&project);
    let reply = flattened(&mut app, 0);
    assert_eq!(operation(&reply), format!("Flattened tracks/0/rack/0: {} notes printed over 1 clip, its arp off, velocity 127 and transposeSemitones 0", printed(&wanted, 0)), "wanted the reply to count the notes printed, got {reply}");
    let landed = remembered(&mut app);
    assert_eq!(racks(&landed), vec![vec![flat(above()), high()]], "wanted the row left with its arp off and its velocity at 127");
    assert_eq!(clips(&landed), clips(&wanted), "wanted the clip to hold the notes the row's own flatten prints");
    assert!(printed(&landed, 0) > KEYS.len(), "wanted the arp's line printed, more notes than the chord held");
}

#[test]
fn flatten_at_a_lower_midi_row_moves_it_with_its_cores_onto_a_track_the_reply_names() {
    let project = playing(vec![midi(above()), high(), midi(below()), low()]);
    let mut wanted = project.clone();
    assert_eq!(flatten::row(&mut wanted, 0, 2, &audio::sent(&project)), Flattened::Split { removed: 0b1100, copied: false, muted: false }, "wanted the row's own flatten to move two rows onto a new track");
    let mut app = open(&project);
    let reply = flattened(&mut app, 2);
    assert_eq!(operation(&reply), format!("Flattened tracks/0/rack/2 onto a new track, tracks/1 MIDI 2: {} notes printed over 1 clip, 2 rows moved there off tracks/0", printed(&wanted, 1)), "wanted the reply to name the new track and count what moved, got {reply}");
    let landed = remembered(&mut app);
    assert_eq!(racks(&landed), vec![vec![midi(above()), high()], vec![flat(below()), low()]], "wanted the first arp and its Core left and the flattened row with its Core on the new track");
    assert_eq!(clips(&landed), clips(&wanted), "wanted both tracks' clips as the row's own flatten leaves them");
    assert_eq!(landed.tracks[1].name, "MIDI 2", "wanted the new track named as the app names a new one");
    sounds_as_before(&landed, &project, "the two tracks");
}

#[test]
fn flatten_inside_a_bus_takes_that_buss_cores_alone_and_keeps_its_level() {
    let second = playing(vec![midi(Arp::DEFAULT), high(), bus(LEVELS), midi(below()), low()]);
    let mut app = open(&second);
    flattened(&mut app, 3);
    let landed = remembered(&mut app);
    assert_eq!(racks(&landed), vec![vec![midi(Arp::DEFAULT), high(), bus(LEVELS)], vec![flat(below()), low(), bus([LEVELS[1], 1.0])]], "wanted bus 2's MIDI row and Core moved, bus 1 left whole, and the new track's one bus at bus 2's level");
    sounds_as_before(&landed, &second, "bus 2 on a track of its own beside bus 1");
    let first = playing(vec![midi(Arp::DEFAULT), midi(above()), high(), bus(LEVELS), low()]);
    let mut app = open(&first);
    flattened(&mut app, 1);
    let landed = remembered(&mut app);
    assert_eq!(racks(&landed), vec![vec![midi(Arp::DEFAULT), bus(LEVELS), low()], vec![flat(above()), high(), bus([LEVELS[0], 1.0])]], "wanted bus 1's MIDI row and Core moved, bus 2's Core left, and the new track's one bus at bus 1's level");
    sounds_as_before(&landed, &first, "bus 1 on a track of its own beside bus 2");
}

#[test]
fn flatten_refuses_a_row_that_is_no_midi_row_one_with_its_arp_off_and_a_path_that_is_no_row() {
    let mut ducked = Slot::new(Kind::Sidechain);
    ducked.wet = 0.5;
    let project = playing(vec![midi(Arp::DEFAULT), high(), Row::Effect(ducked)]);
    let mut app = open(&project);
    let core = refused(&mut app, "tracks/0/rack/1");
    assert!(core.contains("is a core row; flatten takes a midi row"), "wanted a Core row refused, got {core}");
    let sidechain = refused(&mut app, "tracks/0/rack/2");
    assert!(sidechain.contains("is a sidechain row; flatten takes a midi row") && sidechain.contains("the user presses it in the app"), "wanted a sidechain row refused, its own FLATTEN left to the user, got {sidechain}");
    let off = refused(&mut app, "tracks/0/rack/0");
    assert!(off.contains("can't be flattened: Arp is off"), "wanted a MIDI row with its arp off refused with the words its FLAT cell shows, got {off}");
    let track = refused(&mut app, "tracks/0");
    assert!(track.contains("flatten takes a midi row like tracks/N/rack/R"), "wanted a track's own path refused, got {track}");
    let before = call(&mut app, "read", "{}").text;
    refused(&mut app, "tracks/0/rack/0");
    assert_eq!(call(&mut app, "read", "{}").text, before, "wanted a refused flatten to leave the song as it was");
}

fn landed(text: &str) -> Option<Project> {
    let Some(Document::Preset(preset @ Preset::Rack(_))) = json::parse(text).and_then(|json| Document::from_json(&json)) else {
        return None;
    };
    let mut project = playing(vec![midi(Arp::DEFAULT), high()]);
    let plan = project.plan(Host::Track(0), &preset, None, false).ok()?;
    project.land(Host::Track(0), preset, plan, None, ("", ""));
    Some(project)
}

fn lit(project: &Project) -> Vec<usize> {
    project.tracks[0].rack.iter().enumerate().filter(|(_, row)| matches!(row, Row::Midi(midi) if midi.heard().on)).map(|(at, _)| at).collect()
}

#[test]
fn flatten_prints_every_factory_arp_row() {
    let mut rows = 0;
    for (path, text) in PRESETS {
        let Some(project) = landed(text) else {
            continue;
        };
        let sent = audio::sent(&project);
        for row in lit(&project) {
            rows += 1;
            let got = flatten::row(&mut project.clone(), 0, row, &sent);
            assert!(matches!(got, Flattened::Printed | Flattened::Split { .. }), "wanted {path}'s arp row {row} printed, got {got:?}");
        }
    }
    assert_eq!(rows, FACTORY_ARP_ROWS, "wanted {FACTORY_ARP_ROWS} lit arp rows across the factory racks, got {rows}");
}

type Keys = [(u8, f32, f32, u8)];

struct Playback {
    keys: &'static Keys,
    beats: f64,
    bpm: f64,
    looped: Option<(f64, f64)>,
    start: f64,
}

impl Playback {
    const fn through(keys: &'static Keys, beats: f64) -> Playback {
        Playback { keys, beats, bpm: DEFAULT_BPM, looped: None, start: 0.0 }
    }

    fn song(&self, mut project: Project) -> Project {
        let notes = self.keys.iter().map(|&(pitch, start, length, velocity)| Note { pitch, start, length, velocity, articulation: Articulation::PLAIN }).collect();
        project.tracks[0].clips = vec![Clip { start: 0, length: (self.beats * f64::from(TICKS_PER_BEAT)).round() as i32, label: String::new(), notes, file: None }];
        project.bpm = self.bpm;
        project
    }

    fn frames(&self) -> usize {
        let played = self.looped.map_or(self.beats - self.start, |(start, end)| end + 1.5 * (end - start));
        (played * SAMPLE_RATE * 60.0 / self.bpm).round() as usize + frames(TAIL_SECONDS)
    }

    fn render(&self, project: &Project) -> [Vec<f32>; 2] {
        if self.looped.is_none() && self.start == 0.0 {
            return song_render(project, self.frames());
        }
        let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
        renderer.patient();
        renderer.start_workers(0, || None);
        audio.sync(project, None, &[], &[]);
        audio.set_loop(self.looped);
        audio.play(self.start);
        let total = self.frames();
        let (mut left, mut right) = (Vec::with_capacity(total), Vec::with_capacity(total));
        let (mut l, mut r) = ([0.0; BLOCK], [0.0; BLOCK]);
        while left.len() < total {
            let n = BLOCK.min(total - left.len());
            renderer.render(&mut l[..n], &mut r[..n]);
            left.extend_from_slice(&l[..n]);
            right.extend_from_slice(&r[..n]);
        }
        [left, right]
    }
}

fn every_factory_arp_row_sounds_as_before_once_flattened_over(playback: &Playback, what: &str) {
    let mut misses = Vec::new();
    for (path, text) in PRESETS {
        let Some(project) = landed(text).map(|project| playback.song(project)) else {
            continue;
        };
        let rows = lit(&project);
        if rows.is_empty() {
            continue;
        }
        let before = playback.render(&project);
        let peak = peak_of(&before[0]).max(peak_of(&before[1]));
        assert!(peak > AUDIBLE, "wanted {path} heard before the flatten over {what}, got a peak of {peak}");
        let sent = audio::sent(&project);
        for row in rows {
            let mut flat = project.clone();
            let got = flatten::row(&mut flat, 0, row, &sent);
            let after = playback.render(&flat);
            let worst = worst_delta(&after[0], &before[0]).max(worst_delta(&after[1], &before[1]));
            if worst >= ALIKE {
                let from = (0..after[0].len()).find(|&at| (0..2).any(|side| (after[side][at] - before[side][at]).abs() >= ALIKE));
                misses.push((path, row, got, worst, from));
            }
        }
    }
    assert!(misses.is_empty(), "wanted every factory arp row over {what} to sound within {ALIKE} of itself once flattened, got these misses with their FLATTEN, worst delta and first frame apart: {misses:?}");
}

#[test]
fn every_factory_arp_row_sounds_as_before_once_flattened() {
    every_factory_arp_row_sounds_as_before_once_flattened_over(&Playback::through(&HELD, WHOLE_PATTERN_BEATS), "a held chord");
}

#[test]
fn every_factory_arp_row_sounds_as_before_once_flattened_over_moving_chords() {
    every_factory_arp_row_sounds_as_before_once_flattened_over(&Playback::through(&MOVING, SHORT_BEATS), "chords changing off the beat");
}

#[test]
fn every_factory_arp_row_sounds_as_before_once_flattened_over_single_notes() {
    every_factory_arp_row_sounds_as_before_once_flattened_over(&Playback::through(&SINGLE, SHORT_BEATS), "single notes overlapping and apart");
}

#[test]
fn every_factory_arp_row_sounds_as_before_once_flattened_at_another_tempo() {
    every_factory_arp_row_sounds_as_before_once_flattened_over(&Playback { bpm: OTHER_BPM, ..Playback::through(&MOVING, SHORT_BEATS) }, "chords changing at another tempo");
}

#[test]
fn every_factory_arp_row_sounds_as_before_once_flattened_through_a_loop() {
    every_factory_arp_row_sounds_as_before_once_flattened_over(&Playback { looped: Some(LOOP), ..Playback::through(&MOVING, SHORT_BEATS) }, "a loop wrapping twice");
}

#[test]
fn every_factory_arp_row_sounds_as_before_once_flattened_through_a_loop_off_the_beat() {
    every_factory_arp_row_sounds_as_before_once_flattened_over(&Playback { looped: Some(LOOP_OFF_THE_BEAT), ..Playback::through(&MOVING, SHORT_BEATS) }, "a loop off the beat wrapping twice");
}

#[test]
fn every_factory_arp_row_sounds_as_before_once_flattened_played_from_mid_song() {
    every_factory_arp_row_sounds_as_before_once_flattened_over(&Playback { start: START, ..Playback::through(&MOVING, SHORT_BEATS) }, "play started mid song");
}

#[test]
fn every_factory_arp_row_sounds_as_before_once_flattened_played_from_off_the_beat() {
    every_factory_arp_row_sounds_as_before_once_flattened_over(&Playback { start: START_OFF_THE_BEAT, ..Playback::through(&MOVING, SHORT_BEATS) }, "play started off the beat");
}
