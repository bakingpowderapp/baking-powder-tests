use bp::app::{App, Opened, Request};
use bp::audio::vision::Seer;
use bp::audio::{self, Audio};
use bp::json::{self, Json};
use bp::project::{Clip, DEFAULT_BPM, Fade, Fades, Project, Slope, TICKS_PER_BEAT, Track};
use bp::wav::Wave;
use std::path::Path;
use std::sync::Arc;
use support::{BLOCK, HEIGHT, SAMPLE_RATE, WIDTH, call, frames};

const SONG: &str = "/fades/songs/song.json";
const NAME: &str = "tone";
const SECONDS: f64 = 4.0;
const BEATS: i32 = 8;
const LONGER_BEATS: i32 = 12;
const HELD: i16 = 16_384;
const LEVEL: f32 = 0.5;
const CLOSE: f32 = 0.01;
const RISE: Fade = Fade { from: 0.5, to: 1.0, bend: 0.0 };
const FALL: Fade = Fade { from: 2.0, to: 3.0, bend: 0.0 };
const BOTH: Fades = Fades { rise: Some(RISE), fall: Some(FALL) };
const BEFORE_THE_RISE: f64 = 0.25;
const HALFWAY_UP: f64 = 0.75;
const BETWEEN: f64 = 1.5;
const HALFWAY_DOWN: f64 = 2.5;
const AFTER_THE_FALL: f64 = 3.5;
const WAVE: &str = "tracks/0/clips/0/wave";
const CLIP: &str = "tracks/0/clips/0";
const KEPT_DIFFERENTLY: &str = "Kept differently";

fn tone() -> Wave {
    Wave::sound(String::from(NAME), SAMPLE_RATE, 1, vec![HELD; frames(SECONDS)])
}

fn faded(fades: Fades) -> Clip {
    let mut clip = Clip::audio(0, BEATS * TICKS_PER_BEAT, String::from(NAME), String::from(NAME));
    if let Some(file) = &mut clip.file {
        file.fades = fades;
    }
    clip
}

fn project(clip: Clip) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0] = Track::audio();
    project.tracks[0].clips = vec![clip];
    project
}

