use bp::arp::Arp;
use bp::audio::{Audio, Renderer};
use bp::audio::vision::Seer;
use bp::fx::params::motion::MotionBand;
use bp::fx::params::{MotionMode, Phaser, Utility};
use bp::fx::{Kind, Params, Slot};
use bp::project::{Host, Project, RACK_ROWS, Row, VELOCITY};
use support::{AUDIBLE, BLOCK, CORE_ROW, HUSH_DB, LEVEL, SAMPLE_RATE, core_layer, first_difference, midi, one_band, peak};

const BLOCKS: usize = 240;
const SETTLE: usize = 24;
const SILENT: f32 = 1e-4;
const INSTALL_AT: usize = 40;
const FRONT_AT: usize = 80;
const MOVE_AT: usize = 110;
const REMOVE_AT: usize = 140;
const ROW_AT: usize = 170;
const REPLAY_AT: usize = 200;
const REORDER_AT: usize = 100;
const RESTORE_AT: usize = 170;
const HALF_WET: f64 = 0.5;
const TRIMS: usize = 10;
const QUIETER_DB: f64 = -6.0;
const HUSH_RATIO: f32 = 4.0;
const TRIM_DB: f64 = -1.0;
const SQUASH_DB: f64 = -40.0;
const SWEEP_HZ: f64 = 0.5;
const TICK_SECONDS: f32 = 0.05;

type Edit = fn(&mut Project, Place);
type Blocks = Vec<Vec<f32>>;

#[derive(Clone, Copy)]
enum Place {
    Master,
    Track,
}

fn quiet() -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(LEVEL, bp::core::Params::DEFAULT))];
    project
}

fn chain(project: &mut Project, place: Place) -> (&mut Vec<Row>, usize) {
    match place {
        Place::Master => (&mut project.master.rack, 0),
        Place::Track => (&mut project.tracks[0].rack, CORE_ROW + 1),
    }
}

fn utility(tweak: fn(&mut Utility)) -> Row {
    let mut slot = Slot::new(Kind::Utility);
    if let Params::Utility(utility) = &mut slot.params {
        tweak(utility);
    }
    Row::Effect(slot)
}

fn quieter() -> Row {
    utility(|utility| utility.gain_db = QUIETER_DB)
}

fn sweeping() -> Row {
    let phaser = Phaser { sync: false, rate_hz: SWEEP_HZ, ..Phaser::DEFAULT };
    Row::Effect(one_band(MotionBand { mode: MotionMode::Phaser, phaser, ..MotionBand::DEFAULT }))
}

fn squashing() -> Row {
    let mut slot = Slot::new(Kind::Compressor);
    if let Params::Compressor(compressor) = &mut slot.params {
        compressor.threshold_db = SQUASH_DB;
    }
    Row::Effect(slot)
}

fn chain_edits() -> [(usize, Edit); 5] {
    [
        (INSTALL_AT, |project, place| chain(project, place).0.extend([Row::Effect(Slot { wet: HALF_WET, ..Slot::new(Kind::Delay) }), sweeping()])),
        (FRONT_AT, |project, place| {
            let (rack, at) = chain(project, place);
            rack.insert(at, quieter());
        }),
        (MOVE_AT, |project, place| {
            let (rack, at) = chain(project, place);
            if let Some(last) = rack.pop() {
                rack.insert(at, last);
            }
        }),
        (REMOVE_AT, |project, place| {
            let (rack, at) = chain(project, place);
            rack.remove(at + 1);
        }),
        (ROW_AT, |project, place| {
            let (rack, at) = chain(project, place);
            if let Some(Row::Effect(slot)) = rack.get_mut(at) {
                slot.wet = HALF_WET;
            }
        }),
    ]
}

fn run(mut project: Project, place: Place, edits: &[(usize, Edit)], replay: Option<usize>) -> (Blocks, Blocks) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(&project, None, &[], &[]);
    audio.note_on(0, 60, VELOCITY);
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let (mut out_l, mut out_r) = (Vec::with_capacity(BLOCKS), Vec::with_capacity(BLOCKS));
    for block in 0..BLOCKS {
        for (_, edit) in edits.iter().filter(|(at, _)| *at == block) {
            edit(&mut project, place);
            audio.sync(&project, None, &[], &[]);
        }
        if replay == Some(block) {
            audio.play(0.0);
        }
        renderer.render(&mut l, &mut r);
        out_l.push(l.clone());
        out_r.push(r.clone());
    }
    (out_l, out_r)
}

fn render(renderer: &mut Renderer, blocks: usize) -> Blocks {
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    (0..blocks)
        .map(|_| {
            renderer.render(&mut l, &mut r);
            l.clone()
        })
        .collect()
}

#[test]
fn the_master_rack_sounds_as_the_same_chain_at_the_end_of_the_track() {
    let edits = chain_edits();
    let (dry, _) = run(quiet(), Place::Master, &[], None);
    let (master, _) = run(quiet(), Place::Master, &edits, Some(REPLAY_AT));
    let (track, _) = run(quiet(), Place::Track, &edits, Some(REPLAY_AT));
    let (installed, _) = run(quiet(), Place::Master, &edits[..1], None);
    let (unreplayed, _) = run(quiet(), Place::Master, &edits, None);
    assert!(peak(&master) > AUDIBLE, "the note is silent under the master's chain");
    assert!(first_difference(&master[INSTALL_AT..], &dry[INSTALL_AT..]).is_some(), "a delay and a phaser on the master changed nothing");
    assert!(first_difference(&master[FRONT_AT..], &installed[FRONT_AT..]).is_some(), "a utility inserted at the front of the master changed nothing");
    assert!(first_difference(&master[REPLAY_AT..], &unreplayed[REPLAY_AT..]).is_some(), "a second play left the master's free-running phaser where it was");
    assert_eq!(first_difference(&master, &track), None, "the master's chain diverges from the same chain at the end of the track, block");
}

