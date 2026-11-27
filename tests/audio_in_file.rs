use bp::app::{App, Event, Opened, Request};
use bp::keyboard::Key;
use bp::json::{self, Json};
use bp::wav::Wave;
use support::{HEIGHT, RATE, WIDTH, wav};

const LINE: usize = 1024;
const KICK: &str = "/x/kick.wav";
const OTHER_KICK: &str = "/y/kick.wav";
const PATH: &str = "/tmp/audio-in-file-under-test.json";
const FILE_CELL_CENTRE: (i32, i32) = (60, 26);
const EXPORT_ITEM_CENTRE: (i32, i32) = (60, 325);
const WITHOUT_HISTORY_ITEM_CENTRE: (i32, i32) = (404, 357);

fn kick() -> Vec<i16> {
    (0..(RATE / 2) as i16).map(|frame| frame.wrapping_mul(37)).collect()
}

fn other_kick() -> Vec<i16> {
    (0..1_000).map(|frame| -frame).collect()
}

fn fresh() -> App {
    let mut app = App::new();
    app.draw(WIDTH, HEIGHT);
    app.take_requests();
    app
}

fn reads(app: &mut App) -> Vec<String> {
    app.take_requests().into_iter().filter_map(|request| match request {
        Request::Read { path } => Some(path),
        _ => None,
    }).collect()
}

fn drop_wave(app: &mut App, path: &str, samples: &[i16]) {
    app.dropped(String::from(path), bp::browser::Kind::Wave, WIDTH - 20, HEIGHT - 20);
    let asked = reads(app);
    assert_eq!(asked, vec![String::from(path)], "wanted the drop to ask for {path} once, got {asked:?}");
    app.read(String::from(path), Some(wav(samples)));
    app.take_requests();
}

fn key(app: &mut App, letter: char, shift: bool) {
    app.event(Event::Key { key: Key::Char(letter), shift, command: true, repeat: false });
}

fn press_save(app: &mut App) -> (String, u64) {
    key(app, 's', false);
    let payloads: Vec<(String, u64)> = app.take_requests().into_iter().filter_map(|request| match request {
        Request::Write { json, save, .. } | Request::SaveAs { json, save } => Some((json, save)),
        _ => None,
    }).collect();
    assert_eq!(payloads.len(), 1, "wanted exactly one save request per press, got {}", payloads.len());
    payloads.into_iter().next().expect("a save payload")
}

fn confirm(app: &mut App, save: u64) -> String {
    app.saved(String::from(PATH), save);
    remembered(app, "after the save")
}

fn remembered(app: &mut App, when: &str) -> String {
    app.take_requests().into_iter().filter_map(|request| match request {
        Request::Remember { json } => Some(json),
        _ => None,
    }).next_back().unwrap_or_else(|| panic!("wanted the session remembered {when}"))
}

fn parsed(text: &str) -> Json {
    json::parse(text).expect("wanted the payload to parse")
}

fn waves(file: &Json) -> Vec<Json> {
    file.get("waves").and_then(Json::as_array).map(<[Json]>::to_vec).unwrap_or_default()
}

fn wave_names(file: &Json) -> Vec<String> {
    waves(file).iter().filter_map(|entry| entry.get("name").and_then(Json::as_str).map(String::from)).collect()
}

fn clip_files(file: &Json) -> Vec<Json> {
    file.get("tracks").and_then(Json::as_array).unwrap_or(&[]).iter().flat_map(|track| track.get("clips").and_then(Json::as_array).unwrap_or(&[]).to_vec()).filter_map(|clip| clip.get("wave").cloned()).collect()
}

fn keys_of(object: &Json) -> Vec<String> {
    match object {
        Json::Object(fields) => fields.iter().map(|(key, _)| key.clone()).collect(),
        _ => Vec::new(),
    }
}

fn saved_with_kick() -> (App, String, u64) {
    let mut app = fresh();
    drop_wave(&mut app, KICK, &kick());
    let (text, save) = press_save(&mut app);
    (app, text, save)
}

