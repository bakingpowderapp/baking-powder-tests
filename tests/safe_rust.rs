use std::fs;
use std::path::{Path, PathBuf};

const REPO: &str = "../Baking Powder";
const CRATES: [&str; 4] = ["sdk", "fx", "coresynth", "bp"];
const FORBID: &str = "unsafe_code = \"forbid\"";

fn crate_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(REPO).join(name)
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("wanted to read {}: {error}", path.display()))
}

#[test]
fn every_crate_manifest_forbids_unsafe_code() {
    for name in CRATES {
        let manifest = crate_dir(name).join("Cargo.toml");
        let text = read(&manifest);
        assert!(text.contains(FORBID), "wanted {FORBID} in {}, found:\n{text}", manifest.display());
    }
}
