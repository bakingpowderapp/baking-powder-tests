use bp::arp::Arp;
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::project::{Clip, Project};
use support::{BLOCK, LEVEL, SAMPLE_RATE, core, midi, peak_of};

const DENSE_TRACKS: usize = 17;
const CHORD: u8 = 32;
const LOWEST: u8 = 36;
const SOUNDING_SECONDS: f64 = 0.5;
const SETTLING_SECONDS: f64 = 3.0;
const SILENT: f32 = 1e-4;

fn dense() -> Project {
    let mut project = Project::new();
    while project.tracks.len() < DENSE_TRACKS {
        assert!(project.add_midi_track(project.tracks.len()), "wanted room for {DENSE_TRACKS} tracks");
    }
    let pitches: Vec<u8> = (LOWEST..LOWEST + CHORD).collect();
    for track in &mut project.tracks {
        track.rack = vec![midi(Arp::DEFAULT), core(LEVEL)];
        track.clips = vec![Clip::chord(0, String::from("wall"), &pitches)];
    }
    project
}

fn rendered(renderer: &mut Renderer, seconds: f64) -> Vec<f32> {
    let frames = (seconds * SAMPLE_RATE) as usize;
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(frames);
    while out.len() < frames {
        let n = BLOCK.min(frames - out.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    out
}

#[test]
fn stop_silences_a_song_holding_more_notes_than_an_old_table_could_track() {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(&dense(), None, &[], &[]);
    audio.play(0.0);
    let sounding = rendered(&mut renderer, SOUNDING_SECONDS);
    assert!(peak_of(&sounding) > SILENT, "wanted the wall of notes to sound, got a peak of {}", peak_of(&sounding));
    audio.stop();
    rendered(&mut renderer, SETTLING_SECONDS);
    let after = rendered(&mut renderer, SOUNDING_SECONDS);
    assert!(peak_of(&after) <= SILENT, "wanted silence {SETTLING_SECONDS} s after stop, got a peak of {}", peak_of(&after));
}
