use bp::face::{CELL, PITCH};
use bp::fx::Kind;
use bp::project::{Cores, RACK_ROWS, Row};
use bp::rack::{Offer, Rack, SHOWN};
use support::{core, effect};

const W: i32 = 200;
const EXPECTED_ROWS: usize = 40;
const EXPECTED_SHOWN: usize = 15;

fn rows(count: usize) -> Vec<Row> {
    std::iter::once(core(1.0)).chain((1..count).map(|_| effect(Kind::Utility))).collect()
}

fn offer() -> Offer {
    Offer { cores: Cores::Required, paste: false }
}

fn name_at(rack: &mut Rack, slot: usize, rows: &[Row]) -> Option<usize> {
    rack.name_at(W / 2, slot as i32 * PITCH + CELL / 2, rows, offer())
}

#[test]
fn the_rack_shows_fifteen_of_forty_rows_and_the_wheel_reaches_the_rest() {
    assert_eq!((RACK_ROWS, SHOWN), (EXPECTED_ROWS, EXPECTED_SHOWN), "wanted {EXPECTED_ROWS} rows a rack with {EXPECTED_SHOWN} shown");
    let rows = rows(RACK_ROWS);
    let mut rack = Rack::new(W);
    assert_eq!(name_at(&mut rack, 0, &rows), Some(0), "wanted the first row at the top before any scroll");
    assert_eq!(name_at(&mut rack, SHOWN - 1, &rows), Some(SHOWN - 1), "wanted row {SHOWN} at the bottom of the view");
    assert!(rack.wheel(-PITCH, &rows), "a wheel notch did not scroll the rack");
    assert_eq!(name_at(&mut rack, 0, &rows), Some(1), "wanted the second row at the top after one notch");
    rack.wheel(-PITCH * RACK_ROWS as i32, &rows);
    assert_eq!(name_at(&mut rack, SHOWN - 1, &rows), Some(RACK_ROWS - 1), "wanted the last row at the bottom after scrolling past the end");
    assert!(!rack.wheel(-PITCH, &rows), "a full rack scrolled past its last row");
    assert!(rack.wheel(PITCH * 2 * RACK_ROWS as i32, &rows), "scrolling back changed nothing");
    assert_eq!(name_at(&mut rack, 0, &rows), Some(0), "wanted the first row back at the top after scrolling past the start");
}

#[test]
fn picking_a_row_out_of_view_scrolls_it_into_view() {
    let rows = rows(SHOWN + 2);
    let last = rows.len() - 1;
    let mut rack = Rack::new(W);
    rack.pick(&rows, vec![last]);
    assert_eq!(name_at(&mut rack, SHOWN - 1, &rows), Some(last), "wanted the picked last row at the bottom of the view");
    rack.pick(&rows, vec![0]);
    assert_eq!(name_at(&mut rack, 0, &rows), Some(0), "wanted the picked first row back at the top");
}

#[test]
fn the_add_cell_waits_after_the_last_row_until_the_rack_scrolls_to_it() {
    let rows = rows(SHOWN + 1);
    let mut rack = Rack::new(W);
    let bottom = (SHOWN as i32 - 1) * PITCH + CELL / 2;
    let mut pressed = rows.clone();
    rack.down(W / 2, bottom, 1, false, offer(), &mut pressed);
    rack.up(W / 2, bottom, &rows, offer());
    assert!(!rack.panel_open(), "pressing the bottom slot while the add cell is out of view opened the panel");
    rack.wheel(-PITCH * 2, &rows);
    let (changed, action) = rack.down(W / 2, bottom, 1, false, offer(), &mut pressed);
    assert!(changed && action.is_none() && rack.panel_open(), "pressing the add cell after the last row did not open the panel");
}
