use bp::face::{READOUT_ROWS, interior, readout_rect};
use bp::fx::Picture;
use bp::fx::params::VolumeLfo;
use bp::fx::{Kind, Params, Slot};

const LIFT: i32 = 8;
const NEAR: f64 = 0.01;

fn shape(slot: &Slot) -> VolumeLfo {
    let Params::VolumeLfo(lfo) = slot.params else { panic!("wanted a volume LFO in the slot") };
    lfo
}

fn middle() -> (i32, i32) {
    let inner = interior(readout_rect(READOUT_ROWS));
    (inner.x + inner.w / 2, inner.y + inner.h / 2)
}

#[test]
fn a_shift_drag_on_the_line_bows_the_segment_and_a_shift_double_click_straightens_it() {
    let inner = interior(readout_rect(READOUT_ROWS));
    let (x, y) = middle();
    let mut slot = Slot::new(Kind::VolumeLfo);
    let mut picture = Picture::new();
    assert!(picture.down(x, y, inner, &mut slot, true, 1), "wanted the shift press on the line taken");
    picture.moved(x, y - LIFT, inner, &mut slot, false, false);
    picture.up(Some(&mut slot));
    let bowed = shape(&slot);
    assert_eq!(bowed.drawn.count, 2, "wanted no point laid by a shift-drag, got {} points", bowed.drawn.count);
    assert!(bowed.drawn.points[0].curve > 0.0, "wanted the segment bowed upward by a shift-drag of {LIFT} px, got a curve of {}", bowed.drawn.points[0].curve);
    let mid = bowed.value_at(0.5);
    assert!(mid > 0.5 + NEAR, "wanted the bowed line above the straight middle of 0.5, got {mid}");
    picture.down(x, y, inner, &mut slot, true, 2);
    picture.up(Some(&mut slot));
    let straight = shape(&slot);
    assert_eq!(straight.drawn.points[0].curve, 0.0, "wanted a shift double-click to straighten the segment, got a curve of {}", straight.drawn.points[0].curve);
}

#[test]
fn a_plain_press_on_the_line_lays_a_point_and_drags_it() {
    let inner = interior(readout_rect(READOUT_ROWS));
    let (x, y) = middle();
    let mut slot = Slot::new(Kind::VolumeLfo);
    let mut picture = Picture::new();
    assert!(picture.down(x, y, inner, &mut slot, false, 1), "wanted the press on the line taken");
    let laid = shape(&slot);
    assert_eq!(laid.drawn.count, 3, "wanted a point laid by the press, got {} points", laid.drawn.count);
    assert!((laid.drawn.points[1].x - 0.5).abs() <= NEAR && (laid.drawn.points[1].y - 0.5).abs() <= NEAR, "wanted the new point on the line's middle, got ({}, {})", laid.drawn.points[1].x, laid.drawn.points[1].y);
    assert!(laid.drawn.points.iter().all(|point| point.curve == 0.0), "wanted every segment straight after the press, got curves {:?}", laid.drawn.shape().iter().map(|point| point.curve).collect::<Vec<f64>>());
    picture.moved(x, y - LIFT, inner, &mut slot, false, false);
    picture.up(Some(&mut slot));
    let dragged = shape(&slot);
    assert_eq!(dragged.drawn.count, 3, "wanted the laid point kept on release, got {} points", dragged.drawn.count);
    assert!(dragged.drawn.points[1].y > 0.5 + NEAR, "wanted the laid point dragged up by {LIFT} px, got a level of {}", dragged.drawn.points[1].y);
}
