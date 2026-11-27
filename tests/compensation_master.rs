use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::fx::params::{Freak, FreakSlope};
use bp::fx::{self, Kind, Slot};
use bp::project::{Clip, Note, Project, Row, VELOCITY};
use support::{BLOCK, GAP, LEVEL, NO_END, Onsets, SAMPLE_RATE, WITHIN, beat_frame, core_layer, midi};

const NOTE_BEAT: usize = 1;
const FRAMES: usize = 36_000;
const LIMITER: usize = 71;
const LINEAR: usize = 431;
const EXACT: f32 = 1e-6;
const CLOSE: f32 = 1e-4;
const C4: u8 = 60;

fn limiter() -> Row {
    let mut slot = Slot::new(Kind::Utility);
    if let fx::Params::Utility(p) = &mut slot.params {
        p.limit = true;
        p.hard = false;
        p.ceiling_db = 0.0;
        p.lookahead_ms = 1.5;
    }
    Row::Effect(slot)
}

fn linear_crossover() -> Row {
    Row::Effect(Slot { params: fx::Params::Freak(Freak { slope: FreakSlope::Linear, oversample: false, ..Freak::DEFAULT }), ..Slot::new(Kind::Freak) })
}

fn snappy() -> Params {
    let mut patch = Params::DEFAULT;
    patch.amp.attack_ms = 1.0;
    patch.amp.release_ms = 1.0;
    patch
}

fn noted(master: Vec<Row>) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(LEVEL, snappy()))];
    project.tracks[0].clips.push(Clip::midi(0, String::from("note"), vec![Note { pitch: C4, start: NOTE_BEAT as f32, length: 1.0, velocity: VELOCITY, articulation: Articulation::PLAIN }], NO_END));
    project.master.rack = master;
    project.metronome = true;
    project
}

fn notes_only(project: &Project) -> Project {
    let mut quiet = project.clone();
    quiet.metronome = false;
    quiet
}

fn click_only(project: &Project) -> Project {
    let mut bare = project.clone();
    for track in &mut bare.tracks {
        track.clips.clear();
    }
    bare
}

fn render(project: &Project) -> (Vec<f32>, usize) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, None, &[], &[]);
    let output = audio.timing().output;
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(FRAMES + BLOCK);
    while out.len() < FRAMES {
        renderer.render(&mut l, &mut r);
        out.extend_from_slice(&l);
    }
    out.truncate(FRAMES);
    (out, output)
}

fn onsets(out: &[f32]) -> Vec<usize> {
    let mut onsets = Onsets::default();
    onsets.feed(out);
    onsets.at
}

fn onsets_above(out: &[f32], floor: f32) -> Vec<usize> {
    let (mut quiet, mut at) = (GAP, Vec::new());
    for (frame, sample) in out.iter().enumerate() {
        if sample.abs() <= floor {
            quiet += 1;
        } else {
            if quiet >= GAP {
                at.push(frame);
            }
            quiet = 0;
        }
    }
    at
}

fn on_grid(at: &[usize], wanted: &[usize]) -> bool {
    at.len() == wanted.len() && at.iter().zip(wanted).all(|(got, want)| got.abs_diff(*want) <= WITHIN)
}

fn first_apart(with: &[f32], without: &[f32], by: usize, within: f32) -> Option<(usize, f32, f32)> {
    without.iter().zip(&with[by..]).enumerate().find(|(_, (plain, late))| (*plain - *late).abs() > within).map(|(frame, (plain, late))| (frame, *plain, *late))
}

fn shifted_together(master: Vec<Row>, late: usize, within: f32, floor: f32, what: &str) {
    let n = beat_frame(NOTE_BEAT);
    let project = noted(master);
    let plain = noted(Vec::new());
    let (with, output) = render(&project);
    assert_eq!(output, late, "{what}: wanted the output latency at {late}, got {output}");
    let (without, _) = render(&plain);
    let apart = first_apart(&with, &without, late, within);
    assert_eq!(apart, None, "{what}: wanted the whole output shifted by {late}, got a first difference at (frame, plain, late) {apart:?}");
    let (notes, _) = render(&notes_only(&project));
    let (plain_notes, _) = render(&notes_only(&plain));
    let (at, plain_at) = (onsets_above(&notes, floor), onsets_above(&plain_notes, floor));
    assert!(plain_at.len() == 1 && plain_at[0].abs_diff(n) <= WITHIN, "{what}: wanted the plain note's onset at frame {n}, got {plain_at:?}");
    let wanted: Vec<usize> = plain_at.iter().map(|onset| onset + late).collect();
    assert!(on_grid(&at, &wanted), "{what}: wanted the note's onset at frame {wanted:?}, got {at:?}");
    let beats: Vec<usize> = (0..=NOTE_BEAT).map(|beat| beat_frame(beat) + late).collect();
    let at = onsets(&render(&click_only(&project)).0);
    assert!(on_grid(&at, &beats), "{what}: wanted the metronome at frames {beats:?}, got {at:?}");
}

#[test]
fn a_limiter_on_the_master_shifts_the_note_and_the_click_together() {
    shifted_together(vec![limiter()], LIMITER, EXACT, EXACT, "a limiter on the master");
}

#[test]
fn a_linear_crossover_on_the_master_shifts_the_note_and_the_click_together() {
    shifted_together(vec![linear_crossover()], LINEAR, CLOSE, CLOSE, "a linear crossover on the master");
}
