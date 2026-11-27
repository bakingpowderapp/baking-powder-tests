use bp::arp::Arp;
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::fx::params::{Curve, Freak};
use bp::fx::{Kind, Params, Slot};
use bp::project::{Project, Row};
use support::{AUDIBLE, BLOCK, BLOCKS, SAMPLE_RATE, core, effect, first_difference, half_wet, midi, peak};

const DRIVE_DB: f64 = 6.0;

fn distorting() -> Row {
    let mut freak = Freak { band_count: 1, drive_db: DRIVE_DB, ..Freak::DEFAULT };
    freak.bands[0].curve = Some(Curve::Soft);
    Row::Effect(Slot { params: Params::Freak(freak), ..Slot::new(Kind::Freak) })
}

fn run(edits: &[(usize, Vec<Row>)]) -> Vec<Vec<f32>> {
    let mut project = Project::new();
    project.tracks.truncate(1);
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(&project, None, &[], &[]);
    audio.note_on(0, 60, 127);
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(BLOCKS);
    for block in 0..BLOCKS {
        if let Some((_, rack)) = edits.iter().find(|(at, _)| *at == block) {
            project.tracks[0].rack = rack.clone();
            audio.sync(&project, None, &[], &[]);
        }
        renderer.render(&mut l, &mut r);
        out.push(l.clone());
    }
    out
}

#[test]
fn inserting_and_removing_an_effect_mid_note() {
    let control = run(&[]);
    let edited = run(&[(50, vec![midi(Arp::DEFAULT), core(1.0), half_wet(Kind::Delay)]), (100, vec![midi(Arp::DEFAULT), core(1.0)])]);
    assert!(peak(&control[10..]) > AUDIBLE, "the held note is silent");
    assert_eq!(first_difference(&control[..50], &edited[..50]), None, "the output changed before the edit");
    assert!(first_difference(&control[50..100], &edited[50..100]).is_some(), "the inserted delay changed nothing");
    assert!(peak(&edited[50..100]) > AUDIBLE, "the note went silent under the inserted delay");
    assert_eq!(first_difference(&control[100..], &edited[100..]), None, "after removing the delay the sound differs from a rack that never had it");
}

#[test]
fn moving_rows_changes_the_sound() {
    let ordered = run(&[(50, vec![midi(Arp::DEFAULT), core(1.0), distorting(), effect(Kind::Delay)])]);
    let moved = run(&[(50, vec![midi(Arp::DEFAULT), core(1.0), distorting(), effect(Kind::Delay)]), (100, vec![midi(Arp::DEFAULT), core(1.0), effect(Kind::Delay), distorting()])]);
    assert_eq!(first_difference(&ordered[..100], &moved[..100]), None, "the output changed before the move");
    assert!(first_difference(&ordered[100..], &moved[100..]).is_some(), "moving a row changed nothing, but order is routing");
    assert!(peak(&moved[100..]) > AUDIBLE, "the note went silent after the move");
}

#[test]
fn swapping_an_effects_kind_keeps_the_note_sounding() {
    let swapped = run(&[(50, vec![midi(Arp::DEFAULT), core(1.0), effect(Kind::Delay)]), (100, vec![midi(Arp::DEFAULT), core(1.0), effect(Kind::Reverb)])]);
    let fresh = run(&[(100, vec![midi(Arp::DEFAULT), core(1.0), effect(Kind::Reverb)])]);
    assert!(peak(&swapped[100..]) > AUDIBLE, "the note went silent when the row's kind changed");
    assert_eq!(first_difference(&swapped[100..], &fresh[100..]), None, "a swapped-in reverb sounds different from a freshly inserted one");
}
