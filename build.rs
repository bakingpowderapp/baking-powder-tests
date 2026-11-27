use std::fs;
use std::path::Path;

fn main() {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    println!("cargo:rerun-if-changed={}", tests.display());
    let mut names: Vec<String> = fs::read_dir(&tests)
        .expect("wanted the tests folder")
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "rs"))
        .filter_map(|path| path.file_stem()?.to_str().map(String::from))
        .collect();
    names.sort();
    let listing: String = names.iter().map(|name| format!("#[path = {:?}]\nmod {name};\n", tests.join(format!("{name}.rs")))).collect();
    let out = Path::new(&std::env::var("OUT_DIR").expect("wanted OUT_DIR")).join("suite.rs");
    fs::write(out, listing).expect("wanted the suite's module list written");
}
