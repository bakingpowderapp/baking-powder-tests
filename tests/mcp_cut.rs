use bp::app::{App, Opened};
use bp::json::{self, Json};
use bp::mcp::Call;
use bp::project::{Clip, DEFAULT_BPM, Project, TICKS_PER_BEAT, Track};
use bp::wav::Wave;
use std::f64::consts::TAU;
use std::time::Duration;
use support::{HEIGHT, SAMPLE_RATE, WIDTH, call};

const SONG: &str = "/cut/songs/song.json";
const NAME: &str = "strikes";
const BEATS: i32 = 4;
const STRUCK_BEATS: [f64; 3] = [1.0, 2.0, 3.0];
const SOUNDING: usize = 4_800;
const STRIKE_HZ: f64 = 1_000.0;
const STRIKE_PEAK: f64 = 16_384.0;
const SLOWER_FILE_BPM: f64 = 100.0;
const FASTER_FILE_BPM: f64 = 150.0;
const LEAD_BEATS: f64 = 6.0 / TICKS_PER_BEAT as f64;
const FULL_NULL_DB: f64 = -120.0;
const HEAD_SLACK_SECONDS: f64 = 1e-6;
const ONSET_SLACK_BEATS: f64 = 0.004;
const PATIENCE: Duration = Duration::from_secs(120);
const TICK: f32 = 0.01;
const PAUSE: Duration = Duration::from_millis(2);
const WHOLE: &str = "{}";
const CLIPS: &str = r#"{"path":"tracks/0/clips","notes":true}"#;
const TRACKS: &str = r#"{"path":"tracks","notes":true}"#;
const MIDI_CLIP: &str = r#"{"path":"tracks/0/clips","value":[{"startBeat":0,"lengthBeats":8,"notes":[[60,1,0.5,100],[64,3,0.5,100],[67,5,0.5,100]]}]}"#;
const TWIN_CLIP: &str = r#"{"path":"tracks/1/clips","value":[{"startBeat":0,"lengthBeats":8,"notes":[[48,6,0.5,100]]}]}"#;

fn strikes(file_bpm: f64) -> Vec<i16> {
    let frames_per_beat = SAMPLE_RATE * 60.0 / file_bpm;
    let mut pcm = vec![0i16; (f64::from(BEATS) * frames_per_beat) as usize];
    for beat in STRUCK_BEATS {
        let start = (beat * frames_per_beat) as usize;
        for at in 0..SOUNDING {
            pcm[start + at] = (STRIKE_PEAK * (TAU * STRIKE_HZ * at as f64 / SAMPLE_RATE).sin()).round() as i16;
        }
    }
    pcm
}

fn song(file_bpm: f64, warp: bool) -> String {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0] = Track::audio();
    let mut clip = Clip::audio(0, BEATS * TICKS_PER_BEAT, String::from(NAME), String::from(NAME));
    if let Some(file) = &mut clip.file {
        file.warp = warp;
        file.tempo = file_bpm;
    }
    project.tracks[0].clips.push(clip);
    let mut document = project.to_json();
    if let Json::Object(fields) = &mut document {
        fields.push((String::from("waves"), Json::Array(vec![Wave::sound(String::from(NAME), SAMPLE_RATE, 1, strikes(file_bpm)).to_json()])));
    }
    document.write()
}

fn struck(file_bpm: f64, warp: bool) -> App {
    let mut app = App::new();
    app.draw(WIDTH, HEIGHT);
    let opened = app.opened(String::from(SONG), Some(&song(file_bpm, warp)), None);
    assert_eq!(opened, Opened::Project, "wanted the song of three strikes to open, got {opened:?}");
    app.take_requests();
    app
}

fn noted() -> App {
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false}}", Project::new().to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.take_requests();
    wrote(&mut app, MIDI_CLIP);
    app
}