#[test]
fn the_master_rack_is_heard_in_order() {
    let mut swap_first = quiet();
    swap_first.tracks[0].rack.push(utility(|utility| utility.balance = -1.0));
    swap_first.master.rack = vec![utility(|utility| utility.swap = true), utility(|utility| utility.balance = 1.0)];
    let mut mute_first = swap_first.clone();
    mute_first.master.rack.reverse();
    let (l, r) = run(swap_first.clone(), Place::Master, &[], None);
    assert!(peak(&l[SETTLE..]) < SILENT && peak(&r[SETTLE..]) > AUDIBLE, "swap then mute-left should leave only the right, got left {} right {}", peak(&l[SETTLE..]), peak(&r[SETTLE..]));
    let (l, r) = run(mute_first, Place::Master, &[], None);
    assert!(peak(&l[SETTLE..]) < SILENT && peak(&r[SETTLE..]) < SILENT, "mute-left then swap should be silent, got left {} right {}", peak(&l[SETTLE..]), peak(&r[SETTLE..]));
    let reorder: [(usize, Edit); 2] = [(REORDER_AT, |project, _| project.master.rack.reverse()), (RESTORE_AT, |project, _| project.master.rack.reverse())];
    let (l, r) = run(swap_first, Place::Master, &reorder, None);
    assert!(peak(&r[SETTLE..REORDER_AT]) > AUDIBLE, "before the reorder the right is silent");
    let (left, right) = (peak(&l[REORDER_AT + SETTLE..RESTORE_AT]), peak(&r[REORDER_AT + SETTLE..RESTORE_AT]));
    assert!(left < SILENT && right < SILENT, "moving the mute ahead of the swap on the master did not silence it, got left {left} right {right}");
    let (left, right) = (peak(&l[RESTORE_AT + SETTLE..]), peak(&r[RESTORE_AT + SETTLE..]));
    assert!(left < SILENT && right > AUDIBLE, "moving the swap back ahead did not bring the right back, got left {left} right {right}");
}

#[test]
fn a_full_master_rack_is_heard_whole() {
    let fitting = RACK_ROWS - CORE_ROW - 1;
    let trimmed: Vec<Row> = (0..TRIMS).map(|_| utility(|utility| utility.gain_db = TRIM_DB)).chain((TRIMS..fitting).map(|_| utility(|utility| utility.gain_db = 0.0))).collect();
    let mut on_master = quiet();
    on_master.master.rack = trimmed.clone();
    let mut on_track = quiet();
    on_track.tracks[0].rack.extend(trimmed);
    assert_eq!(on_track.tracks[0].rack.len(), RACK_ROWS, "wanted the track's MIDI row, its Core and {fitting} utilities to fill its rack of {RACK_ROWS}, got {} rows", on_track.tracks[0].rack.len());
    let (dry, _) = run(quiet(), Place::Master, &[], None);
    let (master, _) = run(on_master, Place::Master, &[], None);
    let (track, _) = run(on_track, Place::Track, &[], None);
    let (loud, trimmed) = (peak(&dry[SETTLE..]), peak(&master[SETTLE..]));
    assert!(trimmed > AUDIBLE, "the note is silent under {TRIMS} trimmed utilities in a rack of {fitting}");
    assert!(trimmed < loud * 0.5, "{TRIMS} utilities at {TRIM_DB} dB each left the peak at {trimmed} against {loud} dry");
    assert_eq!(first_difference(&master, &track), None, "a master rack of utilities diverges from the same utilities after the track's MIDI row and Core, a full rack of {RACK_ROWS}");
}

#[test]
fn the_master_rack_reports_its_gauges() {
    let mut project = quiet();
    project.master.rack = vec![squashing()];
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(&project, None, &[], &[]);
    audio.note_on(0, 60, VELOCITY);
    audio.play(0.0);
    let out = render(&mut renderer, SETTLE);
    assert!(peak(&out) > AUDIBLE, "the note is silent under the master compressor");
    audio.tick(TICK_SECONDS);
    let reduction = audio.gauge(Host::Master, 0);
    assert!(reduction > 0.0, "a compressor squashing the master from {SQUASH_DB} dB reports {reduction} dB of gain reduction");
}

#[test]
fn installing_a_project_replaces_the_master_rack() {
    let mut hushed = quiet();
    hushed.master.rack = vec![utility(|utility| utility.gain_db = HUSH_DB)];
    let open = quiet();
    let levels = [(hushed.clone(), open.clone()), (open, hushed)].map(|(before, after)| {
        let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
        audio.sync(&before, None, &[], &[]);
        audio.note_on(0, 60, VELOCITY);
        audio.play(0.0);
        let first = render(&mut renderer, INSTALL_AT);
        audio.install(&after, None, &[], &[]);
        audio.note_on(0, 60, VELOCITY);
        let second = render(&mut renderer, BLOCKS - INSTALL_AT);
        (peak(&first[SETTLE..]), peak(&second[SETTLE..]))
    });
    let [(hushed_before, opened_after), (open_before, hushed_after)] = levels;
    assert!(opened_after > hushed_before * HUSH_RATIO, "installing an empty master over one hushed by {HUSH_DB} dB left the peak at {opened_after} against {hushed_before} before");
    assert!(hushed_after * HUSH_RATIO < open_before, "installing a master hushed by {HUSH_DB} dB over an empty one left the peak at {hushed_after} against {open_before} before");
}
