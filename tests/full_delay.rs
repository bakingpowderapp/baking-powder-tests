use bp::arp::Arp;
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::core::Params;
use bp::core::params::Envelope;
use bp::project::{Project, Row, VELOCITY};
use support::{BLOCK, SAMPLE_RATE, SILENT, core_layer, frames, midi, peak_of};

const KEY: u8 = 60;
const DELAY_MS: f64 = 200.0;
const ATTACK_MS: f64 = 300.0;
const RELEASE_MS: f64 = 50.0;
const BLIP_SECONDS: f64 = 0.062;
const RENDER_SECONDS: f64 = 1.5;

fn waiting() -> Params {
    let mut patch = Params::DEFAULT;
    patch.osc1.levels = [0.0, 0.0, 0.0, 1.0];
    patch.amp = Envelope { delay_ms: DELAY_MS, attack_ms: ATTACK_MS, decay_ms: 1.0, sustain: 1.0, release_ms: RELEASE_MS, ..Envelope::DEFAULT };
    patch
}

struct Session {
    audio: Audio,
    renderer: Renderer,
    out: Vec<f32>,
}

impl Session {
    fn open(patch: Params) -> Session {
        let mut project = Project::new();
        project.tracks.truncate(1);
        project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch))];
        let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
        renderer.patient();
        renderer.start_workers(0, || None);
        audio.sync(&project, None, &[], &[]);
        Session { audio, renderer, out: Vec::new() }
    }

    fn render(&mut self, seconds: f64) {
        let end = self.out.len() + frames(seconds);
        let (mut left, mut right) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
        while self.out.len() < end {
            let n = BLOCK.min(end - self.out.len());
            self.renderer.render(&mut left[..n], &mut right[..n]);
            self.out.extend_from_slice(&left[..n]);
        }
    }
}

fn blip(patch: Params) -> Vec<f32> {
    let mut session = Session::open(patch);
    session.audio.note_on(0, KEY, VELOCITY);
    session.render(BLIP_SECONDS);
    session.audio.note_off(0, KEY);
    session.render(RENDER_SECONDS - BLIP_SECONDS);
    session.out
}

#[test]
fn a_blip_shorter_than_the_delay_stays_silent() {
    let out = blip(waiting());
    let loudest = peak_of(&out);
    assert!(loudest <= SILENT, "wanted a {BLIP_SECONDS} s note lifted inside a {DELAY_MS} ms delay to make no sound, got a peak of {loudest}");
}