#[test]
fn a_save_carries_the_dropped_wave_as_lines_of_base64_and_the_clip_names_it() {
    let (_, text, _) = saved_with_kick();
    let file = parsed(&text);
    assert_eq!(keys_of(&file).last().map(String::as_str), Some("waves"), "wanted the pool as the file's last key, got {:?}", keys_of(&file));
    let pool = waves(&file);
    assert_eq!(pool.len(), 1, "wanted one wave in the pool, got {}", pool.len());
    let entry = &pool[0];
    let spelled = (entry.get("name").and_then(Json::as_str), entry.get("sampleRateHz").and_then(Json::as_f64), entry.get("channels").and_then(Json::as_i64), entry.get("sourcePath").and_then(Json::as_str));
    assert_eq!(spelled, (Some("kick"), Some(f64::from(RATE)), Some(1), Some(KICK)), "wanted the entry named for the stem with its rate, channels and path, got {spelled:?}");
    let lines: Vec<&str> = entry.get("pcm16Base64").and_then(Json::as_array).expect("wanted the samples as lines").iter().filter_map(Json::as_str).collect();
    let expected_lines = (kick().len() * 2).div_ceil(LINE / 4 * 3);
    assert_eq!(lines.len(), expected_lines, "wanted {expected_lines} lines of base64, got {}", lines.len());
    assert!(lines[..lines.len() - 1].iter().all(|line| line.len() == LINE), "wanted every line but the last {LINE} characters long, got {:?}", lines.iter().map(|line| line.len()).collect::<Vec<usize>>());
    assert!(lines.iter().all(|line| line.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'='))), "wanted only base64 characters in the lines");
    let read_back = Wave::from_json(entry).expect("wanted the entry to read back");
    assert_eq!((read_back.name.as_str(), read_back.pcm.as_slice()), ("kick", kick().as_slice()), "wanted the samples back word for word");
    let files = clip_files(&file);
    assert_eq!(files.len(), 1, "wanted one audio clip, got {}", files.len());
    assert_eq!((files[0].get("name").and_then(Json::as_str), files[0].get("path")), (Some("kick"), None), "wanted the clip to name its wave and hold no path, got {:?}", files[0]);
}

#[test]
fn a_saved_file_opens_whole_without_asking_the_shell_to_read() {
    let (_, text, _) = saved_with_kick();
    let mut reopened = fresh();
    assert_eq!(reopened.opened(String::from(PATH), Some(&text), None), Opened::Project, "wanted the saved file to open as a project");
    let asked = reads(&mut reopened);
    assert!(asked.is_empty(), "wanted nothing read from disk to open the project, got {asked:?}");
    let again = parsed(&press_save(&mut reopened).0);
    assert_eq!(waves(&again), waves(&parsed(&text)), "wanted the reopened project to save the same pool back");
}

#[test]
fn a_wave_nobody_names_leaves_the_file_and_returns_with_its_clip() {
    let mut app = fresh();
    drop_wave(&mut app, KICK, &kick());
    key(&mut app, 'z', false);
    app.take_requests();
    let without = parsed(&press_save(&mut app).0);
    assert!(clip_files(&without).is_empty() && without.get("waves").is_none(), "wanted no clip and no pool after the drop was undone, got {:?} and {:?}", clip_files(&without), without.get("waves"));
    key(&mut app, 'z', true);
    app.take_requests();
    let with = parsed(&press_save(&mut app).0);
    assert_eq!((clip_files(&with).len(), wave_names(&with)), (1, vec![String::from("kick")]), "wanted the clip and its wave back after redo, got {} clips and {:?}", clip_files(&with).len(), wave_names(&with));
}

#[test]
fn a_kept_save_keeps_its_wave_after_the_clip_is_gone() {
    let (mut app, _, save) = saved_with_kick();
    confirm(&mut app, save);
    key(&mut app, 'z', false);
    app.take_requests();
    let file = parsed(&press_save(&mut app).0);
    assert!(clip_files(&file).is_empty(), "wanted the clip gone from the document, got {:?}", clip_files(&file));
    assert_eq!(wave_names(&file), vec![String::from("kick")], "wanted the wave kept for the save that still names it, got {:?}", wave_names(&file));
}

#[test]
fn relaunch_reads_a_wave_back_out_of_the_project_file_it_was_saved_in() {
    let (mut app, text, save) = saved_with_kick();
    let session = parsed(&confirm(&mut app, save));
    let origins: Vec<(Option<&str>, Option<&str>)> = session.get("origins").and_then(Json::as_array).unwrap_or(&[]).iter().map(|origin| (origin.get("name").and_then(Json::as_str), origin.get("from").and_then(Json::as_str))).collect();
    assert_eq!(origins, vec![(Some("kick"), Some(PATH))], "wanted the session to name the file the wave was saved in, got {origins:?}");
    let mut relaunched = fresh();
    relaunched.resume(&session.write());
    let asked = reads(&mut relaunched);
    assert_eq!(asked, vec![String::from(PATH)], "wanted the relaunch to ask for the project file once, got {asked:?}");
    relaunched.read(String::from(PATH), Some(text.clone().into_bytes()));
    relaunched.take_requests();
    let file = parsed(&press_save(&mut relaunched).0);
    assert_eq!(waves(&file), waves(&parsed(&text)), "wanted the wave back out of the project file, pool for pool");
}

