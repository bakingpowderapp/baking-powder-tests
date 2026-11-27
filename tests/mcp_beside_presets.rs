use bp::app::App;
use bp::arp::Articulation;
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::factory::PRESETS;
use bp::fx::{Kind, Slot};
use bp::json::{self, Json};
use bp::project::{Clip, Note, Project, Row, TICKS_PER_BAR, Track, VELOCITY};
use bp::wav::Wave;
use std::path::Path;
use std::sync::Arc;
use support::{AUDIBLE, BLOCK, SAMPLE_RATE, channels_apart, frames, remembered, session, worst_delta};

const GROWL: &str = "Instruments/Basses/Growl.json";
const CROSS: &str = "Effects/Delay/Cross.json";
const BERTH: &str = "Effects/Space/Berth.json";
const MIDI_TRACK: usize = 1;
const AUDIO_TRACK: usize = 2;
const GROUP_TRACK: usize = 3;
const GROWL_FREAK_ROW: usize = 3;
const FREAK_UNDER_MIDI_ROW: usize = 4;
const CROSS_ROW: usize = 2;
const MANY_ROWS: usize = 21;
const INSERTED_AT: usize = 1;
const SOURCE_BEFORE_INSERT: usize = 1;
const SOURCE_AFTER_INSERT: usize = 2;
const AGENT_PAN_AMOUNT: f64 = 0.2;
const CLICK: &str = "click";
const CLICK_PEAK: i16 = 16_384;
const BASS_PITCH: u8 = 40;
const NOTE_BEATS: f32 = 2.0;
const HEARD_SECONDS: f64 = 1.5;

fn open(name: &str) -> App {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name).join("Factory");
    let _ = std::fs::remove_dir_all(&root);
    for (path, text) in PRESETS.into_iter().filter(|(path, _)| [GROWL, CROSS, BERTH].contains(path)) {
        let file = root.join(path);
        std::fs::create_dir_all(file.parent().expect("wanted a preset's folder")).unwrap_or_else(|error| panic!("wanted {}: {error}", file.display()));
        std::fs::write(&file, text).unwrap_or_else(|error| panic!("wanted {} written: {error}", file.display()));
    }
    let mut project = Project::new();
    project.add_track(AUDIO_TRACK, Track::audio());
    project.add_track(GROUP_TRACK, Track::group());
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false}}", project.to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.factory(root.to_str().map(String::from).expect("wanted the factory's path as text"));
    app.take_requests();
    app
}

fn served(app: &mut App, tool: &str, args: &str) -> (bool, String) {
    let reply = app.serve_call(tool, &json::parse(args).unwrap_or_else(|| panic!("wanted {args} to parse"))).unwrap_or_else(|| panic!("wanted {tool} {args} served at once"));
    (reply.failed, reply.text)
}

fn written(app: &mut App, args: &str) -> String {
    let (failed, reply) = served(app, "write", args);
    assert!(!failed, "wanted the write taken: {args}, got {reply}");
    reply
}

fn read(app: &mut App, path: &str) -> Json {
    let (failed, reply) = served(app, "read", &format!(r#"{{"path":"{path}"}}"#));
    assert!(!failed, "wanted {path} read back, got {reply}");
    json::parse(&reply).unwrap_or_else(|| panic!("wanted {path} read back as JSON, got {reply}"))
}

fn preset(path: &str) -> Json {
    let (_, text) = PRESETS.iter().find(|(known, _)| *known == path).unwrap_or_else(|| panic!("wanted {path} in the factory"));
    json::parse(text).unwrap_or_else(|| panic!("wanted {path} to parse"))
}

fn list<'a>(owner: &'a Json, key: &str) -> &'a [Json] {
    owner.get(key).and_then(Json::as_array).unwrap_or(&[])
}

fn kinds(track: &Json) -> Vec<String> {
    list(track, "rack").iter().filter_map(|row| row.get("kind").and_then(Json::as_str)).map(String::from).collect()
}

