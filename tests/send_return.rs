use bp::arp::Arp;
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::fx::{Kind, Slot};
use bp::project::{Project, Row, VELOCITY};
use support::{AUDIBLE, BLOCK, BLOCKS, LEVEL, SAMPLE_RATE, SEND, core_layer, first_difference, midi, peak, scaled};

fn quiet() -> Project {
    let mut project = Project::new();
    for track in &mut project.tracks {
        track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(LEVEL, Params::DEFAULT))];
    }
    project
}

fn run(project: Project, solo: Option<usize>) -> Vec<Vec<f32>> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(&project, solo, &[], &[]);
    audio.note_on(0, 60, VELOCITY);
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(BLOCKS);
    for _ in 0..BLOCKS {
        renderer.render(&mut l, &mut r);
        out.push(l.clone());
    }
    out
}

#[test]
fn a_send_feeds_the_first_send_bus_and_rides_its_return() {
    let dry = run(quiet(), None);
    assert!(peak(&dry) > AUDIBLE, "the note is silent");
    let mut sending = quiet();
    sending.tracks[0].sends[0] = SEND;
    let sent = run(sending.clone(), None);
    let expected: Vec<Vec<f32>> = dry.iter().map(|block| block.iter().map(|sample| sample + sample * SEND).collect()).collect();
    assert_eq!(first_difference(&sent, &expected), None, "a send of {SEND} through an empty return is not exactly the dry mix plus {SEND} of itself");
    sending.sends[0].gain = 0.0;
    let returned_at_zero = run(sending, None);
    assert_eq!(first_difference(&returned_at_zero, &dry), None, "a send bus returned at zero still changed the output");
    let mut unsent = quiet();
    unsent.sends[0].rack = vec![Row::Effect(Slot::new(Kind::Delay))];
    let unfed = run(unsent, None);
    assert_eq!(first_difference(&unfed, &dry), None, "a delay on a send bus nobody sends to changed the output");
}

#[test]
fn soloing_a_send_bus_hears_it_alone() {
    let dry = run(quiet(), None);
    let mut sending = quiet();
    sending.tracks[0].sends[0] = SEND;
    let bus_only = run(sending, Some(0));
    assert!(peak(&bus_only) > AUDIBLE, "the soloed send bus is silent");
    assert_eq!(first_difference(&bus_only, &scaled(&dry, SEND)), None, "the first send bus soloed and fed at {SEND} is not exactly the dry signal at {SEND}");
}