fn wrote(app: &mut App, args: &str) -> String {
    let reply = call(app, "write", args);
    assert!(!reply.failed, "wanted write {args} to land, got {}", reply.text);
    reply.text
}

fn refused(app: &mut App, args: &str) -> String {
    let reply = call(app, "write", args);
    assert!(reply.failed, "wanted write {args} refused, got {}", reply.text);
    reply.text
}

fn read(app: &mut App, args: &str) -> Json {
    let reply = call(app, "read", args);
    assert!(!reply.failed, "wanted read {args} answered, got {}", reply.text);
    json::parse(&reply.text).unwrap_or_else(|| panic!("wanted read to answer JSON, got {}", reply.text))
}

fn listened(app: &mut App, args: &str) -> Json {
    let (listen, waiting) = Call::new("listen", json::parse(args).expect("wanted the arguments to parse"));
    app.deliver(listen);
    let answer = std::thread::spawn(move || waiting.reply(PATIENCE));
    while !answer.is_finished() {
        app.tick(TICK);
        std::thread::sleep(PAUSE);
    }
    let reply = answer.join().expect("wanted the listen answered");
    assert!(!reply.failed, "wanted listen {args} answered, got {}", reply.text);
    json::parse(&reply.text).unwrap_or_else(|| panic!("wanted listen to answer JSON, got {}", reply.text))
}

fn number(json: &Json, key: &str) -> f64 {
    json.get(key).and_then(Json::as_f64).unwrap_or(0.0)
}

fn spans(clips: &Json) -> Vec<(f64, f64)> {
    clips.as_array().unwrap_or(&[]).iter().map(|clip| (number(clip, "startBeat"), number(clip, "lengthBeats"))).collect()
}

fn notes(clips: &Json) -> Vec<Vec<(f64, f64)>> {
    let note = |note: &Json| (number(note, "pitch"), number(note, "startBeat"));
    clips.as_array().unwrap_or(&[]).iter().map(|clip| clip.get("notes").and_then(Json::as_array).unwrap_or(&[]).iter().map(note).collect()).collect()
}

fn heads(clips: &Json) -> Vec<f64> {
    clips.as_array().unwrap_or(&[]).iter().map(|clip| clip.get("wave").map_or(0.0, |wave| number(wave, "headSeconds"))).collect()
}

fn onsets(heard: &Json) -> Vec<f64> {
    heard.get("hits").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(|hit| hit.as_array()?.first()?.as_f64()).collect()
}

fn cuts(struck: &[f64]) -> String {
    format!("{{\"path\":\"tracks/0\",\"cut\":[{}]}}", struck.iter().map(|beat| (beat - LEAD_BEATS).to_string()).collect::<Vec<_>>().join(","))
}

fn null_db(app: &mut App, revision: f64) -> f64 {
    let heard = listened(app, &format!("{{\"against\":{revision}}}"));
    heard.get("null").map(|null| number(null, "peakDb")).unwrap_or_else(|| panic!("wanted the listen against revision {revision} to tell its null, got {}", heard.write()))
}

fn near(got: &[f64], wanted: &[f64]) -> bool {
    got.len() == wanted.len() && got.iter().zip(wanted).all(|(got, wanted)| (got - wanted).abs() <= ONSET_SLACK_BEATS)
}

