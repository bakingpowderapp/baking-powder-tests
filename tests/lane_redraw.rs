use bp::app::App;
use bp::arp::Arp;
use bp::audio::Renderer;
use bp::core::Params;
use bp::project::{Aim, Curve, LaneLfo, LfoField, Node, Project, Row};
use support::{BLOCK, CORE_ROW, SAMPLE_RATE, core_layer, midi};

const SETTLE_TICKS: usize = 8;
const TICK: f32 = 1.0 / 60.0;

fn open(lfo: Option<LaneLfo>, curve: Option<Curve>) -> (App, Renderer) {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, Params::DEFAULT))];
    project.tracks[0].automation.lfos.extend(lfo);
    project.tracks[0].automation.curves.extend(curve);
    let session = format!("{{\"project\":{},\"palette\":false,\"open\":{CORE_ROW}}}", project.to_json().write());
    let mut app = App::new();
    app.resume(&session);
    let renderer = app.audio_started(SAMPLE_RATE, BLOCK, BLOCK as u32);
    for _ in 0..SETTLE_TICKS {
        app.tick(TICK);
    }
    (app, renderer)
}

#[test]
fn a_core_face_under_an_lfo_keeps_the_app_redrawing_while_stopped() {
    let (mut waved, _renderer) = open(Some(LaneLfo { amount: 1.0, sync: false, rate_hz: 1.0, ..LaneLfo::blank(Aim::knob(CORE_ROW, "OSC 1", "LEVEL")) }), None);
    assert!(waved.tick(TICK), "wanted a redraw asked for with the Core face open under an LFO and the transport stopped");
    let (mut still, _renderer) = open(None, None);
    assert!(!still.tick(TICK), "wanted no redraw asked for with nothing moving on the open face");
}

#[test]
fn an_lfo_given_its_amount_by_a_curve_keeps_the_app_redrawing_too() {
    let amount = Curve { aim: Aim::knob(CORE_ROW, "OSC 1", "LEVEL"), lfo: Some(LfoField::Amount), nodes: vec![Node { tick: 0, travel: 1.0, bend: 0.0 }] };
    let (mut waved, _renderer) = open(None, Some(amount));
    assert!(waved.tick(TICK), "wanted a redraw asked for with the Core face open under an LFO whose amount a curve supplies, and the transport stopped");
}
