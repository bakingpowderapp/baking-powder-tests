use bp::browser::{Item, Seen};
use bp::project::{Preset, Project};
use support::core;

const PATH: &str = "/x/Songs/Song.json";
const STAMP: (u64, u128) = (1_024, 1_700_000_000_000_000_000);
const SAVED_AGAIN: (u64, u128) = (1_024, 1_700_000_000_500_000_000);
const GROWN: (u64, u128) = (2_048, 1_700_000_000_000_000_000);

fn known(seen: &Seen, stamp: (u64, u128)) -> bool {
    matches!(seen.known(PATH, stamp), Some(Item::Json(_)))
}

#[test]
fn a_project_read_once_is_known_until_its_size_or_date_moves() {
    let mut seen = Seen::NONE;
    assert!(!known(&seen, STAMP), "wanted nothing known before the first read");
    seen.keep(PATH, STAMP, &Item::read(&Project::new().to_json().write()));
    assert!(known(&seen, STAMP), "wanted the project known at the size and date it was read with, so its file is not read again");
    assert!(!known(&seen, SAVED_AGAIN), "wanted the project read again once its date moved");
    assert!(!known(&seen, GROWN), "wanted the project read again once its size moved");
}

#[test]
fn a_file_that_is_no_document_is_known_as_one() {
    let mut seen = Seen::NONE;
    seen.keep(PATH, STAMP, &Item::read("{\"rows\": 3}"));
    assert!(known(&seen, STAMP), "wanted a file that reads as neither project nor preset known, so a large one is not read on every listing");
}

#[test]
fn a_preset_is_read_on_every_listing() {
    let mut seen = Seen::NONE;
    seen.keep(PATH, STAMP, &Item::read(&Preset::Row(core(0.5), Vec::new()).to_json().write()));
    assert!(!known(&seen, STAMP), "wanted a preset left out of what is known: its file is small and its rows are held by the browser itself");
}
