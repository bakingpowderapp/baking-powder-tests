use bp::arp::{Arp, Articulation};
use bp::core::{Params, Wave};
use bp::core::params::Envelope;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{AUDIBLE, core_layer, first_frame_apart, frames, midi, song_render};

const PITCH: u8 = 57;
const SECONDS: f64 = 1.0;
const SETTLED: f64 = 0.25;
const LEVEL: f64 = 0.25;
const MEAN_LIMIT: f32 = 0.02;

fn project(wave: Wave) -> Project {
    let mut patch = Params::DEFAULT;
    patch.osc1.shapes[0] = wave;
    patch.amp = Envelope { attack_ms: 1.0, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(LEVEL, patch))];
    let notes = vec![Note { pitch: PITCH, start: 0.0, length: 4.0, velocity: VELOCITY, articulation: Articulation::PLAIN }];
    track.clips.push(Clip { start: 0, length: 4 * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn heard(wave: Wave) -> Vec<f32> {
    let mut rendered = song_render(&project(wave), frames(SECONDS));
    rendered[0].split_off(frames(SETTLED))
}

#[test]
fn every_shape_sounds_finite_and_centred_and_apart_from_the_others() {
    let renders: Vec<(Wave, Vec<f32>)> = Wave::ALL.iter().map(|wave| (*wave, heard(*wave))).collect();
    for (wave, samples) in &renders {
        assert!(samples.iter().all(|sample| sample.is_finite()), "wanted every sample of {wave:?} finite");
        let peak = samples.iter().fold(0.0f32, |peak, sample| peak.max(sample.abs()));
        assert!(peak > AUDIBLE, "wanted {wave:?} heard, got a peak of {peak}");
        let mean = samples.iter().sum::<f32>() / samples.len() as f32;
        assert!(mean.abs() < MEAN_LIMIT, "wanted {wave:?} centred on zero within {MEAN_LIMIT}, got a mean of {mean}");
    }
    for (i, (wave, samples)) in renders.iter().enumerate() {
        for (other, theirs) in &renders[i + 1..] {
            assert!(first_frame_apart(samples, theirs).is_some(), "wanted {wave:?} heard apart from {other:?}, got them bit for bit alike");
        }
    }
}
