use bp::arp::Arp;
use bp::core::{Params, Wave};
use bp::draw::Rect;
use bp::face::{CELL, Face, Faced, H, PITCH, READOUT_ROWS, W, option_rect, panel_rect};
use bp::project::{Project, Row};
use support::{core_layer, midi};

const BPM: f64 = 120.0;
const OSC_ROW: i32 = 1;
const SQUARE_SLOT: i32 = 2;
const SLOT: usize = 1;
const PARABOLA: &str = "PARABOLA";

fn cell_rect(row: i32, column: i32) -> Rect {
    Rect::new(column * PITCH, (READOUT_ROWS + row) * PITCH, CELL, CELL)
}

fn click(patch: &mut Params, face: &mut Face, rect: Rect) -> bool {
    let (x, y) = (rect.x + rect.w / 2, rect.y + rect.h / 2);
    let taken = patch.down(face, x, y, BPM);
    patch.up(face, x, y);
    taken
}

fn offered(patch: &Params, slot: usize) -> Vec<&'static str> {
    let shapes = patch.osc1.shapes;
    Wave::ALL.iter().filter(|wave| **wave == shapes[slot] || !shapes.contains(wave)).map(|wave| Wave::LABELS[wave.index()]).collect()
}

fn setting_at(patch: &Params, face: &Face, row: i32, column: i32) -> Option<(&'static str, &'static str)> {
    let rect = cell_rect(row, column);
    patch.index_at(rect.x + rect.w / 2, rect.y + rect.h / 2, face.view()).and_then(|index| patch.setting_of(index)).map(|(band, param, _)| (band, param))
}

#[test]
fn the_four_wave_cells_are_the_slot_levels_and_a_second_click_opens_the_shape_list() {
    let mut face = Face::new();
    let mut patch = Params::DEFAULT;
    for (column, name) in [(1, "WAVE 1"), (2, "WAVE 2"), (3, "WAVE 3"), (4, "WAVE 4")] {
        assert_eq!(setting_at(&patch, &face, OSC_ROW, column), Some(("OSC 1", name)), "wanted OSC 1's slot level at column {column} of view A");
    }
    let cell = cell_rect(OSC_ROW, SQUARE_SLOT);
    assert!(click(&mut patch, &mut face, cell), "wanted the first click on the square slot taken");
    assert!(!face.panel_open(), "wanted no shape list after one click, the cell merely picked");
    assert!(click(&mut patch, &mut face, cell), "wanted the second click on the picked slot taken");
    assert!(face.panel_open(), "wanted the second click on the picked slot to open its shape list");
    let options = offered(&patch, SLOT);
    assert!(!options.contains(&"SAW") && !options.contains(&"TRIANGLE") && !options.contains(&"SINE") && options.contains(&"SQUARE"), "wanted the list to leave out the shapes the row already uses and keep the slot's own, got {options:?}");
    let panel = panel_rect(cell, &options, 0, Rect::new(0, 0, W, H));
    let parabola = options.iter().position(|option| *option == PARABOLA).expect("wanted PARABOLA offered");
    assert!(click(&mut patch, &mut face, option_rect(panel, parabola)), "wanted the press on PARABOLA taken");
    assert_eq!(patch.osc1.shapes[SLOT], Wave::Parabola, "wanted the square slot to hold a parabola after the pick");
    assert!(!face.panel_open(), "wanted the shape list closed once a shape was picked");
    assert_eq!(patch.osc1.levels, Params::DEFAULT.osc1.levels, "wanted the slot levels untouched by the pick");
    assert!(click(&mut patch, &mut face, cell), "wanted a click on the slot taken again");
    assert!(click(&mut patch, &mut face, cell), "wanted the second click taken again");
    let again = offered(&patch, SLOT);
    assert!(again.contains(&"SQUARE") && !again.contains(&"SAW"), "wanted square offered back and saw still kept out once the slot holds a parabola, got {again:?}");
}

#[test]
fn the_tab_row_holds_views_a_b_and_c() {
    let mut face = Face::new();
    let mut patch = Params::DEFAULT;
    assert!(click(&mut patch, &mut face, cell_rect(0, 1)), "wanted the B button to take the press");
    assert_eq!(face.page(), 1, "wanted the B button to turn the face to view B");
    assert!(click(&mut patch, &mut face, cell_rect(0, 2)), "wanted the C button to take the press");
    assert_eq!(face.page(), 2, "wanted the C button to turn the face to view C");
    assert_eq!(setting_at(&patch, &face, OSC_ROW, 0), None, "wanted the DRIFT title, no setting, under the tab row on view C");
    assert_eq!(setting_at(&patch, &face, OSC_ROW + 1, 0), Some(("ANALOG", "DRIFT")), "wanted drift first on view C, under its title");
    assert_eq!(setting_at(&patch, &face, 0, 3), Some(("VOICE", "VOICES")), "wanted the voice count right of the C button and no fourth view");
}

#[test]
fn slot_shapes_and_levels_round_trip_through_the_project_file() {
    let mut patch = Params::DEFAULT;
    patch.osc1.shapes = [Wave::Saw, Wave::Parabola, Wave::Triangle, Wave::Organ];
    patch.osc1.levels = [0.5, 0.25, 0.0, 1.0];
    patch.osc2.shapes[3] = Wave::Stair;
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(1.0, patch))];
    let text = project.to_json().write();
    assert!(text.contains("\"shapes\": [\"saw\", \"parabola\", \"triangle\", \"organ\"]") && text.contains("\"levels\": [0.5, 0.25, 0, 1]"), "wanted the slot shapes and levels spelled as two arrays, got {text}");
    let back = Project::from_json(&project.to_json()).expect("wanted the project to read back");
    let Row::Core(layer) = back.tracks[0].rack[1] else {
        panic!("wanted the Core row back");
    };
    assert_eq!((layer.patch.osc1.shapes, layer.patch.osc1.levels, layer.patch.osc2.shapes), (patch.osc1.shapes, patch.osc1.levels, patch.osc2.shapes), "wanted every slot's shape and level back from the file");
}
