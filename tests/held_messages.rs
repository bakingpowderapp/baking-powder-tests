use bp::arp::Arp;
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::project::{Aim, Node, Project, TRACKS, VELOCITY};
use support::{AUDIBLE, BLOCK, LEVEL, SAMPLE_RATE, core, midi, peak};

const HEARD: usize = 0;
const QUEUE: usize = 4096;
const SETTLE: usize = 16;
const TICK_SECONDS: f32 = 1.0 / 60.0;
const SILENT_NODE: Node = Node { tick: 0, travel: 0.0, bend: 0.0 };

fn full() -> Project {
    let mut project = Project::new();
    while project.tracks.len() < TRACKS {
        project.add_midi_track(project.tracks.len());
    }
    for track in &mut project.tracks {
        track.rack = vec![midi(Arp::DEFAULT), core(LEVEL)];
    }
    project
}

fn open(project: &Project) -> (Audio, Renderer) {
    let (mut audio, renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, None, &[], &[]);
    (audio, renderer)
}

fn sync(audio: &mut Audio, project: &Project) {
    audio.sync(project, None, &[], &[]);
}

fn render(renderer: &mut Renderer, blocks: usize) -> Vec<Vec<f32>> {
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    (0..blocks)
        .map(|_| {
            renderer.render(&mut l, &mut r);
            l.clone()
        })
        .collect()
}

fn toggle_mutes(project: &mut Project, from: usize) {
    for track in &mut project.tracks[from..] {
        track.mute ^= true;
    }
}

#[test]
fn a_message_the_full_queue_refused_lands_at_the_next_tick() {
    let mut project = full();
    toggle_mutes(&mut project, 0);
    let (mut audio, mut renderer) = open(&project);
    render(&mut renderer, 1);
    let toggles = QUEUE / TRACKS + 1;
    for _ in 0..toggles {
        toggle_mutes(&mut project, 0);
        sync(&mut audio, &project);
    }
    assert!(!project.tracks[HEARD].mute, "wanted the last toggle to leave the tracks unmuted");
    audio.note_on(HEARD, 60, VELOCITY);
    audio.play(0.0);
    let before = render(&mut renderer, SETTLE);
    assert!(peak(&before) < AUDIBLE, "wanted the note held back with the toggle the full queue refused, heard {}", peak(&before));
    audio.tick(TICK_SECONDS);
    let after = render(&mut renderer, SETTLE);
    assert!(peak(&after) > AUDIBLE, "wanted the held toggle and note to land at the tick, heard {}", peak(&after));
}

#[test]
fn removing_volume_automation_restores_the_gain_even_split_across_blocks() {
    let mut project = full();
    project.tracks[HEARD].automation.set_node(&Aim::Gain, None, SILENT_NODE);
    let (mut audio, mut renderer) = open(&project);
    audio.note_on(HEARD, 60, VELOCITY);
    audio.play(0.0);
    let silenced = render(&mut renderer, SETTLE);
    assert!(peak(&silenced) < AUDIBLE, "wanted the volume curve at zero to silence the track, heard {}", peak(&silenced));
    let others = TRACKS - 1;
    for _ in 0..(QUEUE - 1) / others {
        toggle_mutes(&mut project, HEARD + 1);
        sync(&mut audio, &project);
    }
    project.tracks[HEARD].automation.curves.clear();
    sync(&mut audio, &project);
    let between = render(&mut renderer, 1);
    assert!(peak(&between) < AUDIBLE, "wanted the base gain still to come, heard {}", peak(&between));
    audio.tick(TICK_SECONDS);
    let restored = render(&mut renderer, SETTLE);
    assert!(peak(&restored) > AUDIBLE, "wanted the base gain back once its strip landed, heard {}", peak(&restored));
}