fn aim(lane: &Json) -> (Option<f64>, Option<String>) {
    (lane.get("row").and_then(Json::as_f64), lane.get("param").and_then(Json::as_str).map(String::from))
}

fn row_aim(row: usize, param: &str) -> (Option<f64>, Option<String>) {
    (Some(row as f64), Some(String::from(param)))
}

fn counted(reply: &str, at: &str, noun: &str) -> Option<usize> {
    let line = reply.lines().map(str::trim).find(|line| line.starts_with(&format!("{at}: ")))?;
    let before = &line[..line.find(&format!(" {noun}"))?];
    before.rsplit(' ').next()?.parse().ok()
}

fn kept_differently(reply: &str) -> Vec<&str> {
    reply.lines().skip_while(|line| !line.starts_with("Kept differently")).skip(1).take_while(|line| line.starts_with(' ')).map(str::trim).collect()
}

fn played(project: &Project, waves: &[Arc<Wave>]) -> [Vec<f32>; 2] {
    let length = frames(HEARD_SECONDS);
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    audio.sync(project, None, &[], waves);
    audio.play(0.0);
    let (mut left, mut right) = (vec![0.0; length], vec![0.0; length]);
    for (l, r) in left.chunks_mut(BLOCK).zip(right.chunks_mut(BLOCK)) {
        renderer.render(l, r);
    }
    [left, right]
}

fn with_bass_note(mut project: Project) -> Project {
    let note = Note { pitch: BASS_PITCH, start: 0.0, length: NOTE_BEATS, velocity: VELOCITY, articulation: Articulation::PLAIN };
    project.tracks[MIDI_TRACK].clips.push(Clip { start: 0, length: TICKS_PER_BAR, label: String::new(), notes: vec![note], file: None });
    project
}

fn clicked(mut project: Project) -> [Vec<f32>; 2] {
    let mut pcm = vec![0; frames(HEARD_SECONDS)];
    pcm[0] = CLICK_PEAK;
    let click = Arc::new(Wave::sound(String::from(CLICK), SAMPLE_RATE, 1, pcm));
    project.tracks[AUDIO_TRACK].clips.push(Clip::audio(0, TICKS_PER_BAR, String::from(CLICK), String::from(CLICK)));
    played(&project, &[click])
}

fn field<'a>(json: &'a mut Json, key: &str) -> Option<&'a mut Json> {
    let Json::Object(fields) = json else {
        return None;
    };
    fields.iter_mut().find(|(known, _)| known == key).map(|(_, value)| value)
}

fn without_lane(mut project: Json, param: &str) -> Json {
    if let Some(Json::Array(tracks)) = field(&mut project, "tracks")
        && let Some(Json::Array(lanes)) = tracks.get_mut(MIDI_TRACK).and_then(|track| field(track, "lfos"))
    {
        lanes.retain(|lane| lane.get("param").and_then(Json::as_str) != Some(param));
    }
    project
}

fn bass_heard(project: &Json) -> [Vec<f32>; 2] {
    played(&with_bass_note(Project::from_json(project).expect("wanted the remembered project to read")), &[])
}

fn assert_lane_heard(app: &mut App, param: &str, what: &str) {
    let project = session(app).get("project").cloned().expect("wanted the remembered session to hold the project");
    let (with, without) = (bass_heard(&project), bass_heard(&without_lane(project, param)));
    let apart = worst_delta(&with[0], &without[0]);
    assert!(apart > AUDIBLE, "wanted {what} heard against the same sound without it, the two differ by at most {apart}");
}

