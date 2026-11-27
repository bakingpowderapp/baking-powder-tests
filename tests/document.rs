use bp::arp::{Arp, Articulation, MidiRow, Style, Ties};
use bp::audio::Tail;
use bp::core::{FilterMode, FilterShape, Params, Slope};
use bp::division::Division;
use bp::dsp::noise::NoiseColor;
use bp::project::warp::Loop;
use bp::face::Drawn;
use bp::fx::{self, Kind, Slot};
use bp::json;
use bp::project::{Aim, Clip, Curve, Fade, Fades, File, LaneLfo, LfoClock, LfoField, LfoShape, Node, Note, PaletteColumn, Preset, Project, Row, Span, TICKS_PER_BAR, TICKS_PER_BEAT, Track, VELOCITY};
use bp::theory::{ChordVariation, Root, ScaleName};
use bp::wav::Wave;
use support::core_layer;

const QUIET: f32 = 0.05;

fn core_row() -> Row {
    let mut patch = Params::DEFAULT;
    patch.voices = 1;
    patch.osc1.fm = 0.3;
    patch.osc1.sync = true;
    patch.noise.tone = -0.25;
    patch.osc1.tune = 7.0;
    patch.osc1.fine = -12.0;
    patch.osc1.unison = 4;
    patch.osc1.detune = 18.0;
    patch.osc1.pulse_width = 0.3;
    patch.osc2.level = 0.6;
    patch.osc2.levels[1] = 1.0;
    patch.amp.attack_ms = 2.0;
    patch.amp.release_ms = 30.0;
    patch.amp.decay_bend = 0.5;
    patch.filter.on = true;
    patch.filter.mode = FilterMode::Ladder;
    patch.filter.shape = FilterShape::HighPass;
    patch.filter.slope = Slope::Db12;
    patch.filter.cutoff_hz = 640.0;
    patch.filter.env_amount = -0.5;
    patch.filter.key_track = 1.2;
    patch.filter.env.decay_ms = 90.0;
    patch.noise.on = true;
    patch.noise.color = NoiseColor::Pink;
    patch.analog.detune = 6.0;
    patch.mono.glide_ms = 80.0;
    patch.mono.legato = false;
    Row::Core(core_layer(0.8, patch))
}

fn midi_row() -> Row {
    Row::Midi(MidiRow { arp: Arp { on: true, style: Style::Down, rate: Division::Sixteenth, sync: false, time_ms: 120.0, gate: 1.5, ..Arp::DEFAULT }, ..MidiRow::DEFAULT })
}

fn passing_midi_row() -> Row {
    Row::Midi(MidiRow { bypass: true, times: 2, velocity: 64, transpose: -5, force_key: true, ties: Ties::Smart, arp: Arp { on: true, steps: 2, ..Arp::DEFAULT } })
}

fn slot(kind: Kind, tweak: impl FnOnce(&mut fx::Params)) -> Row {
    let mut slot = Slot::new(kind);
    slot.wet = 0.6;
    tweak(&mut slot.params);
    Row::Effect(slot)
}

fn effect_rows() -> Vec<Row> {
    Kind::ALL
        .iter()
        .map(|&kind| {
            slot(kind, |params| match params {
                fx::Params::Reverb(p) => {
                    p.decay_s = 3.2;
                    p.shimmer = 0.4;
                    p.mod_sync = true;
                }
                fx::Params::Delay(p) => {
                    p.sync_l = true;
                    p.division_l = Division::Eighth;
                    p.ducking = -0.5;
                    p.splice = 0.5;
                    p.hold = true;
                }
                fx::Params::Freak(p) => {
                    p.band_count = 3;
                    p.drive_db = 4.0;
                    p.crossovers_hz[1] = 1500.0;
                    p.bands[1].curve = Some(fx::params::Curve::Tube);
                    p.bands[1].drive_db = 9.0;
                    p.bands[2].order = fx::params::freak::Order::Post;
                    p.bands[2].squeeze.ratio = 6.0;
                }
                fx::Params::Gate(p) => {
                    p.signal.source = 4.0;
                    p.signal.pre = true;
                    p.threshold_db = -20.0;
                    p.return_db = 6.0;
                    p.hold_ms = 40.0;
                    p.flip = true;
                }
                fx::Params::Compressor(p) => {
                    p.threshold_db = -18.0;
                    p.auto = false;
                }
                fx::Params::Sidechain(p) => {
                    p.signal.source = 1.0;
                    p.signal.pre = true;
                }
                fx::Params::Motion(p) => {
                    p.band_count = 3;
                    p.bands[1].mode = fx::params::MotionMode::Chorus;
                    p.bands[1].chorus.depth = 1.5;
                }
                fx::Params::Eq(p) => {
                    p.bands[3].on = true;
                    p.bands[3].gain_db = -4.0;
                    p.scale = 1.5;
                    p.pass.on = true;
                    p.pass.bands[1].level = 0.5;
                    p.pass.bands[2].q = 20.0;
                    p.pass.trim_db = -3.0;
                }
                fx::Params::Utility(p) => {
                    p.limit = true;
                    p.ceiling_db = -0.5;
                    p.width = 1.4;
                }
                fx::Params::Transient(p) => {
                    p.attack_db = 6.0;
                    p.sustain_db = -6.0;
                    p.clip = true;
                }
                fx::Params::VolumeLfo(p) => {
                    p.sync = false;
                    p.rate_hz = 2.0;
                    p.phase = 0.3;
                }
                fx::Params::Space(p) => {
                    p.depth_db = 9.0;
                    p.q = 6.0;
                    p.low_hz = 40.0;
                }
                fx::Params::Suppressor(p) => {
                    p.band_count = 2;
                    p.bands[0].depth = 7.0;
                    p.bands[1].detail = 8.0;
                }
                fx::Params::Time(p) => {
                    p.play = false;
                    p.stop_ms = 900.0;
                    p.curve = 2.0;
                }
                fx::Params::Repeat(p) => {
                    p.interval = Division::Half;
                    p.variation = 0.6;
                    p.mix = fx::params::RepeatMix::Insert;
                }
            })
        })
        .collect()
}

