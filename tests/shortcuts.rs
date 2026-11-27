use bp::app::{App, Event, Request};
use bp::keyboard::Key;

const TAKEN: [char; 11] = ['a', 'c', 'd', 'f', 'g', 'i', 'j', 's', 'u', 'v', 'z'];
const BARE: [Key; 8] = [Key::Space, Key::Return, Key::Escape, Key::Delete, Key::Left, Key::Right, Key::Up, Key::Down];

fn command(app: &mut App, letter: char) -> bool {
    app.event(Event::Key { key: Key::Char(letter), shift: false, command: true, repeat: false })
}

fn besides_remembering(requests: Vec<Request>) -> Vec<Request> {
    requests.into_iter().filter(|request| !matches!(request, Request::Remember { .. })).collect()
}

#[test]
fn the_app_names_the_letters_it_takes_under_command() {
    let taken: Vec<char> = ('a'..='z').filter(|&letter| App::takes_command(Key::Char(letter))).collect();
    assert_eq!(taken, TAKEN, "wanted the command letters {TAKEN:?}, got {taken:?}");
}

#[test]
fn bare_keys_are_never_command_shortcuts() {
    let claimed: Vec<Key> = BARE.into_iter().filter(|&key| App::takes_command(key)).collect();
    assert!(claimed.is_empty(), "wanted no bare key claimed under command, got {claimed:?}");
}

#[test]
fn letters_left_to_the_system_pass_unanswered_under_command() {
    let mut app = App::new();
    app.take_requests();
    for letter in ('a'..='z').filter(|letter| !TAKEN.contains(letter)) {
        let repainted = command(&mut app, letter);
        let requests = besides_remembering(app.take_requests());
        assert!(!repainted && requests.is_empty(), "wanted command-{letter} to pass unanswered, got repaint {repainted} and requests {requests:?}");
    }
}

#[test]
fn saving_is_taken_without_asking_for_a_repaint() {
    let mut app = App::new();
    app.take_requests();
    let repainted = command(&mut app, 's');
    let requests = besides_remembering(app.take_requests());
    assert!(!repainted, "wanted command-s to answer no repaint, got a repaint");
    assert!(matches!(requests.as_slice(), [Request::SaveAs { .. }]), "wanted one Save As request from an unsaved project, got {requests:?}");
}
