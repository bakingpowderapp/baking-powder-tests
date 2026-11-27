use bp::browser::{Kind, kind};

const FOLDER: bool = true;
const FILE: bool = false;

fn sorted(path: &str, folder: bool, wanted: Option<Kind>) {
    let got = kind(path, folder);
    assert_eq!(got, wanted, "wanted {path} sorted as {wanted:?}, got {got:?}");
}

#[test]
fn a_folder_is_a_folder_whatever_its_name() {
    sorted("/x/Samples", FOLDER, Some(Kind::Folder));
    sorted("/x/kick.wav", FOLDER, Some(Kind::Folder));
    sorted("/x/song.json", FOLDER, Some(Kind::Folder));
    sorted("/x/Samples/", FOLDER, Some(Kind::Folder));
}

#[test]
fn a_wave_is_known_by_its_extension_in_any_case() {
    sorted("/x/kick.wav", FILE, Some(Kind::Wave));
    sorted("/x/KICK.WAV", FILE, Some(Kind::Wave));
    sorted("/x/Kick.Wav", FILE, Some(Kind::Wave));
}

#[test]
fn a_document_is_known_by_its_extension_in_any_case() {
    sorted("/x/song.json", FILE, Some(Kind::Document));
    sorted("/x/SONG.JSON", FILE, Some(Kind::Document));
}

#[test]
fn other_files_are_passed_over() {
    sorted("/x/notes.txt", FILE, None);
    sorted("/x/kick.aif", FILE, None);
    sorted("/x/wav", FILE, None);
    sorted("/x/json", FILE, None);
    sorted("/x/kick.wav.bak", FILE, None);
}

#[test]
fn hidden_names_are_passed_over_whatever_they_are() {
    sorted("/x/.DS_Store", FILE, None);
    sorted("/x/.kick.wav", FILE, None);
    sorted("/x/.song.json", FILE, None);
    sorted("/x/.git", FOLDER, None);
}

#[test]
fn only_the_last_component_is_judged() {
    sorted("/x/.hidden/kick.wav", FILE, Some(Kind::Wave));
    sorted("/x/song.json/kick.wav", FILE, Some(Kind::Wave));
    sorted("/x/kick.wav/song.json", FILE, Some(Kind::Document));
}