fn lane(aim: Aim, lfo: Option<LfoField>) -> Curve {
    Curve { aim, lfo, nodes: vec![Node { tick: 0, travel: 0.2, bend: 0.0 }, Node { tick: TICKS_PER_BAR, travel: 0.9, bend: -0.5 }, Node { tick: 2 * TICKS_PER_BAR, travel: 0.5, bend: 0.0 }] }
}

fn project() -> Project {
    let mut effects = effect_rows();
    let returns = effects.split_off(12);
    let mastered = effects.split_off(10);
    let audible = effects.split_off(8);
    let mut group = Track::group();
    group.name = String::from("GROUP 1");
    group.folded = true;
    let mut lead = Track::blank();
    lead.name = String::from("MIDI 1");
    lead.inside = true;
    lead.rack = [midi_row(), core_row(), passing_midi_row()].into_iter().chain(effects).collect();
    lead.sends[0] = 0.5;
    lead.solo = true;
    lead.gain = 1.3;
    let notes = vec![Note { pitch: 60, start: 0.0, length: 1.0, velocity: 100, articulation: Articulation::PLAIN }, Note { pitch: 64, start: 1.0, length: 7.0, velocity: VELOCITY, articulation: Articulation::PLAIN }, Note { pitch: 67, start: 2.5, length: 0.5, velocity: 64, articulation: Articulation::PLAIN }];
    lead.clips.push(Clip { start: 0, length: 8 * TICKS_PER_BEAT, label: String::from("I"), notes, file: None });
    lead.automation.curves = vec![lane(Aim::knob(0, "ARP", "GATE"), None), lane(Aim::knob(1, "FILTER", "CUTOFF"), None), lane(Aim::Gain, None), lane(Aim::knob(1, "OSC 1", "LEVEL"), Some(LfoField::Amount))];
    lead.automation.lfos = vec![LaneLfo { amount: 0.4, shape: LfoShape::Triangle, sync: false, division: Division::Quarter, rate_hz: 3.0, phase: 0.25, centre: 0.6, retrigger: true, legato: true, once: false, clock: LfoClock::Note, audio: false, fade_ms: 120.0, lag_ms: 40.0, points: Drawn::RAMP, ..LaneLfo::blank(Aim::knob(1, "OSC 1", "LEVEL")) }];
    let mut take = Track::audio();
    take.name = String::from("AUDIO 1");
    take.inside = true;
    take.mute = true;
    take.rack = audible;
    let file = File { name: String::from("kick.wav"), head: 0.1, offset: -0.05, tail: 0.2, pitch: -2.0, fine: 15.0, level: -3.0, spans: vec![Span { from: 0.0, to: 0.5, level: -6.0 }], fades: Fades { rise: Some(Fade { from: 0.0, to: 0.02, bend: 0.5 }), fall: Some(Fade { from: 0.3, to: 0.4, bend: -0.25 }) }, tempo: 128.0, warp: true, gap: Loop::Forward, envelope: 40.0 };
    take.clips.push(Clip { start: 8 * TICKS_PER_BEAT, length: TICKS_PER_BAR, label: String::from("take"), notes: Vec::new(), file: Some(file) });
    let mut pad = Track::blank();
    pad.name = String::from("MIDI 2");
    pad.sends[15] = 0.25;
    pad.clips.push(Clip::chord(TICKS_PER_BAR, String::from("V"), &[55, 59, 62]));
    let mut project = Project::new();
    project.tracks = vec![group, lead, take, pad];
    project.root = Root::Eb;
    project.scale = ScaleName::Dorian;
    project.bpm = 97.5;
    project.metronome = true;
    project.lock = true;
    project.master.gain = 0.9;
    project.master.rack = mastered;
    project.sends[0].rack = returns;
    project.sends[15].gain = 0.8;
    project.decks[1][3] = PaletteColumn { selected: ChordVariation::Seventh, parallel: true, sharp3: false, flat7: true, bass_offset: 1, octave: -1, bottom: 1, top: 0, lift: 1 };
    project
}