fn delay_beside_growl(track: usize, kind: &str) -> App {
    let mut app = open(&format!("beside_rack_{kind}"));
    written(&mut app, &format!(r#"{{"path":"tracks/{track}","value":{{"kind":"{kind}","preset":"{GROWL}","rack":[{{"kind":"delay"}}]}},"details":true}}"#));
    let landed = read(&mut app, &format!("tracks/{track}"));
    assert_eq!(kinds(&landed), vec!["delay"], "wanted the one delay written beside the preset kept once on the {kind} track, got {}", landed.write());
    app
}

#[test]
fn a_rack_written_beside_a_sound_on_an_audio_or_group_track_lands_once() {
    delay_beside_growl(GROUP_TRACK, "group");
    let project = remembered(&mut delay_beside_growl(AUDIO_TRACK, "audio"));
    let mut one_delay = project.clone();
    one_delay.tracks[AUDIO_TRACK].rack = vec![Row::Effect(Slot::new(Kind::Delay))];
    let apart = channels_apart(&clicked(project), &clicked(one_delay));
    assert_eq!(apart, [None, None], "wanted a click on the audio track to echo as through one delay, first frames apart {apart:?}");
}

#[test]
fn a_rack_of_twenty_one_rows_written_beside_a_sound_is_taken_whole() {
    let mut app = open("beside_rack_many");
    let rack = format!("[{}]", vec![r#"{"kind":"delay"}"#; MANY_ROWS].join(","));
    let (failed, reply) = served(&mut app, "write", &format!(r#"{{"path":"tracks/{AUDIO_TRACK}","value":{{"kind":"audio","preset":"{GROWL}","rack":{rack}}},"details":true}}"#));
    assert!(!failed, "wanted a rack of {MANY_ROWS} rows written beside the preset taken, got {reply}");
    let landed = read(&mut app, &format!("tracks/{AUDIO_TRACK}"));
    assert_eq!(kinds(&landed), vec!["delay"; MANY_ROWS], "wanted the {MANY_ROWS} written delays as the track's rack, got {:?}", kinds(&landed));
}

#[test]
fn an_lfo_written_beside_a_sound_lands_on_the_presets_row_and_is_heard() {
    let mut app = open("beside_lfo");
    let reply = written(&mut app, &format!(r#"{{"path":"tracks/{MIDI_TRACK}","value":{{"preset":"{GROWL}","lfos":[{{"row":{GROWL_FREAK_ROW},"param":"driveDb","amount":0.5,"tempoSync":true,"division":"1/4"}}]}},"details":true}}"#));
    let landed = read(&mut app, &format!("tracks/{MIDI_TRACK}"));
    assert_eq!(kinds(&landed).get(FREAK_UNDER_MIDI_ROW).map(String::as_str), Some("freak"), "wanted the preset's freak row under the track's MIDI row, got {:?}", kinds(&landed));
    let lanes: Vec<_> = list(&landed, "lfos").iter().map(aim).collect();
    assert_eq!(lanes, vec![row_aim(FREAK_UNDER_MIDI_ROW, "driveDb")], "wanted the lane written beside the preset on its freak row's drive, in place of the preset's own, got {lanes:?}");
    assert_eq!(counted(&reply, &format!("tracks/{MIDI_TRACK}"), "LFO lane"), Some(1), "wanted the reply to count the one lane that landed, got {reply}");
    assert_lane_heard(&mut app, "driveDb", "the lane on the freak's drive");
}

#[test]
fn a_track_inserted_with_a_wired_row_preset_keeps_it_and_its_moved_source() {
    let mut app = open("beside_insert_wired");
    let name = |track: &Json| track.get("name").and_then(Json::as_str).map(String::from);
    let source = name(&read(&mut app, &format!("tracks/{SOURCE_BEFORE_INSERT}"))).expect("wanted the source track named");
    written(&mut app, &format!(r#"{{"path":"tracks/{INSERTED_AT}","insert":{{"kind":"audio","rack":[{{"preset":"{BERTH}","sourceTrack":{SOURCE_BEFORE_INSERT}}}]}},"details":true}}"#));
    let inserted = read(&mut app, &format!("tracks/{INSERTED_AT}"));
    let rows: Vec<(Option<&str>, Option<f64>, Option<f64>)> = list(&inserted, "rack").iter().map(|row| (row.get("kind").and_then(Json::as_str), row.get("sourceTrack").and_then(Json::as_f64), row.get("depthDb").and_then(Json::as_f64))).collect();
    let wanted = vec![(Some("space"), Some(SOURCE_AFTER_INSERT as f64), preset(BERTH).get("depthDb").and_then(Json::as_f64))];
    assert_eq!(rows, wanted, "wanted the inserted track's Berth row kept with its source moved down with the track, got {}", inserted.write());
    let moved = name(&read(&mut app, &format!("tracks/{SOURCE_AFTER_INSERT}")));
    assert_eq!(moved.as_deref(), Some(source.as_str()), "wanted {source} one place down after the insert, got {moved:?} there");
}

#[test]
fn an_lfo_written_beside_a_nested_row_preset_lands_beside_its_own_and_wins_its_knob() {
    let mut app = open("beside_nested_lfo");
    let lanes = format!(r#"[{{"row":{CROSS_ROW},"param":"feedback","amount":0.5,"tempoSync":true,"division":"1/4"}},{{"row":{CROSS_ROW},"param":"wetPan","amount":{AGENT_PAN_AMOUNT},"tempoSync":true,"division":"1/8"}}]"#);
    written(&mut app, &format!(r#"{{"path":"tracks/{MIDI_TRACK}","value":{{"rack":[{{"kind":"midi"}},{{"kind":"core"}},{{"preset":"{CROSS}"}}],"lfos":{lanes}}},"details":true}}"#));
    let landed = read(&mut app, &format!("tracks/{MIDI_TRACK}"));
    let mut aims: Vec<_> = list(&landed, "lfos").iter().map(aim).collect();
    aims.sort_by(|a, b| a.1.cmp(&b.1));
    let cross_own: Vec<_> = list(&preset(CROSS), "lfos").iter().filter_map(|lane| lane.get("param").and_then(Json::as_str)).filter(|param| *param != "wetPan").map(|param| row_aim(CROSS_ROW, param)).collect();
    let mut wanted: Vec<_> = cross_own.into_iter().chain([row_aim(CROSS_ROW, "feedback"), row_aim(CROSS_ROW, "wetPan")]).collect();
    wanted.sort_by(|a, b| a.1.cmp(&b.1));
    assert_eq!(aims, wanted, "wanted the written lanes kept beside the delay preset's own, one lane to a knob, got {}", landed.write());
    let pan: Vec<Option<f64>> = list(&landed, "lfos").iter().filter(|lane| aim(lane) == row_aim(CROSS_ROW, "wetPan")).map(|lane| lane.get("amount").and_then(Json::as_f64)).collect();
    assert_eq!(pan, vec![Some(AGENT_PAN_AMOUNT)], "wanted the written pan lane to win over the preset's own on the same knob, got {pan:?}");
    assert_lane_heard(&mut app, "feedback", "the written lane on the delay's feedback");
}

#[test]
fn an_inserted_track_carries_follows_as_written() {
    let mut app = open("beside_follows_midi");
    written(&mut app, &format!(r#"{{"path":"tracks/{INSERTED_AT}","insert":{{"kind":"midi","preset":"{GROWL}","follows":{SOURCE_BEFORE_INSERT}}},"details":true}}"#));
    let follows = read(&mut app, &format!("tracks/{INSERTED_AT}")).get("follows").and_then(Json::as_f64);
    assert_eq!(follows, Some(SOURCE_AFTER_INSERT as f64), "wanted the inserted MIDI track to follow the track it named, one place down after the insert, got {follows:?}");
    let mut app = open("beside_follows_audio");
    let reply = written(&mut app, &format!(r#"{{"path":"tracks/{INSERTED_AT}","insert":{{"kind":"audio","preset":"{GROWL}","follows":{SOURCE_BEFORE_INSERT}}},"details":true}}"#));
    let refused = kept_differently(&reply);
    assert!(refused.iter().any(|line| line.starts_with(&format!("tracks/{INSERTED_AT}/follows"))), "wanted the reply to say the audio track's follows was not kept, got {reply}");
}