#[test]
fn relaunch_reads_a_wave_back_from_its_wav_under_the_name_that_wanted_it() {
    let mut app = fresh();
    drop_wave(&mut app, OTHER_KICK, &other_kick());
    app.dropped(String::from(KICK), bp::browser::Kind::Wave, WIDTH - 20, HEIGHT - 20);
    reads(&mut app);
    app.read(String::from(KICK), Some(wav(&kick())));
    let session = parsed(&remembered(&mut app, "after the drops"));
    let mut relaunched = fresh();
    relaunched.resume(&session.write());
    let mut asked = reads(&mut relaunched);
    asked.sort();
    assert_eq!(asked, vec![String::from(KICK), String::from(OTHER_KICK)], "wanted both files asked for once each, got {asked:?}");
    relaunched.read(String::from(KICK), Some(wav(&kick())));
    relaunched.read(String::from(OTHER_KICK), Some(wav(&other_kick())));
    relaunched.take_requests();
    let file = parsed(&press_save(&mut relaunched).0);
    let pool: Vec<(String, Vec<i16>)> = waves(&file).iter().filter_map(Wave::from_json).map(|wave| (wave.name, wave.pcm)).collect();
    assert_eq!(pool, vec![(String::from("kick"), other_kick()), (String::from("kick 2"), kick())], "wanted each wave back under the name that wanted it, got {:?}", pool.iter().map(|(name, pcm)| (name.as_str(), pcm.len())).collect::<Vec<(&str, usize)>>());
}

#[test]
fn a_second_file_with_the_same_stem_takes_the_next_number_and_the_same_path_places_the_songs_own_copy() {
    let mut app = fresh();
    drop_wave(&mut app, KICK, &kick());
    drop_wave(&mut app, OTHER_KICK, &other_kick());
    app.dropped(String::from(KICK), bp::browser::Kind::Wave, WIDTH - 20, HEIGHT - 20);
    let asked = reads(&mut app);
    assert!(asked.is_empty(), "wanted the kick the song holds placed again without its file being read, got {asked:?}");
    let file = parsed(&press_save(&mut app).0);
    let clips: Vec<String> = clip_files(&file).iter().filter_map(|file| file.get("name").and_then(Json::as_str).map(String::from)).collect();
    assert_eq!(clips, vec![String::from("kick"), String::from("kick 2"), String::from("kick")], "wanted the clips named kick, kick 2 and kick again, got {clips:?}");
    assert_eq!(wave_names(&file), vec![String::from("kick"), String::from("kick 2")], "wanted two waves in the pool, got {:?}", wave_names(&file));
}

#[test]
fn export_without_history_carries_the_waves() {
    let (mut app, _, _) = saved_with_kick();
    for (x, y) in [FILE_CELL_CENTRE, EXPORT_ITEM_CENTRE, WITHOUT_HISTORY_ITEM_CENTRE] {
        app.event(Event::Move { x, y });
        app.event(Event::Down { x, y, clicks: 1 });
        app.event(Event::Up { x, y });
        app.draw(WIDTH, HEIGHT);
    }
    let exported: Vec<(String, String)> = app.take_requests().into_iter().filter_map(|request| match request {
        Request::Preset { name, json } => Some((name, json)),
        _ => None,
    }).collect();
    assert_eq!(exported.len(), 1, "wanted one export after File, Export Project then Without History, got {}", exported.len());
    let file = parsed(&exported[0].1);
    assert_eq!(wave_names(&file), vec![String::from("kick")], "wanted the exported document to carry its wave, got {:?} in {}", wave_names(&file), exported[0].0);
}

#[test]
fn a_pool_entry_round_trips_any_length_of_samples() {
    for frames in [1usize, 2, 3, 383, 384, 385, 1_536, 1_537] {
        let samples: Vec<i16> = (0..frames).map(|frame| (frame as i16).wrapping_mul(-311)).collect();
        let wave = Wave::sound(String::from("tone"), f64::from(RATE), 1, samples.clone());
        let entry = wave.to_json();
        let lines: Vec<usize> = entry.get("pcm16Base64").and_then(Json::as_array).expect("lines").iter().filter_map(Json::as_str).map(str::len).collect();
        let wanted_lines = (frames * 2).div_ceil(LINE / 4 * 3);
        assert_eq!(lines.len(), wanted_lines, "{frames} frames: wanted {wanted_lines} lines, got {lines:?}");
        assert!(lines.last().is_some_and(|last| *last <= LINE && last % 4 == 0), "{frames} frames: wanted a last line of whole base64 groups within {LINE}, got {lines:?}");
        let back = Wave::from_json(&entry).expect("wanted the entry to read back");
        assert_eq!((back.pcm, back.rate, back.channels), (samples, f64::from(RATE), 1), "{frames} frames: wanted the samples, rate and channels back");
    }
    let stereo = Wave::sound(String::from("pair"), 44_100.0, 2, vec![1, -1, 2, -2, 3]);
    let back = Wave::from_json(&stereo.to_json()).expect("wanted the stereo entry to read back");
    assert_eq!((back.frames(), back.channels), (2, 2), "wanted a dangling sample dropped so whole stereo frames remain, got {} frames in {} channels", back.frames(), back.channels);
}
