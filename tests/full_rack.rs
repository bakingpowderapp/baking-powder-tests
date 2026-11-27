use bp::arp::Arp;
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::fx::{Kind, Slot};
use bp::project::{Layer, Project, RACK_ROWS, Row};
use support::{AUDIBLE, BLOCK, BLOCKS, CORE_ROW, SAMPLE_RATE, first_difference, midi, peak};

const INSTALL_AT: usize = 50;
const CHANGE_AT: usize = 100;
const SWAPPED_ROW: usize = 6;
const WET: f64 = 0.1;

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

fn full_rack(kind_at: impl Fn(usize) -> Kind) -> Vec<Row> {
    let effects = (CORE_ROW + 1..RACK_ROWS).map(|row| Row::Effect(Slot { wet: WET, ..Slot::new(kind_at(row)) }));
    let rack: Vec<Row> = [midi(Arp::DEFAULT), Row::Core(Layer::DEFAULT)].into_iter().chain(effects).collect();
    assert_eq!(rack.len(), RACK_ROWS, "a full rack has {RACK_ROWS} rows");
    rack
}

#[test]
fn swapping_a_kind_in_a_full_rack_matches_the_rack_installed_fresh() {
    let before = full_rack(|_| Kind::Utility);
    let after = full_rack(|row| if row == SWAPPED_ROW { Kind::Delay } else { Kind::Utility });
    let swapped = run(&[(INSTALL_AT, before), (CHANGE_AT, after.clone())]);
    let fresh = run(&[(CHANGE_AT, after)]);
    assert!(peak(&swapped[CHANGE_AT..]) > AUDIBLE, "the note went silent when a row of a full rack changed kind");
    assert_eq!(first_difference(&swapped[CHANGE_AT..], &fresh[CHANGE_AT..]), None, "a kind swapped into a full rack sounds different from the same rack installed fresh");
}

#[test]
fn replacing_every_effect_of_a_full_rack_matches_the_rack_installed_fresh() {
    let before = full_rack(|_| Kind::Utility);
    let after = full_rack(|_| Kind::Delay);
    let replaced = run(&[(INSTALL_AT, before), (CHANGE_AT, after.clone())]);
    let fresh = run(&[(CHANGE_AT, after)]);
    assert!(peak(&replaced[CHANGE_AT..]) > AUDIBLE, "the note went silent when every effect of a full rack was replaced");
    assert_eq!(first_difference(&replaced[CHANGE_AT..], &fresh[CHANGE_AT..]), None, "a rack preset loaded over a full rack sounds different from the same rack installed fresh");
}
