use bp::app::App;
use bp::json;
use bp::project::Project;
use support::call;

const SONG: &str = r#"{"kind":"project","tracks":[{"kind":"audio"},{"kind":"group"},{"kind":"midi"},{},{"kind":"audio","name":""},{"kind":"group","name":"Bus"}]}"#;
const READ: [(&str, &str); 6] = [("an audio track with no name", "AUDIO"), ("a group track with no name", "GROUP"), ("a MIDI track with no name", "MIDI"), ("a track with no kind and no name", "MIDI"), ("an audio track named with nothing", ""), ("a group track named Bus", "Bus")];
const INSERTED: [(&str, &str); 5] = [(r#"{"kind":"audio"}"#, "AUDIO 3"), (r#"{"kind":"group"}"#, "GROUP 4"), (r#"{"kind":"midi"}"#, "MIDI 5"), ("{}", "MIDI 6"), (r#"{"kind":"audio","name":" "}"#, "AUDIO 7")];
const NAMED: &str = "Named as the app names a new track:";

#[test]
fn a_track_read_without_a_name_takes_its_kinds_word_and_a_name_given_stays() {
    let song = Project::from_json(&json::parse(SONG).expect("wanted the song to parse")).expect("wanted the song to open");
    let names: Vec<&str> = song.tracks.iter().map(|track| track.name.as_str()).collect();
    assert_eq!(names.len(), READ.len(), "wanted the song's {} tracks read, got {names:?}", READ.len());
    let apart: Vec<String> = READ.iter().zip(&names).filter(|((_, wanted), got)| wanted != *got).map(|((what, wanted), got)| format!("{what} read as {got:?}, wanted {wanted:?}")).collect();
    assert!(apart.is_empty(), "wanted each track read under the name it was given or its kind's word, got {apart:#?}");
}

fn name_at(app: &mut App, track: usize) -> Option<String> {
    let read = call(app, "read", &format!(r#"{{"path":"tracks/{track}/name"}}"#));
    json::parse(&read.text).and_then(|name| name.as_str().map(String::from))
}

fn named(reply: &str) -> Vec<&str> {
    reply.lines().skip_while(|line| *line != NAMED).skip(1).take_while(|line| line.starts_with(' ')).map(str::trim).collect()
}

#[test]
fn a_track_an_agent_inserts_without_a_name_takes_its_kinds_word_and_the_next_number() {
    let mut app = App::new();
    let before = Project::new().tracks.len();
    let apart: Vec<String> = INSERTED.iter().enumerate().filter_map(|(at, (track, wanted))| {
        let reply = call(&mut app, "write", &format!(r#"{{"path":"tracks/end","insert":{track}}}"#));
        assert!(!reply.failed, "wanted {track} inserted below the song's tracks, got {}", reply.text);
        assert_eq!(named(&reply.text), [format!("tracks/{}: {wanted}", before + at)], "wanted the reply to give the name {track} took, got {}", reply.text);
        let name = name_at(&mut app, before + at);
        (name.as_deref() != Some(*wanted)).then(|| format!("{track} inserted is named {name:?}, wanted {wanted:?}"))
    }).collect();
    assert!(apart.is_empty(), "wanted each track inserted without a name called by its kind's word and the next number, as the app names a new track, got {apart:#?}");
}

#[test]
fn a_track_written_at_the_end_of_the_list_or_inside_a_whole_list_without_a_name_is_numbered_too() {
    let mut app = App::new();
    let before = Project::new().tracks.len();
    let reply = call(&mut app, "write", &format!(r#"{{"path":"tracks/{before}","value":{{"kind":"audio"}}}}"#));
    assert!(!reply.failed, "wanted an audio track written at the end of the list, got {}", reply.text);
    assert_eq!(named(&reply.text), [format!("tracks/{before}: AUDIO 3")], "wanted the reply to give the name the track took, got {}", reply.text);
    assert_eq!(name_at(&mut app, before).as_deref(), Some("AUDIO 3"), "wanted the track written at the end of the list named by its kind and the next number");
    let reply = call(&mut app, "write", r#"{"path":"tracks","value":[{"kind":"group","name":"Bus"},{"kind":"midi","name":"MIDI 7"},{"kind":"midi"},{"kind":"audio","name":""}]}"#);
    assert!(!reply.failed, "wanted a whole list of tracks written, got {}", reply.text);
    let names: Vec<Option<String>> = (0..4).map(|track| name_at(&mut app, track)).collect();
    let wanted = ["Bus", "MIDI 7", "MIDI 8", "AUDIO 9"].map(|name| Some(String::from(name)));
    assert_eq!(names, wanted, "wanted the named tracks left as named and the two without a name numbered on from the highest number in the list, got {}", reply.text);
    assert_eq!(named(&reply.text), ["tracks/2: MIDI 8", "tracks/3: AUDIO 9"], "wanted the reply to give both names taken and say nothing of the tracks that came named, got {}", reply.text);
}

#[test]
fn a_track_written_with_a_name_keeps_it_and_the_reply_names_none() {
    let mut app = App::new();
    let before = Project::new().tracks.len();
    let reply = call(&mut app, "write", r#"{"path":"tracks/end","insert":{"kind":"audio","name":"Take"}}"#);
    assert!(!reply.failed, "wanted a named audio track inserted, got {}", reply.text);
    assert_eq!(name_at(&mut app, before).as_deref(), Some("Take"), "wanted the name given kept");
    assert!(named(&reply.text).is_empty(), "wanted no name told for a track that came with one, got {}", reply.text);
}
