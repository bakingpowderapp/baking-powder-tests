use bp::core::params::{Lfo, Modulator};
use bp::core::{Params, Shape, Target};
use bp::draw::{DrawList, Rect};
use bp::face::{CELL, FRAME, Face, PITCH, READOUT_ROWS};

const BRT1: (i32, i32) = (1, 14);
const BRT2: (i32, i32) = (2, 14);
const BRT3: (i32, i32) = (3, 14);
const KNOB: f64 = 0.5;
const WOBBLE_HZ: f64 = 2.0;

fn interior((row, column): (i32, i32)) -> Rect {
    let rect = Rect::new(column * PITCH, (READOUT_ROWS + row) * PITCH, PITCH + FRAME, CELL);
    Rect::new(rect.x + FRAME, rect.y + FRAME, rect.w - 2 * FRAME, rect.h - 2 * FRAME)
}

fn drawn(patch: &Params, cell: (i32, i32)) -> bool {
    let face = Face::new();
    let mut list = DrawList::new();
    patch.draw(&face, &mut list, None, |_, _| {});
    let inside = interior(cell);
    list.fills().iter().any(|fill| fill.rect.overlaps(inside))
}

fn fm_by_motion() -> Params {
    let mut patch = Params::DEFAULT;
    patch.motion.lfo1 = Lfo { target: Target::Fm, amount: 1.0, shape: Shape::Sine, rate_hz: WOBBLE_HZ, ..Lfo::DEFAULT };
    patch
}

#[test]
fn bright_rests_on_a_carrier_whenever_the_engine_drops_it() {
    assert!(drawn(&Params::DEFAULT, BRT1), "wanted BRT1 drawn live at row {} column {} with no FM at all", BRT1.0, BRT1.1);
    let mut knob = Params::DEFAULT;
    knob.osc1.fm = KNOB;
    assert!(!drawn(&knob, BRT1), "wanted BRT1 resting under the FM knob at {KNOB}");
    assert!(!drawn(&fm_by_motion(), BRT1), "wanted BRT1 resting while an LFO alone drives the FM, as the engine drops bright on a carrier under any FM");
    assert!(drawn(&fm_by_motion(), BRT2), "wanted BRT2 live while osc 2 is the modulator and not a carrier");
}

#[test]
fn bright_rests_on_osc_two_when_osc_three_modulates_it_by_motion() {
    let mut by_three = fm_by_motion();
    by_three.modulator = Modulator::Osc3;
    assert!(!drawn(&by_three, BRT2), "wanted BRT2 resting while osc 3 bends osc 2 with an LFO's FM alone");
    assert!(drawn(&by_three, BRT3), "wanted BRT3 live, the modulator itself never bent");
}
