use bp::arp::Arp;
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::fx::Kind;
use bp::project::{Project, Row, VELOCITY};
use support::{AUDIBLE, BLOCK, BLOCKS, BOTH, HALF, LEVEL, SAMPLE_RATE, core_layer, first_difference, half_wet, midi, peak, scaled};

const EDIT_AT: usize = 50;
const REVERT_AT: usize = 100;
const FIRST: [(usize, u8); 1] = [(0, 60)];
const SECOND: [(usize, u8); 1] = [(1, 67)];

type Edit = fn(&mut Project);

fn quiet() -> Project {
    let mut project = Project::new();
    for track in &mut project.tracks {
        track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(LEVEL, Params::DEFAULT))];
    }
    project
}

fn run(mut project: Project, held: &[(usize, u8)], solo: Option<usize>, edits: &[(usize, Edit)]) -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(&project, solo, &[], &[]);
    for &(track, midi) in held {
        audio.note_on(track, midi, VELOCITY);
    }
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let (mut out_l, mut out_r) = (Vec::with_capacity(BLOCKS), Vec::with_capacity(BLOCKS));
    for block in 0..BLOCKS {
        for (_, edit) in edits.iter().filter(|(at, _)| *at == block) {
            edit(&mut project);
            audio.sync(&project, solo, &[], &[]);
        }
        renderer.render(&mut l, &mut r);
        out_l.push(l.clone());
        out_r.push(r.clone());
    }
    (out_l, out_r)
}

#[test]
fn a_muted_track_is_not_heard() {
    let mut muted = quiet();
    muted.tracks[0].mute = true;
    let (heard, _) = run(muted, &BOTH, None, &[]);
    let (alone, _) = run(quiet(), &SECOND, None, &[]);
    assert!(peak(&alone) > AUDIBLE, "the unmuted track is silent");
    assert_eq!(first_difference(&heard, &alone), None, "with track 0 muted the output differs from track 1 alone");
}

#[test]
fn soloing_a_track_silences_the_others() {
    let mut soloed = quiet();
    soloed.tracks[1].solo = true;
    let (heard, _) = run(soloed, &BOTH, None, &[]);
    let (alone, _) = run(quiet(), &SECOND, None, &[]);
    assert_eq!(first_difference(&heard, &alone), None, "with track 1 soloed the output differs from track 1 alone");
}

#[test]
fn muting_every_track_leaves_exact_silence() {
    let mut all = quiet();
    for track in &mut all.tracks {
        track.mute = true;
    }
    let (l, r) = run(all, &BOTH, None, &[]);
    assert_eq!((peak(&l), peak(&r)), (0.0, 0.0), "muted tracks still reach the output");
}

#[test]
fn the_faders_scale_the_output_exactly() {
    let (dry, _) = run(quiet(), &FIRST, None, &[]);
    let mut track = quiet();
    track.tracks[0].gain = HALF;
    let (track_half, _) = run(track, &FIRST, None, &[]);
    let mut master = quiet();
    master.master.gain = HALF;
    let (master_half, _) = run(master, &FIRST, None, &[]);
    assert!(peak(&dry) > AUDIBLE, "the note is silent");
    assert_eq!(first_difference(&track_half, &scaled(&dry, HALF)), None, "a track fader at {HALF} is not exactly half the signal");
    assert_eq!(first_difference(&master_half, &scaled(&dry, HALF)), None, "the master fader at {HALF} is not exactly half the signal");
}

#[test]
fn an_effect_on_the_master_changes_the_sound_and_leaves_no_trace_when_removed() {
    let edits: [(usize, Edit); 2] = [(EDIT_AT, |project| project.master.rack = vec![half_wet(Kind::Delay)]), (REVERT_AT, |project| project.master.rack.clear())];
    let (control, _) = run(quiet(), &FIRST, None, &[]);
    let (edited, _) = run(quiet(), &FIRST, None, &edits);
    assert_eq!(first_difference(&control[..EDIT_AT], &edited[..EDIT_AT]), None, "the output changed before the master edit");
    assert!(first_difference(&control[EDIT_AT..REVERT_AT], &edited[EDIT_AT..REVERT_AT]).is_some(), "a delay on the master changed nothing");
    assert!(peak(&edited[EDIT_AT..REVERT_AT]) > AUDIBLE, "the note went silent under the master delay");
    assert_eq!(first_difference(&control[REVERT_AT..], &edited[REVERT_AT..]), None, "after removing the master delay the sound differs from a master that never had it");
}