fn heard(fades: Fades) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(&project(faded(fades)), None, &[], &[Arc::new(tone())]);
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(frames(SECONDS));
    while out.len() < frames(SECONDS) {
        let n = BLOCK.min(frames(SECONDS) - out.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    out
}

fn shaped(what: &str, level_at: impl Fn(f64) -> f32) {
    let wanted = [(BEFORE_THE_RISE, 0.0, "silent before the rise starts"), (HALFWAY_UP, LEVEL / 2.0, "half way up the rise"), (BETWEEN, LEVEL, "at full level between the two fades"), (HALFWAY_DOWN, LEVEL / 2.0, "half way down the fall"), (AFTER_THE_FALL, 0.0, "silent after the fall ends")];
    for (seconds, level, where_) in wanted {
        let got = level_at(seconds);
        assert!((got - level).abs() < CLOSE, "wanted {what} {where_}, {level} at {seconds} s of the file, got {got}");
    }
}

fn reread(project: &Project) -> Project {
    let text = project.to_json().write();
    Project::from_json(&json::parse(&text).expect("wanted the written project to parse")).expect("wanted the written project to read")
}

fn fades_of(project: &Project) -> Fades {
    project.tracks[0].clips[0].file.as_ref().expect("wanted the clip to keep its file").fades
}

fn folder(name: &str) -> String {
    let root = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap_or_else(|error| panic!("wanted {}: {error}", root.display()));
    root.to_string_lossy().into_owned()
}

fn opened(root: Option<&str>, fades: Fades) -> App {
    let mut document = project(faded(fades)).to_json();
    if let Json::Object(fields) = &mut document {
        fields.push((String::from("waves"), Json::Array(vec![tone().to_json()])));
    }
    let mut app = App::new();
    app.draw(WIDTH, HEIGHT);
    if let Some(root) = root {
        app.add_folder(String::from(root));
    }
    let opened = app.opened(String::from(SONG), Some(&document.write()), None);
    assert_eq!(opened, Opened::Project, "wanted the song of one held tone to open, got {opened:?}");
    app.take_requests();
    app
}

fn written(app: &mut App, args: &str) -> String {
    let reply = call(app, "write", args);
    assert!(!reply.failed, "wanted the write taken: {args}, got {}", reply.text);
    reply.text
}

fn read(app: &mut App, path: &str) -> Json {
    let reply = call(app, "read", &format!(r#"{{"path":"{path}"}}"#));
    assert!(!reply.failed, "wanted {path} read, got {}", reply.text);
    json::parse(&reply.text).unwrap_or_else(|| panic!("wanted {path} read as JSON, got {}", reply.text))
}

fn ends(app: &mut App, key: &str) -> (Option<f64>, Option<f64>) {
    let fade = read(app, &format!("{WAVE}/{key}"));
    (fade.get("fromSeconds").and_then(Json::as_f64), fade.get("toSeconds").and_then(Json::as_f64))
}

fn kept_differently(reply: &str) -> Vec<&str> {
    reply.lines().skip_while(|line| !line.starts_with(KEPT_DIFFERENTLY)).skip(1).take_while(|line| line.starts_with(' ')).map(str::trim).collect()
}

fn fade(from: f64, to: f64) -> String {
    format!(r#"{{"fromSeconds":{from},"toSeconds":{to}}}"#)
}

fn exported(app: &mut App) -> Vec<(String, Vec<u8>)> {
    app.take_requests().into_iter().filter_map(|request| if let Request::Exported { path, bytes } = request { Some((path, bytes)) } else { None }).collect()
}

#[test]
fn a_rise_and_a_fall_shape_the_clips_level_in_the_files_seconds() {
    let out = heard(BOTH);
    shaped("the clip", |seconds| out[frames(seconds)]);
    let plain = heard(Fades::NONE);
    for seconds in [BEFORE_THE_RISE, AFTER_THE_FALL] {
        assert_eq!(plain[frames(seconds)], LEVEL, "wanted a clip with no fade at full level {seconds} s in");
    }
}

#[test]
fn a_bend_bows_either_fade_and_one_under_five_milliseconds_plays_over_five() {
    let over = |bend: f32| Fade { from: 1.0, to: 2.0, bend };
    for slope in Slope::BOTH {
        assert_eq!(over(0.0).gain(slope, 1.5), 0.5, "wanted a straight {} half way at its middle", slope.key());
        assert!(over(0.5).gain(slope, 1.5) > 0.5 && over(-0.5).gain(slope, 1.5) < 0.5, "wanted a {} bent up to sit above its straight line and one bent down below it", slope.key());
    }
    let at_once = Fade { from: 1.0, to: 1.0, bend: 0.0 };
    assert_eq!(at_once.ends(Slope::Rise), (0.995, 1.0), "wanted a rise with no length to play over the 5 ms before its end");
    assert_eq!(at_once.ends(Slope::Fall), (1.0, 1.005), "wanted a fall with no length to play over the 5 ms after its start");
}

#[test]
fn the_two_fades_never_cross_and_may_meet() {
    let rise_then_fall = |fall: Fade| Fades::NONE.with(Slope::Rise, Some(RISE)).with(Slope::Fall, Some(fall)).fall;
    let fall_then_rise = |rise: Fade| Fades::NONE.with(Slope::Fall, Some(FALL)).with(Slope::Rise, Some(rise)).rise;
    assert_eq!(fall_then_rise(Fade { from: 0.5, to: 2.5, bend: 0.0 }), Some(Fade { from: 0.5, to: FALL.from, bend: 0.0 }), "wanted a rise set past the fall's start to end there");
    assert_eq!(fall_then_rise(Fade { from: 2.5, to: 2.75, bend: 0.0 }), Some(Fade { from: FALL.from, to: FALL.from, bend: 0.0 }), "wanted a rise set wholly past the fall's start to wait at it");
    assert_eq!(rise_then_fall(Fade { from: 0.75, to: 3.0, bend: 0.0 }), Some(Fade { from: RISE.to, to: 3.0, bend: 0.0 }), "wanted a fall set before the rise's end to start there");
    assert_eq!(rise_then_fall(Fade { from: 0.1, to: 0.2, bend: 0.0 }), Some(Fade { from: RISE.to, to: RISE.to, bend: 0.0 }), "wanted a fall set wholly before the rise's end to wait at it");
    let met = Fade { from: RISE.to, to: 3.0, bend: 0.0 };
    assert_eq!(rise_then_fall(met), Some(met), "wanted a fall that starts where the rise ends kept as set, the two meeting");
    assert_eq!(Fades::NONE.with(Slope::Rise, Some(RISE)).with(Slope::Fall, Some(FALL)), BOTH, "wanted two fades that lie apart kept as set");
}

#[test]
fn a_wave_saves_its_fades_as_fade_in_and_fade_out_and_reads_them_back() {
    let bent = Fades { rise: Some(Fade { bend: 0.5, ..RISE }), fall: Some(Fade { bend: -0.25, ..FALL }) };
    let saved = project(faded(bent));
    assert_eq!(fades_of(&reread(&saved)), bent, "wanted both fades read back as saved, bends with them");
    let wave = saved.to_json().get("tracks").and_then(Json::as_array).and_then(|tracks| tracks[0].get("clips")).and_then(Json::as_array).and_then(|clips| clips[0].get("wave")).cloned().expect("wanted the clip's wave saved");
    let seconds = |key: &str| wave.get(key).map(|fade| (fade.get("fromSeconds").and_then(Json::as_f64), fade.get("toSeconds").and_then(Json::as_f64), fade.get("bend").and_then(Json::as_f64)));
    assert_eq!(seconds("fadeIn"), Some((Some(RISE.from), Some(RISE.to), Some(0.5))), "wanted the rise saved as fadeIn, got {}", wave.write());
    assert_eq!(seconds("fadeOut"), Some((Some(FALL.from), Some(FALL.to), Some(-0.25))), "wanted the fall saved as fadeOut, got {}", wave.write());
    let plain = project(faded(Fades::NONE)).to_json().write();
    assert!(!plain.contains("fadeIn") && !plain.contains("fadeOut"), "wanted a wave with no fade saved without either key, got {plain}");
    let one = Fades { rise: Some(RISE), fall: None };
    assert_eq!(fades_of(&reread(&project(faded(one)))), one, "wanted a rise alone read back alone");
}

#[test]
fn two_fades_read_crossing_in_one_wave_part_where_the_rise_ends() {
    let text = format!(r#"{{"kind":"project","tracks":[{{"kind":"audio","clips":[{{"startBeat":0,"lengthBeats":{BEATS},"wave":{{"name":"{NAME}","fadeIn":{},"fadeOut":{}}}}}]}}]}}"#, fade(0.0, 2.0), fade(1.0, 3.0));
    let read = Project::from_json(&json::parse(&text).expect("wanted the song to parse")).expect("wanted the song to open");
    assert_eq!(fades_of(&read), Fades { rise: Some(Fade { from: 0.0, to: 2.0, bend: 0.0 }), fall: Some(Fade { from: 2.0, to: 3.0, bend: 0.0 }) }, "wanted the fall to give way and start where the rise ends");
}

fn seconds_of(ticks: i32) -> f64 {
    f64::from(ticks) / f64::from(TICKS_PER_BEAT) * 60.0 / DEFAULT_BPM
}

fn heard_until(clip: &Clip) -> Option<f64> {
    clip.file.as_ref().map(|file| file.entry() + seconds_of(clip.length))
}

#[test]
fn a_matched_wave_takes_its_fade_in_from_its_own_start_and_its_fade_out_from_its_own_end() {
    let like = faded(BOTH);
    let mut later = Clip { length: LONGER_BEATS * TICKS_PER_BEAT, ..faded(Fades::NONE) };
    if let Some(file) = &mut later.file {
        file.head = 1.0;
        file.offset = 0.25;
    }
    let by_start = later.file.as_ref().zip(like.file.as_ref()).map(|(theirs, ours)| theirs.entry() - ours.entry()).expect("wanted both clips to hold a file");
    let by_end = heard_until(&later).zip(heard_until(&like)).map(|(theirs, ours)| theirs - ours).expect("wanted both clips heard to an end");
    assert!(by_end > by_start, "wanted the longer clip's end further on than its start, so the two rules differ");
    let moved = |fade: Fade, by: f64| Fade { from: fade.from + by, to: fade.to + by, ..fade };
    let matched = later.matched(&like, heard_until).expect("wanted the matched clip to keep its file");
    assert_eq!(matched.fades, Fades { rise: Some(moved(RISE, by_start)), fall: Some(moved(FALL, by_end)) }, "wanted the fade-in laid as far into the matched wave's sound as into the one it was matched to, and the fade-out ending as far before the matched clip's own end");
    assert_eq!((matched.head, matched.offset), (1.0, 0.25), "wanted the matched wave to keep its own start");
    let unloaded = later.matched(&like, |_| None).expect("wanted the matched clip to keep its file");
    assert_eq!(unloaded.fades, Fades { rise: Some(moved(RISE, by_start)), fall: Some(moved(FALL, by_start)) }, "wanted a clip whose sound is not loaded to take both fades from its own start");
}

#[test]
fn a_clip_bounced_alone_carries_both_fades_in_its_sound() {
    let (wave, lowered) = audio::bounce(String::from("kept"), &[faded(BOTH)], &[Arc::new(tone())], DEFAULT_BPM, 0, BEATS * TICKS_PER_BEAT).expect("wanted the clip bounced");
    assert_eq!((wave.frames(), lowered), (frames(SECONDS), 0.0), "wanted the bounce as long as the clip and not lowered");
    shaped("the bounce", |seconds| f32::from(wave.pcm[frames(seconds)]) / f32::from(i16::MAX));
}

#[test]
fn an_agent_writes_one_audio_clip_as_a_wave_with_its_fades_and_crops_it_by_beats() {
    let root = folder("clip_fades_export");
    let mut app = opened(Some(&root), BOTH);
    let path = format!("{root}/kept.wav");
    let reply = call(&mut app, "export", &format!(r#"{{"path":"{path}","clip":"{CLIP}"}}"#));
    assert!(!reply.failed && reply.text.contains(&format!("{CLIP} as heard, beats 0 to {BEATS}")), "wanted the clip written whole as heard, got {}", reply.text);
    let files = exported(&mut app);
    let [(landed, bytes)] = files.as_slice() else {
        panic!("wanted one file handed to the shell, got {}", files.len());
    };
    assert_eq!(landed, &path, "wanted the wave handed over for the path asked");
    let wave = Wave::parse(String::from("kept"), path.clone(), bytes).expect("wanted the written file to read as a wave");
    assert_eq!(wave.frames(), frames(SECONDS), "wanted the wave as long as the clip");
    shaped("the written wave", |seconds| f32::from(wave.pcm[frames(seconds) * wave.channels]) / f32::from(i16::MAX));
    let cropped = format!("{root}/cropped.wav");
    let reply = call(&mut app, "export", &format!(r#"{{"path":"{cropped}","clip":"{CLIP}","fromBeat":3,"untilBeat":5}}"#));
    assert!(!reply.failed && reply.text.contains("beats 3 to 5"), "wanted the clip written from beat 3 to beat 5, got {}", reply.text);
    let files = exported(&mut app);
    let wave = files.first().and_then(|(_, bytes)| Wave::parse(String::from("cropped"), cropped.clone(), bytes).ok()).expect("wanted the cropped file to read as a wave");
    assert_eq!(wave.frames(), frames(1.0), "wanted two beats of the clip, one second");
    let (first, last) = (f32::from(wave.pcm[0]) / f32::from(i16::MAX), f32::from(wave.pcm[(wave.frames() - 1) * wave.channels]) / f32::from(i16::MAX));
    assert!((first - LEVEL).abs() < CLOSE && (last - LEVEL / 2.0).abs() < CLOSE, "wanted the crop to open at full level, 1.5 s into the file, and close half way down the fall, got {first} and {last}");
    let outside = call(&mut app, "export", &format!(r#"{{"path":"{root}/outside.wav","clip":"{CLIP}","fromBeat":20,"untilBeat":24}}"#));
    assert!(outside.failed && outside.text.contains("lie outside"), "wanted beats past the clip refused, got {}", outside.text);
    assert!(exported(&mut app).is_empty(), "wanted no file handed over for a refused crop");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_fade_an_agent_writes_across_the_other_stops_where_they_meet_and_the_reply_says_what_was_kept() {
    let mut app = opened(None, Fades::NONE);
    let reply = written(&mut app, &format!(r#"{{"path":"{WAVE}/fadeOut","value":{}}}"#, fade(FALL.from, FALL.to)));
    assert!(kept_differently(&reply).is_empty(), "wanted a fade written clear of the other kept as sent, got {reply}");
    let reply = written(&mut app, &format!(r#"{{"path":"{WAVE}/fadeIn","value":{}}}"#, fade(0.5, 2.5)));
    assert_eq!(ends(&mut app, "fadeIn"), (Some(0.5), Some(FALL.from)), "wanted a fadeIn written past fadeOut's fromSeconds to end there");
    assert_eq!(kept_differently(&reply), [format!("{WAVE}/fadeIn/toSeconds: sent 2.5, kept {}", FALL.from)], "wanted the reply to say where the fadeIn was kept, got {reply}");
    let reply = written(&mut app, &format!(r#"{{"path":"{WAVE}/fadeOut","value":{}}}"#, fade(1.0, 3.0)));
    assert_eq!(ends(&mut app, "fadeOut"), (Some(FALL.from), Some(3.0)), "wanted a fadeOut written before fadeIn's toSeconds to start there");
    assert_eq!(kept_differently(&reply), [format!("{WAVE}/fadeOut/fromSeconds: sent 1, kept {}", FALL.from)], "wanted the reply to say where the fadeOut was kept, got {reply}");
    assert_eq!(ends(&mut app, "fadeIn"), (Some(0.5), Some(FALL.from)), "wanted the fadeIn written earlier left where it was");
}

#[test]
fn two_crossing_fades_an_agent_writes_in_one_wave_part_where_the_fade_in_ends() {
    let mut app = opened(None, Fades::NONE);
    let reply = written(&mut app, &format!(r#"{{"path":"{WAVE}","value":{{"name":"{NAME}","fadeIn":{},"fadeOut":{}}}}}"#, fade(0.0, 2.0), fade(1.0, 3.0)));
    assert_eq!((ends(&mut app, "fadeIn"), ends(&mut app, "fadeOut")), ((Some(0.0), Some(2.0)), (Some(2.0), Some(3.0))), "wanted the fadeOut to give way, starting where the fadeIn ends");
    assert_eq!(kept_differently(&reply), [format!("{WAVE}/fadeOut/fromSeconds: sent 1, kept 2")], "wanted the reply to say where the fadeOut was kept, got {reply}");
}
