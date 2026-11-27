use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::fx::{Kind, Slot};
use bp::project::{Clip, Layer, Note, Project, Row, SEND_COUNT, TICKS_PER_BEAT, Track};
use support::{AUDIBLE, SAMPLE_RATE, core_layer, midi, peak_of};

const FRAMES: usize = 98_304;
const REFERENCE_BUFFER: usize = 256;
const LADDER: [usize; 5] = [64, 128, 512, 1024, 2048];

fn chord() -> Vec<Note> {
    [48, 60, 64].iter().map(|&pitch| Note { pitch, start: 0.0, length: 8.0, velocity: 110, articulation: Articulation::PLAIN }).collect()
}

fn project() -> Project {
    let mut project = Project::new();
    project.tracks.clear();
    let rack = vec![
        midi(Arp::DEFAULT),
        Row::Core(core_layer(1.0, Params::DEFAULT)),
        Row::Effect(Slot::new(Kind::VolumeLfo)),
        Row::Effect(Slot::new(Kind::Delay)),
        Row::Core(Layer::DEFAULT),
        Row::Effect(Slot::new(Kind::Reverb)),
    ];
    let mut track = Track {
        name: String::new(),
        solo: false,
        mute: false,
        gain: 0.8,
        sends: [0.0; SEND_COUNT],
        rack,
        clips: vec![Clip { start: 0, length: 16 * TICKS_PER_BEAT, label: String::new(), notes: chord(), file: None }],
        ..Track::blank()
    };
    track.sends[0] = 0.3;
    project.tracks.push(track);
    project
}

fn render(project: &Project, buffer: usize) -> (Vec<f32>, Vec<f32>) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, buffer, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(project, None, &[], &[]);
    audio.play(0.0);
    let (mut l, mut r) = (vec![0f32; buffer], vec![0f32; buffer]);
    let (mut left, mut right) = (Vec::with_capacity(FRAMES), Vec::with_capacity(FRAMES));
    for _ in 0..FRAMES / buffer {
        renderer.render(&mut l, &mut r);
        left.extend(&l);
        right.extend(&r);
    }
    (left, right)
}

fn expect_matching(name: &str, reference: &(Vec<f32>, Vec<f32>), other: &(Vec<f32>, Vec<f32>)) {
    for (side, wanted, got) in [("left", &reference.0, &other.0), ("right", &reference.1, &other.1)] {
        if let Some(frame) = wanted.iter().zip(got.iter()).position(|(a, b)| a.to_bits() != b.to_bits()) {
            panic!("{name}: {side} frame {frame} wanted {} got {}", wanted[frame], got[frame]);
        }
    }
}

#[test]
fn every_buffer_renders_the_same_samples() {
    let project = project();
    let reference = render(&project, REFERENCE_BUFFER);
    let heard = peak_of(&reference.0);
    assert!(heard > AUDIBLE, "wanted the chord heard at buffer {REFERENCE_BUFFER}, got a peak of {heard}");
    for buffer in LADDER {
        let other = render(&project, buffer);
        expect_matching(&format!("buffer {buffer} against {REFERENCE_BUFFER}"), &reference, &other);
    }
}