#[test]
fn cut_splits_the_clip_under_each_beat_and_names_a_beat_no_clip_lay_under() {
    let mut app = noted();
    let told = wrote(&mut app, r#"{"path":"tracks/0","cut":[2,4.5,9]}"#);
    assert!(told.contains("2 clips split in two"), "wanted the reply to count the two cuts made, got {told}");
    assert!(told.contains("no clip to split at 9"), "wanted the reply to name beat 9, past the clip's end, got {told}");
    let clips = read(&mut app, CLIPS);
    assert_eq!(spans(&clips), vec![(0.0, 2.0), (2.0, 2.5), (4.5, 3.5)], "wanted the eight beats in three pieces, cut at beats 2 and 4.5");
    assert_eq!(notes(&clips), vec![vec![(60.0, 1.0)], vec![(64.0, 1.0)], vec![(67.0, 0.5)]], "wanted each note kept at its song beat, counted from its own piece's start");
}

#[test]
fn cut_with_no_path_cuts_every_track_and_a_single_beat_needs_no_list() {
    let mut app = noted();
    wrote(&mut app, TWIN_CLIP);
    let told = wrote(&mut app, r#"{"cut":4}"#);
    assert!(told.contains("2 clips split in two"), "wanted one cut on each of the two tracks, got {told}");
    let tracks = read(&mut app, TRACKS);
    let pieces: Vec<Vec<(f64, f64)>> = tracks.as_array().unwrap_or(&[]).iter().map(|track| spans(track.get("clips").unwrap_or(&Json::Null))).collect();
    assert_eq!(pieces, vec![vec![(0.0, 4.0), (4.0, 4.0)]; 2], "wanted both tracks' clips cut in two at beat 4");
}

#[test]
fn cut_refuses_what_is_not_a_beat_and_a_path_that_is_not_a_track() {
    let mut app = noted();
    for args in [r#"{"path":"tracks/0","cut":"x"}"#, r#"{"path":"tracks/0","cut":[1,-2]}"#, r#"{"path":"tracks/0","cut":0}"#, r#"{"path":"tracks/0","cut":[]}"#] {
        let told = refused(&mut app, args);
        assert!(told.contains("cut is a song beat above 0"), "wanted {args} refused for its beats, got {told}");
    }
    let told = refused(&mut app, r#"{"path":"tracks/0/clips/0","cut":1}"#);
    assert!(told.contains("cut takes tracks/N"), "wanted a cut at a clip's own path refused, naming where it goes, got {told}");
    assert_eq!(spans(&read(&mut app, CLIPS)), vec![(0.0, 8.0)], "wanted the refused cuts to leave the clip whole");
}

#[test]
fn a_cut_wave_clip_plays_every_sample_as_before_each_piece_starting_where_the_last_left_the_file() {
    let mut app = struck(DEFAULT_BPM, false);
    let before = number(&listened(&mut app, WHOLE), "revision");
    wrote(&mut app, r#"{"path":"tracks/0","cut":[1,2.5,3.125]}"#);
    let clips = read(&mut app, CLIPS);
    assert_eq!(spans(&clips), vec![(0.0, 1.0), (1.0, 1.5), (2.5, 0.625), (3.125, 0.875)], "wanted the wave clip in four pieces");
    let seconds_per_beat = 60.0 / DEFAULT_BPM;
    for ((start, _), head) in spans(&clips).into_iter().zip(heads(&clips)) {
        assert!((head - start * seconds_per_beat).abs() <= HEAD_SLACK_SECONDS, "wanted the piece at beat {start} to start {} s into the file, where the piece before it stops, got {head}", start * seconds_per_beat);
    }
    assert_eq!(null_db(&mut app, before), FULL_NULL_DB, "wanted the cut clip to null against the whole one, sample for sample");
}

#[test]
fn a_cut_warped_clip_plays_every_sample_as_before_whether_its_file_runs_slower_or_faster_than_the_song() {
    for file_bpm in [SLOWER_FILE_BPM, FASTER_FILE_BPM] {
        let mut app = struck(file_bpm, true);
        let heard = listened(&mut app, WHOLE);
        assert!(near(&onsets(&heard), &STRUCK_BEATS), "wanted the {file_bpm} bpm file warped onto the song's beats {STRUCK_BEATS:?}, got hits at {:?}", onsets(&heard));
        wrote(&mut app, &cuts(&STRUCK_BEATS));
        let clips = read(&mut app, CLIPS);
        assert_eq!(spans(&clips).len(), 4, "wanted the warped clip in four pieces, got {:?}", spans(&clips));
        for ((start, _), head) in spans(&clips).into_iter().zip(heads(&clips)) {
            assert!((head - start * 60.0 / file_bpm).abs() <= HEAD_SLACK_SECONDS, "wanted the piece at beat {start} of the {file_bpm} bpm file to start {} s into it, got {head}", start * 60.0 / file_bpm);
        }
        assert_eq!(null_db(&mut app, number(&heard, "revision")), FULL_NULL_DB, "wanted the cut {file_bpm} bpm clip to null against the whole one, sample for sample");
    }
}

#[test]
fn warp_off_on_one_piece_keeps_its_hit_in_place_and_leaves_the_other_pieces_alone() {
    let mut app = struck(SLOWER_FILE_BPM, true);
    let warped = onsets(&listened(&mut app, WHOLE));
    wrote(&mut app, &cuts(&STRUCK_BEATS));
    wrote(&mut app, r#"{"path":"tracks/0/clips/2/wave/warp","value":false}"#);
    let after = onsets(&listened(&mut app, WHOLE));
    assert!(near(&after, &warped), "wanted the piece holding the second strike to keep it at beat {} once its warp is off, the other strikes unmoved; got {after:?} against {warped:?}", warped[1]);
}

#[test]
fn a_piece_of_a_faster_file_with_warp_off_plays_on_into_the_next_strike_until_shortened_by_bpm_over_warp_bpm() {
    let mut app = struck(FASTER_FILE_BPM, true);
    wrote(&mut app, &cuts(&STRUCK_BEATS));
    wrote(&mut app, r#"{"path":"tracks/0/clips/1/wave/warp","value":false}"#);
    let piece = spans(&read(&mut app, CLIPS))[1];
    let early = piece.0 + (STRUCK_BEATS[1] - piece.0) * DEFAULT_BPM / FASTER_FILE_BPM;
    let doubled = onsets(&listened(&mut app, WHOLE));
    assert!(near(&doubled, &[STRUCK_BEATS[0], early, STRUCK_BEATS[2]]), "wanted the piece at the file's own speed to reach the file's next strike early, at beat {early:.4}, sounding on into that strike's own piece at beat {} so the two are told as one hit; got {doubled:?}", STRUCK_BEATS[1]);
    wrote(&mut app, &format!("{{\"path\":\"tracks/0/clips/1/lengthBeats\",\"value\":{}}}", piece.1 * DEFAULT_BPM / FASTER_FILE_BPM));
    let clean = onsets(&listened(&mut app, WHOLE));
    assert!(near(&clean, &STRUCK_BEATS), "wanted the piece shortened by bpm over warpBpm to stop short of the file's next strike, each strike heard once; got {clean:?}");
}

#[test]
fn a_write_that_duplicates_a_track_cuts_both_and_drops_pieces_is_one_undo_step() {
    let mut app = struck(DEFAULT_BPM, false);
    let whole = read(&mut app, TRACKS).write();
    wrote(&mut app, r#"{"edits":[{"path":"tracks/0","duplicate":true},{"path":"tracks/0","cut":[1,2,3]},{"path":"tracks/1","cut":[1,2,3]},{"path":"tracks/0/clips/3","value":null},{"path":"tracks/1/clips/0","value":null}]}"#);
    let tracks = read(&mut app, TRACKS);
    let pieces: Vec<usize> = tracks.as_array().unwrap_or(&[]).iter().map(|track| spans(track.get("clips").unwrap_or(&Json::Null)).len()).collect();
    assert_eq!(pieces, vec![3, 3], "wanted two tracks of three pieces each after the one write");
    let undone = call(&mut app, "undo", "{}");
    assert!(!undone.failed, "wanted the write undone, got {}", undone.text);
    assert_eq!(read(&mut app, TRACKS).write(), whole, "wanted one undo to take back the copy, both cuts and both dropped pieces together");
}
