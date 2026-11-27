use bp::arp::Arp;
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::fx::{Kind, Params, Slot};
use bp::project::{Project, Row};
use support::{AUDIBLE, BLOCK, BLOCKS, SAMPLE_RATE, core, first_difference, midi, peak};

const SETTLE: usize = 20;
const INAUDIBLE: f32 = 1e-4;

fn run(edits: &[(usize, Vec<Row>)]) -> (Vec<Vec<f32>>, Vec<Vec<f32>>) {
    let mut project = Project::new();
    project.tracks.truncate(1);
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(&project, None, &[], &[]);
    audio.note_on(0, 60, 127);
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let (mut out_l, mut out_r) = (Vec::with_capacity(BLOCKS), Vec::with_capacity(BLOCKS));
    for block in 0..BLOCKS {
        if let Some((_, rack)) = edits.iter().find(|(at, _)| *at == block) {
            project.tracks[0].rack = rack.clone();
            audio.sync(&project, None, &[], &[]);
        }
        renderer.render(&mut l, &mut r);
        out_l.push(l.clone());
        out_r.push(r.clone());
    }
    (out_l, out_r)
}

fn strip(balance: f64) -> Row {
    let mut slot = Slot::new(Kind::Utility);
    if let Params::Utility(strip) = &mut slot.params {
        strip.balance = balance;
    }
    Row::Effect(slot)
}

#[test]
fn a_centred_core_is_the_same_in_both_channels() {
    let (l, r) = run(&[]);
    assert!(peak(&l) > AUDIBLE, "the note is silent");
    assert_eq!(first_difference(&l, &r), None, "a mono Core differs between left and right");
}

#[test]
fn a_balance_hard_right_leaves_the_left_inaudible() {
    let (l, r) = run(&[(0, vec![midi(Arp::DEFAULT), core(1.0), strip(1.0)])]);
    assert!(peak(&r[SETTLE..]) > AUDIBLE, "the right is silent with the balance hard right");
    assert!(peak(&l[SETTLE..]) < INAUDIBLE, "wanted the left below {INAUDIBLE} with the balance hard right, got {}", peak(&l[SETTLE..]));
}

#[test]
fn a_balance_hard_left_leaves_the_right_inaudible() {
    let (l, r) = run(&[(0, vec![midi(Arp::DEFAULT), core(1.0), strip(-1.0)])]);
    assert!(peak(&l[SETTLE..]) > AUDIBLE, "the left is silent with the balance hard left");
    assert!(peak(&r[SETTLE..]) < INAUDIBLE, "wanted the right below {INAUDIBLE} with the balance hard left, got {}", peak(&r[SETTLE..]));
}
