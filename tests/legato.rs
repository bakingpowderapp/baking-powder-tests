use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BEAT, VELOCITY};
use support::{SAMPLE_RATE, core_layer, midi, peak_of};

const TEMPOS: [f64; 2] = [120.0, 93.75];
const BUFFERS: [usize; 3] = [2048, 512, 256];
const NOTES: usize = 8;
const WINDOWS_PER_BEAT: usize = 8;
const AUDIBLE: f32 = 0.05;
const MIDI: u8 = 60;

fn project(bpm: f64) -> Project {
    let mut patch = Params::DEFAULT;
    patch.amp.attack_ms = 1.0;
    patch.amp.sustain = 1.0;
    patch.amp.release_ms = 1.0;
    let notes = (0..NOTES).map(|beat| Note { pitch: MIDI, start: beat as f32, length: 1.0, velocity: VELOCITY, articulation: Articulation::PLAIN }).collect();
    let mut project = Project::new();
    project.bpm = bpm;
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch))];
    project.tracks[0].clips.push(Clip { start: 0, length: NOTES as i32 * TICKS_PER_BEAT, label: String::new(), notes, file: None });
    project
}

fn play(project: &Project, block: usize) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, block, Seer::Inline);
    audio.sync(project, None, &[], &[]);
    audio.play(0.0);
    let total = ((NOTES as f64 + 0.25) * SAMPLE_RATE * 60.0 / project.bpm) as usize;
    let mut out = Vec::with_capacity(total + block);
    let (mut l, mut r) = (vec![0.0; block], vec![0.0; block]);
    while out.len() < total {
        renderer.render(&mut l, &mut r);
        out.extend_from_slice(&l);
    }
    out
}

#[test]
fn back_to_back_notes_on_one_pitch_never_drop_out() {
    let mut quiet = Vec::new();
    for bpm in TEMPOS {
        let project = project(bpm);
        let window_beats = 1.0 / WINDOWS_PER_BEAT as f64;
        let frames_per_beat = SAMPLE_RATE * 60.0 / bpm;
        for block in BUFFERS {
            let out = play(&project, block);
            for index in 0..NOTES * WINDOWS_PER_BEAT {
                let beat = index as f64 * window_beats;
                let (from, to) = ((beat * frames_per_beat).round() as usize, ((beat + window_beats) * frames_per_beat).round() as usize);
                let peak = peak_of(&out[from..to]);
                if peak < AUDIBLE {
                    quiet.push(format!("{bpm} bpm, buffer {block}, beat {beat}: peak {peak}"));
                }
            }
        }
    }
    assert!(quiet.is_empty(), "{quiet:#?}");
}