fn reopened(project: &Project) -> Project {
    let saved = project.to_json().write();
    let json = json::parse(&saved).expect("a saved project is valid JSON");
    Project::from_json(&json).expect("a saved project opens")
}

#[test]
fn a_rich_project_saves_the_same_text_after_reopening() {
    let original = project();
    let text = original.to_json().write();
    let again = reopened(&original).to_json().write();
    let line = text.lines().zip(again.lines()).position(|(a, b)| a != b);
    assert_eq!(again, text, "the save changes on a second pass, first at line {line:?}: {:?}", line.map(|at| (text.lines().nth(at), again.lines().nth(at))));
}

#[test]
fn a_rich_project_bounces_the_same_bytes_after_reopening() {
    let original = project();
    let opened = reopened(&original);
    let first = support::bounce(&original, &[], (0, original.end()), Tail::Rings).expect("the original bounces");
    let second = support::bounce(&opened, &[], (0, opened.end()), Tail::Rings).expect("the reopened project bounces");
    let loudest = Wave::parse(String::from("bounce"), String::new(), &first).expect("the bounce reads as a wave").pcm.iter().map(|sample| sample.unsigned_abs()).max().unwrap_or(0);
    assert!(f32::from(loudest) > QUIET * f32::from(i16::MAX), "the bounce is near silent, its loudest sample {loudest} of {}", i16::MAX);
    let divergence = first.iter().zip(&second).position(|(a, b)| a != b);
    assert!(first.len() == second.len() && divergence.is_none(), "the reopened project bounces differently: byte {divergence:?}, lengths {} and {}", first.len(), second.len());
}

#[test]
fn presets_save_the_same_text_after_reopening() {
    let lead = &project().tracks[1];
    let rack = Preset::Rack(lead.sound());
    let rows = lead.rack[..4].iter().map(|row| Preset::Row(*row, Vec::new()));
    let chords = Preset::Chords(project().decks[1]);
    for preset in std::iter::once(rack).chain(rows).chain([chords]) {
        let text = preset.to_json().write();
        let json = json::parse(&text).expect("a saved preset is valid JSON");
        let again = Preset::from_json(&json).expect("a saved preset opens").to_json().write();
        assert_eq!(again, text, "a {} preset changes on a second pass", preset.name());
    }
}

#[test]
fn a_track_named_for_the_largest_number_reads_without_overflowing_the_next_one() {
    let mut project = Project::new();
    project.tracks[0].name = format!("MIDI {}", u32::MAX);
    let read = Project::from_json(&project.to_json());
    assert_eq!(read.map(|read| read.tracks[0].name.clone()), Some(project.tracks[0].name.clone()), "wanted a song whose track is named MIDI and the largest number a counter holds to read back whole");
}

#[test]
fn a_clip_read_with_both_notes_and_a_wave_is_an_audio_clip_and_writes_back_as_it_read() {
    let mut project = Project::new();
    project.tracks[0].clips.push(Clip::chord(0, String::new(), &[60, 64, 67]));
    let text = project.to_json().write();
    let both = text.replacen("\"notes\":", "\"wave\":{\"name\":\"take\"},\"notes\":", 1);
    assert_ne!(both, text, "wanted a wave written beside the clip's notes in the document's text");
    let read = Project::from_json(&json::parse(&both).expect("wanted the document to parse")).expect("wanted the document to read");
    let clip = &read.tracks[0].clips[0];
    assert!(clip.file.is_some() && clip.notes.is_empty(), "wanted a clip given a wave to read as an audio clip with no notes, got {} notes and a wave: {}", clip.notes.len(), clip.file.is_some());
    let again = Project::from_json(&read.to_json()).expect("wanted the written document to read");
    assert!(again == read, "wanted the song written and read again to be the song as first read");
}
