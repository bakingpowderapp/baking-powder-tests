use bp::arp::{Arp, Articulation};
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::core::Params;
use bp::division::Division;
use bp::face::Drawn;
use bp::fx::{self, Kind, Slot};
use bp::project::{Aim, Clip, LaneLfo, LfoClock, LfoShape, Note, Project, Row};
use support::{BLOCK, NO_END, SAMPLE_RATE, beat_frame, core_layer, midi};

const LIMITER: usize = 71;
const FRAMES: usize = 72_000;
const WATCHED: usize = 4_800;
const ALIKE: f32 = 0.02;
const LEVEL: f64 = 0.25;
const GAIN_ROW: usize = 3;

fn limiter(bypass: bool) -> Row {
    let mut slot = Slot::new(Kind::Utility);
    if let fx::Params::Utility(p) = &mut slot.params {
        p.limit = true;
        p.hard = false;
        p.ceiling_db = 0.0;
        p.lookahead_ms = 1.5;
    }
    Row::Effect(Slot { bypass, ..slot })
}

fn ramp_restarting_on_each_note() -> LaneLfo {
    LaneLfo { amount: 0.125, shape: LfoShape::Ramp, sync: false, division: Division::Quarter, rate_hz: 4.0, phase: 0.0, centre: 0.5, retrigger: true, legato: false, once: true, clock: LfoClock::Song, audio: false, fade_ms: 0.0, lag_ms: 0.0, points: Drawn::RAMP, ..LaneLfo::blank(Aim::knob(GAIN_ROW, "STRIP", "GAIN")) }
}

fn project(latent: bool) -> Project {
    let mut patch = Params::DEFAULT;
    patch.amp.attack_ms = 1.0;
    patch.amp.release_ms = 1.0;
    let mut project = Project::new();
    project.tracks.truncate(1);
    let track = &mut project.tracks[0];
    track.rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(LEVEL, patch)), limiter(!latent), Row::Effect(Slot::new(Kind::Utility))];
    track.automation.lfos = vec![ramp_restarting_on_each_note()];
    track.clips = vec![Clip::midi(0, String::new(), vec![Note { pitch: 60, start: 1.0, length: 2.0, velocity: 127, articulation: Articulation::PLAIN }], NO_END)];
    project
}

fn render(project: &Project) -> (Vec<f32>, usize) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, None, &[], &[]);
    let output = audio.timing().output;
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(FRAMES + BLOCK);
    while out.len() < FRAMES {
        renderer.render(&mut l, &mut r);
        out.extend_from_slice(&l);
    }
    out.truncate(FRAMES);
    (out, output)
}

#[test]
fn a_retriggered_lane_behind_a_latent_row_restarts_on_its_own_note() {
    let (dry, dry_output) = render(&project(false));
    let (late, late_output) = render(&project(true));
    assert_eq!(dry_output, 0, "wanted no latency with the limiter bypassed, got {dry_output}");
    assert_eq!(late_output, LIMITER, "wanted the limiter's {LIMITER} samples as the output latency, got {late_output}");
    let note = beat_frame(1);
    let loudest = dry[note..note + WATCHED].iter().fold(0.0f32, |most, sample| most.max(sample.abs()));
    assert!(loudest > 0.01, "wanted the note to sound from frame {note}, got a peak of {loudest}");
    let (worst, at) = (0..WATCHED).map(|i| ((late[note + LIMITER + i] - dry[note + i]).abs(), note + i)).fold((0.0f32, note), |(most, at), (delta, frame)| if delta > most { (delta, frame) } else { (most, at) });
    assert!(worst <= ALIKE * loudest, "wanted the latent track's first {WATCHED} frames of the note to follow the dry track's, its ramp restarting {LIMITER} frames later with its own audio, within {ALIKE} of the peak {loudest}; got {worst} at frame {at} of the dry track");
}
