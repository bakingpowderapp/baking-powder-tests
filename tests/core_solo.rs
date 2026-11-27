use bp::arp::Arp;
use bp::audio::{Audio, CoreSolo};
use bp::audio::vision::Seer;
use bp::fx::Kind;
use bp::project::{Project, Row, VELOCITY};
use support::{AUDIBLE, BLOCK, BLOCKS, BOTH, CORE_ROW, LEVEL, SAMPLE_RATE, core, first_difference, half_wet, midi, peak};

const FIRST_CORE: Option<CoreSolo> = Some(CoreSolo { row: CORE_ROW, effects: false });
const FIRST_CORE_WITH_EFFECTS: Option<CoreSolo> = Some(CoreSolo { row: CORE_ROW, effects: true });

fn two_tracks(first: Vec<Row>) -> Project {
    let mut project = Project::new();
    project.tracks[0].rack = first;
    project.tracks[1].rack = vec![midi(Arp::DEFAULT), core(LEVEL)];
    project
}

fn run(project: &Project, solos: &[Option<CoreSolo>]) -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, None, solos, &[]);
    for &(track, midi) in &BOTH {
        audio.note_on(track, midi, VELOCITY);
    }
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let (mut out_l, mut out_r) = (Vec::with_capacity(BLOCKS), Vec::with_capacity(BLOCKS));
    for _ in 0..BLOCKS {
        renderer.render(&mut l, &mut r);
        out_l.push(l.clone());
        out_r.push(r.clone());
    }
    (out_l, out_r)
}

#[test]
fn soloing_a_core_silences_the_racks_other_cores_and_no_other_track() {
    let (soloed, _) = run(&two_tracks(vec![midi(Arp::DEFAULT), core(LEVEL), core(LEVEL)]), &[FIRST_CORE]);
    let (expected, _) = run(&two_tracks(vec![midi(Arp::DEFAULT), core(LEVEL), core(0.0)]), &[]);
    assert!(peak(&expected) > AUDIBLE, "the tracks are silent");
    assert_eq!(first_difference(&soloed, &expected), None, "soloing track 0's first core is not that core beside an untouched track 1");
    let (both, _) = run(&two_tracks(vec![midi(Arp::DEFAULT), core(LEVEL), core(LEVEL)]), &[]);
    assert!(first_difference(&both, &expected).is_some(), "the second core adds nothing, so the solo proves nothing");
}

#[test]
fn a_core_solo_mutes_the_racks_effects_until_pressed_again() {
    let (soloed, _) = run(&two_tracks(vec![midi(Arp::DEFAULT), core(LEVEL), half_wet(Kind::Delay), core(LEVEL)]), &[FIRST_CORE]);
    let (dry, _) = run(&two_tracks(vec![midi(Arp::DEFAULT), core(LEVEL)]), &[]);
    assert_eq!(first_difference(&soloed, &dry), None, "a first press on S is not the core alone with the delay muted");
    let (with_effects, _) = run(&two_tracks(vec![midi(Arp::DEFAULT), core(LEVEL), half_wet(Kind::Delay), core(LEVEL)]), &[FIRST_CORE_WITH_EFFECTS]);
    let (through, _) = run(&two_tracks(vec![midi(Arp::DEFAULT), core(LEVEL), half_wet(Kind::Delay)]), &[]);
    assert_eq!(first_difference(&with_effects, &through), None, "a second press on S is not the core through the delay");
}
