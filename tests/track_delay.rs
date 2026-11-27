use bp::app::App;
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer, Tail};
use bp::fx::params::{Freak, FreakSlope, Lookahead};
use bp::fx::{Kind, Params, Slot};
use bp::json::{self, Json};
use bp::project::{Clip, Project, Row, TICKS_PER_BEAT, Timing, Track};
use bp::wav::Wave;
use std::f64::consts::TAU;
use std::sync::Arc;
use std::thread::{self, ScopedJoinHandle};
use std::panic;
use support::{BLOCK, SAMPLE_RATE, beat_frame};

const CLICK_NAME: &str = "click";
const TONE_NAME: &str = "tone";
const CLICK: i16 = 8_192;
const QUIET_CLICK: i16 = 2_048;
const LEVEL: f32 = 0.25;
const QUIET: f32 = 0.0625;
const TONE_HZ: f64 = 1_000.0;
const WAVE_SECONDS: usize = 10;
const CLIP_BEATS: i32 = 16;
const SPAN_BEATS: i32 = 4;
const LISTEN: usize = 40_000;
const AT: usize = 10_000;
const EDGE: usize = 200;
const TEN_MS: f32 = 10.0;
const TEN_MS_F64: f64 = 10.0;
const FIVE_MS: f32 = 5.0;
const MOST_MS: f32 = 100.0;
const UNHEARD_MS: f32 = 0.004;
const TEN_MS_FRAMES: usize = 480;
const FIVE_MS_FRAMES: usize = 240;
const MOST_FRAMES: usize = 4_800;
const LIMITER_MS: f64 = 1.5;
const LIMITER: usize = 71;
const LIN: usize = 431;
const EXACT: f32 = 1e-6;
const CLOSE: f32 = 1e-4;
const DIP: f32 = 1e-3;
const NEAR: usize = 2;
const SHOWN: usize = 12;
const HEADER: usize = 68;
const BYTES_PER_FRAME: usize = 6;
const SCALE: f32 = 8_388_608.0;
const DUCK_FROM_DB: f64 = -60.0;
const DUCK_RATIO: f64 = 20.0;
const FAST_MS: f64 = 0.1;
const SENT: f32 = 1.0;
const RETURN: usize = 0;
const SOURCE: usize = 0;
const DUCKED: usize = 1;
const GROUP: usize = 0;
const FIRST: usize = 1;
const SECOND: usize = 2;
const OUTSIDE: usize = 3;
const FIRST_GAIN: f32 = 1.0;
const SECOND_GAIN: f32 = 0.5;
const OUTSIDE_GAIN: f32 = 0.25;
const PLAIN: usize = 0;
const LIMITED: usize = 1;
const GROUPED: usize = 2;
const MEMBER: usize = 3;
const SENDER: usize = 4;
const KEYED: usize = 5;
const OTHER_GROUP: usize = 4;
const OTHER_MEMBER: usize = 5;
const TRACK_PATH: &str = "tracks/1/delayMs";

fn limiter(ms: f64) -> Row {
    let mut slot = Slot::new(Kind::Utility);
    if let Params::Utility(p) = &mut slot.params {
        p.limit = true;
        p.hard = false;
        p.ceiling_db = 0.0;
        p.lookahead_ms = ms;
    }
    Row::Effect(slot)
}

fn linear() -> Row {
    Row::Effect(Slot { params: Params::Freak(Freak { slope: FreakSlope::Linear, oversample: false, ..Freak::DEFAULT }), ..Slot::new(Kind::Freak) })
}

fn ducker(source: usize, pre: bool) -> Row {
    let mut slot = Slot::new(Kind::Sidechain);
    if let Params::Sidechain(p) = &mut slot.params {
        p.threshold_db = DUCK_FROM_DB;
        p.ratio = DUCK_RATIO;
        p.attack_ms = FAST_MS;
        p.lookahead = Lookahead::Off;
    }
    slot.wire(Some(source), pre);
    Row::Effect(slot)
}

fn clicks_at(level: i16, at: &[usize]) -> Arc<Wave> {
    let mut pcm = vec![0i16; WAVE_SECONDS * SAMPLE_RATE as usize];
    for &frame in at {
        pcm[frame] = level;
    }
    Arc::new(Wave::sound(String::from(CLICK_NAME), SAMPLE_RATE, 1, pcm))
}

fn click() -> Arc<Wave> {
    clicks_at(CLICK, &[AT])
}

fn tone() -> Arc<Wave> {
    let pcm = (0..WAVE_SECONDS * SAMPLE_RATE as usize).map(|frame| ((TAU * TONE_HZ * frame as f64 / SAMPLE_RATE).cos() * f64::from(CLICK)).round() as i16).collect();
    Arc::new(Wave::sound(String::from(TONE_NAME), SAMPLE_RATE, 1, pcm))
}

fn playing(name: &str, rack: Vec<Row>, delay_ms: f32) -> Track {
    let mut track = Track { rack, delay_ms, ..Track::audio() };
    track.clips.push(Clip::audio(0, CLIP_BEATS * TICKS_PER_BEAT, String::from(name), String::from(name)));
    track
}

fn clicking(rack: Vec<Row>, delay_ms: f32) -> Track {
    playing(CLICK_NAME, rack, delay_ms)
}

fn project(tracks: Vec<Track>) -> Project {
    let mut project = Project::new();
    project.tracks.clear();
    for (at, track) in tracks.into_iter().enumerate() {
        project.add_track_outside(at, track);
    }
    project
}

fn grouped(group_ms: f32, first_ms: f32) -> Project {
    grouped_with(group_ms, first_ms, second_clicking(), outside_clicking())
}

fn grouped_with(group_ms: f32, first_ms: f32, second: Track, outside: Track) -> Project {
    let mut project = Project::new();
    project.tracks.clear();
    project.add_track(GROUP, Track { delay_ms: group_ms, ..Track::group() });
    project.add_track(FIRST, Track { gain: FIRST_GAIN, ..clicking(Vec::new(), first_ms) });
    project.add_track(SECOND, second);
    project.add_track_outside(OUTSIDE, outside);
    let inside = project.members_of(GROUP);
    assert_eq!(inside, vec![FIRST, SECOND], "wanted tracks {FIRST} and {SECOND} inside the group and track {OUTSIDE} outside it, got members {inside:?}");
    project
}

fn second_clicking() -> Track {
    Track { gain: SECOND_GAIN, ..clicking(Vec::new(), 0.0) }
}

fn outside_clicking() -> Track {
    Track { gain: OUTSIDE_GAIN, ..clicking(Vec::new(), 0.0) }
}

fn member_sending(group_ms: f32, first_ms: f32) -> Project {
    let mut project = grouped(group_ms, first_ms);
    project.tracks[FIRST].sends[RETURN] = SENT;
    project.sends[RETURN].rack = vec![limiter(LIMITER_MS)];
    project
}

fn keyed_outside(group_ms: f32, first_ms: f32, pre: bool) -> Project {
    grouped_with(group_ms, first_ms, second_clicking(), playing(TONE_NAME, vec![ducker(FIRST, pre)], 0.0))
}

fn keyed_on_master(group_ms: f32, first_ms: f32) -> Project {
    let mut project = grouped_with(group_ms, first_ms, second_clicking(), playing(TONE_NAME, Vec::new(), 0.0));
    project.master.rack = vec![ducker(FIRST, false)];
    project
}

fn keyed_in_another_group(group_ms: f32, first_ms: f32) -> Project {
    let mut project = grouped(group_ms, first_ms);
    project.add_track_outside(OTHER_GROUP, Track::group());
    project.add_track(OTHER_MEMBER, playing(TONE_NAME, vec![ducker(FIRST, false)], 0.0));
    let inside = project.members_of(OTHER_GROUP);
    assert_eq!(inside, vec![OTHER_MEMBER], "wanted track {OTHER_MEMBER} inside a second group, got members {inside:?}");
    project
}

fn keyed_beside(group_ms: f32, first_ms: f32, pre: bool) -> Project {
    grouped_with(group_ms, first_ms, Track { gain: SECOND_GAIN, ..playing(TONE_NAME, vec![ducker(FIRST, pre)], 0.0) }, outside_clicking())
}

fn keyed_on_group(group_ms: f32, first_ms: f32, pre: bool) -> Project {
    let mut project = grouped_with(group_ms, first_ms, Track { gain: SECOND_GAIN, ..playing(TONE_NAME, Vec::new(), 0.0) }, outside_clicking());
    project.tracks[GROUP].rack = vec![ducker(FIRST, pre)];
    project
}

fn keyed(source_ms: f32, pre: bool) -> Project {
    project(vec![clicking(Vec::new(), source_ms), playing(TONE_NAME, vec![ducker(SOURCE, pre)], 0.0)])
}

fn everything(delays: [f32; 6]) -> Project {
    let mut project = Project::new();
    project.tracks.clear();
    project.add_track_outside(PLAIN, clicking(Vec::new(), delays[PLAIN]));
    project.add_track_outside(LIMITED, clicking(vec![limiter(LIMITER_MS), linear()], delays[LIMITED]));
    project.add_track_outside(GROUPED, Track { rack: vec![linear()], delay_ms: delays[GROUPED], ..Track::group() });
    project.add_track(MEMBER, clicking(vec![limiter(LIMITER_MS)], delays[MEMBER]));
    let mut sender = clicking(Vec::new(), delays[SENDER]);
    sender.sends[RETURN] = SENT;
    project.add_track(SENDER, sender);
    project.add_track_outside(KEYED, Track { rack: vec![ducker(LIMITED, false)], delay_ms: delays[KEYED], ..Track::audio() });
    project.sends[RETURN].rack = vec![limiter(LIMITER_MS)];
    let inside = project.members_of(GROUPED);
    assert_eq!(inside, vec![MEMBER, SENDER], "wanted tracks {MEMBER} and {SENDER} inside the group, got members {inside:?}");
    project
}

fn song() -> usize {
    beat_frame(SPAN_BEATS as usize)
}

fn live(project: &Project, waves: &[Arc<Wave>]) -> (Vec<f32>, Timing) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, None, &[], waves);
    let timing = audio.timing().clone();
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(LISTEN + BLOCK);
    while out.len() < LISTEN {
        renderer.render(&mut l, &mut r);
        out.extend_from_slice(&l);
    }
    out.truncate(LISTEN);
    (out, timing)
}

type Played = (Vec<f32>, Timing);
type Bounced = Option<Vec<u8>>;

fn bounced(project: &Project, waves: &[Arc<Wave>], from_beat: i32) -> Bounced {
    let span = (from_beat * TICKS_PER_BEAT, (from_beat + SPAN_BEATS) * TICKS_PER_BEAT);
    support::bounce(project, waves, span, Tail::Cut)
}

fn exported(wav: Bounced) -> Vec<f32> {
    let wav = wav.expect("wanted the project to export");
    wav[HEADER..].as_chunks::<BYTES_PER_FRAME>().0.iter().map(|frame| (i32::from_le_bytes([0, frame[0], frame[1], frame[2]]) >> 8) as f32 / SCALE).collect()
}

fn export(project: &Project, waves: &[Arc<Wave>], from_beat: i32) -> Vec<f32> {
    exported(bounced(project, waves, from_beat))
}

fn joined<T>(handle: ScopedJoinHandle<'_, T>) -> T {
    handle.join().unwrap_or_else(|panic| panic::resume_unwind(panic))
}

fn played_and_bounced<const N: usize>(projects: [&Project; N], waves: &[Arc<Wave>]) -> ([Played; N], [Bounced; N]) {
    thread::scope(|scope| {
        let bounces = scope.spawn(|| projects.map(|project| bounced(project, waves, 0)));
        let plays = projects.map(|project| scope.spawn(move || live(project, waves)));
        (plays.map(joined), joined(bounces))
    })
}

fn heard(out: &[f32], floor: f32) -> Vec<(usize, f32)> {
    out.iter().enumerate().filter(|(_, sample)| sample.abs() > floor).map(|(frame, &sample)| (frame, sample)).collect()
}

fn lands(out: &[f32], wanted: &[(usize, f32)], within: f32, what: &str) {
    let mut wanted = wanted.to_vec();
    wanted.sort_by_key(|&(frame, _)| frame);
    let got = heard(out, within);
    let alike = got.len() == wanted.len() && got.iter().zip(&wanted).all(|(&(frame, level), &(at, height))| frame == at && (level - height).abs() <= within);
    assert!(alike, "{what}: wanted clicks at {wanted:?} as (frame, level) and nothing else above {within}, got {} frames above it, the first {:?}", got.len(), &got[..got.len().min(SHOWN)]);
}

fn hears(project: &Project, waves: &[Arc<Wave>], output: usize, live_at: &[(usize, f32)], export_at: &[(usize, f32)], within: f32, what: &str) {
    let ([(out, timing)], [file]) = played_and_bounced([project], waves);
    assert_eq!(timing.output, output, "{what}, live output: wanted the output latency at {output}, got {}", timing.output);
    lands(&out, live_at, within, &format!("{what}, live output"));
    let file = exported(file);
    assert_eq!(file.len(), song(), "{what}, trimmed export: wanted the span's {} frames, got {}", song(), file.len());
    lands(&file, export_at, within, &format!("{what}, trimmed export"));
}

fn plans(project: &Project, wanted: &[(isize, usize, usize)], output: usize, what: &str) -> Timing {
    let timing = Timing::of(project, SAMPLE_RATE);
    let got: Vec<(isize, usize, usize)> = timing.tracks.iter().map(|track| (track.delay, track.latency, track.to_master)).collect();
    assert_eq!(got, wanted, "{what}, the plan: wanted each track's delay, latency and to_master at {wanted:?}, got {got:?}");
    assert_eq!(timing.output, output, "{what}, the plan: wanted the output latency at {output}, got {}", timing.output);
    timing
}

fn unkeyed(project: &Project) -> Project {
    let mut plain = project.clone();
    for row in plain.tracks.iter_mut().flat_map(|track| track.rack.iter_mut()).chain(plain.master.rack.iter_mut()) {
        if let Row::Effect(slot) = row
            && slot.kind() == Kind::Sidechain
        {
            slot.wire(None, false);
        }
    }
    plain
}

fn first_dip(keyed: &[f32], plain: &[f32]) -> Option<usize> {
    keyed.iter().zip(plain).position(|(a, b)| (a - b).abs() > DIP)
}

fn ducks(project: &Project, waves: &[Arc<Wave>], live_frame: usize, export_frame: usize, what: &str) {
    let without_key = unkeyed(project);
    let ([(keyed, timing), (plain, plain_timing)], files) = played_and_bounced([project, &without_key], waves);
    assert_eq!(timing.output, plain_timing.output, "{what}, live output: wanted the keyed and unkeyed projects at one output latency, got {} and {}", timing.output, plain_timing.output);
    let dip = first_dip(&keyed, &plain);
    assert!(dip.is_some_and(|dip| dip.abs_diff(live_frame) <= NEAR), "{what}, live output: wanted the duck to begin at frame {live_frame}, got {dip:?}");
    let [keyed, plain] = files.map(exported);
    let dip = first_dip(&keyed, &plain);
    assert!(dip.is_some_and(|dip| dip.abs_diff(export_frame) <= NEAR), "{what}, trimmed export: wanted the duck to begin at frame {export_frame}, got {dip:?}");
}

fn fed_returns(project: &Project, wanted: (usize, usize, usize), what: &str) {
    let timing = Timing::of(project, SAMPLE_RATE);
    let got = (timing.tracks[FIRST].to_returns, timing.returns_in, timing.output);
    assert_eq!(got, wanted, "{what}, the plan: wanted the member fed {} late into the returns, returns_in at {} and the output at {}, got to_returns, returns_in and the output {got:?}", wanted.0, wanted.1, wanted.2);
}

fn track_key(project: &Project, track: usize) -> (usize, usize, usize) {
    let timing = Timing::of(project, SAMPLE_RATE);
    let row = &timing.tracks[track].rack.rows[0];
    (row.signal, row.before, timing.output)
}

fn master_key(project: &Project) -> (usize, usize, usize) {
    let timing = Timing::of(project, SAMPLE_RATE);
    let row = &timing.master.rows[0];
    (row.signal, row.before, timing.output)
}

fn tap(pre: bool) -> &'static str {
    if pre { "pre" } else { "post" }
}

fn reopen(text: &str) -> Project {
    Project::from_json(&json::parse(text).unwrap_or_else(|| panic!("wanted {text} to parse"))).unwrap_or_else(|| panic!("wanted {text} to open as a project"))
}

fn number(text: &str) -> Option<f64> {
    json::parse(text).as_ref().and_then(Json::as_f64)
}

fn resumed(project: &Project) -> (App, Renderer) {
    let session = format!("{{\"project\":{},\"automation\":false,\"keyboard\":false,\"palette\":false}}", project.to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    let renderer = app.audio_started(SAMPLE_RATE, BLOCK, BLOCK as u32);
    app.take_requests();
    (app, renderer)
}

fn served(app: &mut App, tool: &str, args: &str) -> String {
    let reply = app.serve_call(tool, &json::parse(args).unwrap_or_else(|| panic!("wanted {args} to parse"))).unwrap_or_else(|| panic!("wanted {tool} {args} served at once"));
    assert!(!reply.failed, "wanted {tool} {args} answered, got {}", reply.text);
    reply.text
}

fn document(app: &mut App) -> Json {
    let text = served(app, "read", "{}");
    json::parse(&text).unwrap_or_else(|| panic!("wanted read to answer JSON, got {text}"))
}

fn output_samples(app: &mut App) -> f64 {
    let document = document(app);
    document.get("latency").and_then(|latency| latency.get("outputSamples")).and_then(Json::as_f64).unwrap_or_else(|| panic!("wanted latency.outputSamples in {}", document.write()))
}

#[test]
fn a_track_ten_ms_late_lands_480_frames_after_a_dry_one_and_leaves_the_output_latency_alone() {
    let waves = [click()];
    let project = project(vec![clicking(Vec::new(), 0.0), clicking(Vec::new(), TEN_MS)]);
    plans(&project, &[(0, 0, 0), (TEN_MS_FRAMES as isize, 0, TEN_MS_FRAMES)], 0, "a dry track beside one 10 ms late");
    let both = [(AT, LEVEL), (AT + TEN_MS_FRAMES, LEVEL)];
    hears(&project, &waves, 0, &both, &both, EXACT, "a dry track beside one 10 ms late");
}

#[test]
fn a_track_ten_ms_early_leads_by_480_raises_the_output_latency_and_exports_without_what_it_moved_before_the_start() {
    let waves = [clicks_at(CLICK, &[EDGE, AT])];
    let project = project(vec![clicking(Vec::new(), 0.0), clicking(Vec::new(), -TEN_MS)]);
    plans(&project, &[(0, 0, TEN_MS_FRAMES), (-(TEN_MS_FRAMES as isize), 0, 0)], TEN_MS_FRAMES, "a dry track beside one 10 ms early");
    let live_at = [(EDGE, LEVEL), (AT, LEVEL), (EDGE + TEN_MS_FRAMES, LEVEL), (AT + TEN_MS_FRAMES, LEVEL)];
    let export_at = [(EDGE, LEVEL), (AT - TEN_MS_FRAMES, LEVEL), (AT, LEVEL)];
    hears(&project, &waves, TEN_MS_FRAMES, &live_at, &export_at, EXACT, &format!("a dry track beside one 10 ms early, whose click at song frame {EDGE} would land {} frames before the span", TEN_MS_FRAMES - EDGE));
    let start = beat_frame(1);
    let later = [clicks_at(CLICK, &[start + EDGE, start + AT])];
    let file = export(&project, &later, 1);
    assert_eq!(file.len(), song(), "a span from beat 1, trimmed export: wanted the span's {} frames, got {}", song(), file.len());
    lands(&file, &export_at, EXACT, &format!("a span from beat 1, trimmed export: the early track's click at song frame {} would land {} frames before the span", start + EDGE, TEN_MS_FRAMES - EDGE));
}

#[test]
fn a_track_behind_a_431_sample_row_absorbs_its_delay_in_its_own_latency_first() {
    let waves = [click()];
    let beside = |ms: f32| project(vec![clicking(Vec::new(), 0.0), clicking(vec![linear()], ms)]);
    let later = beside(FIVE_MS);
    let output = LIN - FIVE_MS_FRAMES;
    plans(&later, &[(0, 0, output), (FIVE_MS_FRAMES as isize, LIN, 0)], output, "a dry track beside a 431-sample one 5 ms late");
    hears(&later, &waves, output, &[(AT + output, LEVEL), (AT + LIN, LEVEL)], &[(AT, LEVEL), (AT + FIVE_MS_FRAMES, LEVEL)], CLOSE, "a dry track beside a 431-sample one 5 ms late");
    let earlier = beside(-FIVE_MS);
    let output = LIN + FIVE_MS_FRAMES;
    plans(&earlier, &[(0, 0, output), (-(FIVE_MS_FRAMES as isize), LIN, 0)], output, "a dry track beside a 431-sample one 5 ms early");
    hears(&earlier, &waves, output, &[(AT + LIN, LEVEL), (AT + output, LEVEL)], &[(AT - FIVE_MS_FRAMES, LEVEL), (AT, LEVEL)], CLOSE, "a dry track beside a 431-sample one 5 ms early");
    let past = beside(TEN_MS);
    plans(&past, &[(0, 0, 0), (TEN_MS_FRAMES as isize, LIN, TEN_MS_FRAMES - LIN)], 0, "a dry track beside a 431-sample one 10 ms late, past its own latency");
    let both = [(AT, LEVEL), (AT + TEN_MS_FRAMES, LEVEL)];
    hears(&past, &waves, 0, &both, &both, CLOSE, "a dry track beside a 431-sample one 10 ms late, past its own latency");
}

#[test]
fn a_delayed_tracks_send_lands_with_its_direct_path() {
    let waves = [click()];
    let sending = |ms: f32| {
        let mut sender = clicking(Vec::new(), ms);
        sender.sends[RETURN] = SENT;
        let mut project = project(vec![sender, clicking(Vec::new(), 0.0)]);
        project.sends[RETURN].rack = vec![limiter(LIMITER_MS)];
        project
    };
    let later = sending(TEN_MS);
    let timing = plans(&later, &[(TEN_MS_FRAMES as isize, 0, LIMITER + TEN_MS_FRAMES), (0, 0, LIMITER)], LIMITER, "a track 10 ms late sent to a limited return");
    let feed = (timing.tracks[0].to_returns, timing.returns[RETURN].latency, timing.return_to_master[RETURN]);
    assert_eq!(feed, (TEN_MS_FRAMES, LIMITER, 0), "a track 10 ms late sent to a limited return, the plan: wanted it fed {TEN_MS_FRAMES} late into the return, the return {LIMITER} late and met at the master, got to_returns, the return's latency and return_to_master {feed:?}");
    hears(&later, &waves, LIMITER, &[(AT + LIMITER, LEVEL), (AT + LIMITER + TEN_MS_FRAMES, 2.0 * LEVEL)], &[(AT, LEVEL), (AT + TEN_MS_FRAMES, 2.0 * LEVEL)], EXACT, "a track 10 ms late sent at unity to a limited return, its direct and returned clicks as one beside a dry track");
    let earlier = sending(-TEN_MS);
    let output = TEN_MS_FRAMES + LIMITER;
    let timing = plans(&earlier, &[(-(TEN_MS_FRAMES as isize), 0, LIMITER), (0, 0, output)], output, "a track 10 ms early sent to a limited return");
    let feed = (timing.tracks[0].to_returns, timing.returns[RETURN].latency, timing.return_to_master[RETURN]);
    assert_eq!(feed, (0, output, 0), "a track 10 ms early sent to a limited return, the plan: wanted it fed at once into the return, the return {output} late and met at the master, got to_returns, the return's latency and return_to_master {feed:?}");
    hears(&earlier, &waves, output, &[(AT + LIMITER, 2.0 * LEVEL), (AT + output, LEVEL)], &[(AT - TEN_MS_FRAMES, 2.0 * LEVEL), (AT, LEVEL)], EXACT, "a track 10 ms early sent at unity to a limited return, its direct and returned clicks as one beside a dry track");
}

#[test]
fn a_delayed_member_moves_inside_its_group() {
    let waves = [click()];
    let (first, second, outside) = (LEVEL * FIRST_GAIN, LEVEL * SECOND_GAIN, LEVEL * OUTSIDE_GAIN);
    let later = grouped(0.0, TEN_MS);
    let timing = Timing::of(&later, SAMPLE_RATE);
    let feeds = (timing.tracks[FIRST].delay, timing.tracks[FIRST].to_group, timing.tracks[SECOND].to_group, timing.tracks[GROUP].source, timing.output);
    assert_eq!(feeds, (TEN_MS_FRAMES as isize, TEN_MS_FRAMES, 0, 0, 0), "a member 10 ms late, the plan: wanted it fed {TEN_MS_FRAMES} late into its group beside a member fed at once, the group's source and the output at 0, got its delay, both members' to_group, the group's source and the output {feeds:?}");
    let both = [(AT, second + outside), (AT + TEN_MS_FRAMES, first)];
    hears(&later, &waves, 0, &both, &both, EXACT, "a member 10 ms late inside a group, beside a member and a track outside");
    let earlier = grouped(0.0, -TEN_MS);
    let timing = Timing::of(&earlier, SAMPLE_RATE);
    let feeds = (timing.tracks[FIRST].to_group, timing.tracks[SECOND].to_group, timing.tracks[GROUP].source, timing.tracks[GROUP].to_master, timing.tracks[OUTSIDE].to_master);
    assert_eq!(feeds, (0, TEN_MS_FRAMES, TEN_MS_FRAMES, 0, TEN_MS_FRAMES), "a member 10 ms early, the plan: wanted it fed at once and the other member {TEN_MS_FRAMES} late into a group sourced {TEN_MS_FRAMES} late, the group met at the master and the track outside held {TEN_MS_FRAMES}, got both members' to_group, the group's source, its to_master and the outside track's to_master {feeds:?}");
    hears(&earlier, &waves, TEN_MS_FRAMES, &[(AT, first), (AT + TEN_MS_FRAMES, second + outside)], &[(AT - TEN_MS_FRAMES, first), (AT, second + outside)], EXACT, "a member 10 ms early inside a group, beside a member and a track outside");
}

#[test]
fn a_delay_on_a_group_moves_every_member_and_adds_to_a_members_own() {
    let waves = [click()];
    let (first, second, outside) = (LEVEL * FIRST_GAIN, LEVEL * SECOND_GAIN, LEVEL * OUTSIDE_GAIN);
    let later = grouped(TEN_MS, 0.0);
    plans(&later, &[(TEN_MS_FRAMES as isize, 0, TEN_MS_FRAMES), (0, 0, 0), (0, 0, 0), (0, 0, 0)], 0, "a group 10 ms late beside a track outside");
    let both = [(AT, outside), (AT + TEN_MS_FRAMES, first + second)];
    hears(&later, &waves, 0, &both, &both, EXACT, "a group 10 ms late with two members, beside a track outside");
    let earlier = grouped(-TEN_MS, 0.0);
    plans(&earlier, &[(-(TEN_MS_FRAMES as isize), 0, 0), (0, 0, 0), (0, 0, 0), (0, 0, TEN_MS_FRAMES)], TEN_MS_FRAMES, "a group 10 ms early beside a track outside");
    hears(&earlier, &waves, TEN_MS_FRAMES, &[(AT, first + second), (AT + TEN_MS_FRAMES, outside)], &[(AT - TEN_MS_FRAMES, first + second), (AT, outside)], EXACT, "a group 10 ms early with two members, beside a track outside");
    let cancelled = grouped(-TEN_MS, TEN_MS);
    let timing = plans(&cancelled, &[(-(TEN_MS_FRAMES as isize), 0, 0), (TEN_MS_FRAMES as isize, 0, 0), (0, 0, 0), (0, 0, TEN_MS_FRAMES)], TEN_MS_FRAMES, "a member 10 ms late inside a group 10 ms early");
    let feeds = (timing.tracks[FIRST].to_group, timing.tracks[SECOND].to_group, timing.tracks[GROUP].source);
    assert_eq!(feeds, (TEN_MS_FRAMES, 0, 0), "a member 10 ms late inside a group 10 ms early, the plan: wanted it fed {TEN_MS_FRAMES} late into a group sourced at 0, got both members' to_group and the group's source {feeds:?}");
    hears(&cancelled, &waves, TEN_MS_FRAMES, &[(AT, second), (AT + TEN_MS_FRAMES, first + outside)], &[(AT - TEN_MS_FRAMES, second), (AT, first + outside)], EXACT, "a member 10 ms late inside a group 10 ms early, landing with the track outside");
}

#[test]
fn a_sidechain_keyed_from_a_delayed_track_ducks_where_that_track_is_heard() {
    let waves = [click(), tone()];
    for pre in [false, true] {
        let later = keyed(TEN_MS, pre);
        let timing = Timing::of(&later, SAMPLE_RATE);
        let row = &timing.tracks[DUCKED].rack.rows[0];
        let lines = (row.signal, row.before, timing.output);
        assert_eq!(lines, (TEN_MS_FRAMES, 0, 0), "a {} key from a track 10 ms late, the plan: wanted the key read {TEN_MS_FRAMES} back, nothing before the row and the output at 0, got key, before and output {lines:?}", tap(pre));
        ducks(&later, &waves, AT + TEN_MS_FRAMES, AT + TEN_MS_FRAMES, &format!("a {} key from a track 10 ms late", tap(pre)));
        let earlier = keyed(-TEN_MS, pre);
        let timing = Timing::of(&earlier, SAMPLE_RATE);
        let row = &timing.tracks[DUCKED].rack.rows[0];
        let lines = (row.signal, row.before, timing.tracks[DUCKED].latency, timing.output);
        assert_eq!(lines, (0, TEN_MS_FRAMES, TEN_MS_FRAMES, TEN_MS_FRAMES), "a {} key from a track 10 ms early, the plan: wanted the key read at once and the ducked audio held {TEN_MS_FRAMES} before the row, its track and the output {TEN_MS_FRAMES} late, got key, before, latency and output {lines:?}", tap(pre));
        ducks(&earlier, &waves, AT, AT - TEN_MS_FRAMES, &format!("a {} key from a track 10 ms early", tap(pre)));
    }
    let mut master = project(vec![clicking(Vec::new(), TEN_MS), playing(TONE_NAME, Vec::new(), 0.0)]);
    master.master.rack = vec![ducker(SOURCE, false)];
    let key = Timing::of(&master, SAMPLE_RATE).master.rows[0].signal;
    assert_eq!(key, TEN_MS_FRAMES, "a Sidechain on the master keyed from a track 10 ms late, the plan: wanted the key read {TEN_MS_FRAMES} back, got {key}");
    ducks(&master, &waves, AT + TEN_MS_FRAMES, AT + TEN_MS_FRAMES, "a Sidechain on the master keyed from a track 10 ms late");
}

#[test]
fn a_members_send_lands_with_its_path_through_its_delayed_group() {
    let waves = [click()];
    let (first, second, outside) = (LEVEL * FIRST_GAIN, LEVEL * SECOND_GAIN, LEVEL * OUTSIDE_GAIN);
    let returned = first * SENT;
    let landed = first + returned + second;
    let later = member_sending(TEN_MS, 0.0);
    fed_returns(&later, (TEN_MS_FRAMES, 0, LIMITER), "a member sending from a group 10 ms late");
    hears(&later, &waves, LIMITER, &[(AT + LIMITER, outside), (AT + LIMITER + TEN_MS_FRAMES, landed)], &[(AT, outside), (AT + TEN_MS_FRAMES, landed)], EXACT, "a member sent at unity to a limited return from a group 10 ms late, its returned click landing with its path through the group");
    let mut latent = member_sending(TEN_MS, 0.0);
    latent.tracks[FIRST].rack = vec![linear()];
    fed_returns(&latent, (TEN_MS_FRAMES - LIN, 0, LIMITER), "a member behind a 431-sample row sending from a group 10 ms late");
    hears(&latent, &waves, LIMITER, &[(AT + LIMITER, outside), (AT + LIMITER + TEN_MS_FRAMES, landed)], &[(AT, outside), (AT + TEN_MS_FRAMES, landed)], CLOSE, "a member behind a 431-sample row sent at unity to a limited return from a group 10 ms late, its returned click landing with its path through the group and nothing held back for the member's own latency");
    let earlier = member_sending(-TEN_MS, 0.0);
    let output = TEN_MS_FRAMES + LIMITER;
    fed_returns(&earlier, (0, TEN_MS_FRAMES, output), "a member sending from a group 10 ms early");
    hears(&earlier, &waves, output, &[(AT + LIMITER, landed), (AT + output, outside)], &[(AT - TEN_MS_FRAMES, landed), (AT, outside)], EXACT, "a member sent at unity to a limited return from a group 10 ms early, its returned click landing with its path through the group");
    let cancelled = member_sending(-TEN_MS, TEN_MS);
    fed_returns(&cancelled, (TEN_MS_FRAMES, TEN_MS_FRAMES, output), "a member 10 ms late sending from a group 10 ms early");
    let met = first + returned + outside;
    hears(&cancelled, &waves, output, &[(AT + LIMITER, second), (AT + output, met)], &[(AT - TEN_MS_FRAMES, second), (AT, met)], EXACT, "a member 10 ms late sent at unity to a limited return from a group 10 ms early, its returned click landing with its path through the group and the track outside");
}

#[test]
fn a_sidechain_keyed_from_a_member_ducks_where_its_delayed_group_plays_it() {
    let waves = [click(), tone()];
    let cases = [
        (TEN_MS, 0.0, [(TEN_MS_FRAMES, 0, 0), (TEN_MS_FRAMES, 0, 0), (TEN_MS_FRAMES, 0, 0)], AT + TEN_MS_FRAMES, AT + TEN_MS_FRAMES, "a member of a group 10 ms late"),
        (-TEN_MS, 0.0, [(0, TEN_MS_FRAMES, TEN_MS_FRAMES), (0, 0, TEN_MS_FRAMES), (0, TEN_MS_FRAMES, TEN_MS_FRAMES)], AT, AT - TEN_MS_FRAMES, "a member of a group 10 ms early"),
        (-TEN_MS, TEN_MS, [(0, 0, TEN_MS_FRAMES), (TEN_MS_FRAMES, 0, TEN_MS_FRAMES), (0, 0, TEN_MS_FRAMES)], AT + TEN_MS_FRAMES, AT, "a member 10 ms late inside a group 10 ms early"),
    ];
    for (group_ms, first_ms, [outside, master, other], live_frame, export_frame, what) in cases {
        for pre in [false, true] {
            let project = keyed_outside(group_ms, first_ms, pre);
            let lines = track_key(&project, OUTSIDE);
            assert_eq!(lines, outside, "a {} key from {what} on a track outside the group, the plan: wanted key, before and output at {outside:?}, got {lines:?}", tap(pre));
            ducks(&project, &waves, live_frame, export_frame, &format!("a {} key from {what} on a track outside the group", tap(pre)));
        }
        let project = keyed_on_master(group_ms, first_ms);
        let lines = master_key(&project);
        assert_eq!(lines, master, "a key from {what} on the master, the plan: wanted key, before and output at {master:?}, got {lines:?}");
        ducks(&project, &waves, live_frame, export_frame, &format!("a key from {what} on the master"));
        let project = keyed_in_another_group(group_ms, first_ms);
        let lines = track_key(&project, OTHER_MEMBER);
        assert_eq!(lines, other, "a key from {what} on a member of another group, the plan: wanted key, before and output at {other:?}, got {lines:?}");
        ducks(&project, &waves, live_frame, export_frame, &format!("a key from {what} on a member of another group"));
    }
}

#[test]
fn a_sidechain_inside_a_delayed_group_keyed_from_its_member_ducks_with_it_as_the_group_moves() {
    let waves = [click(), tone()];
    let cases = [
        (TEN_MS, 0.0, (0, 0, 0), AT + TEN_MS_FRAMES, AT + TEN_MS_FRAMES, "a member of a group 10 ms late"),
        (-TEN_MS, 0.0, (0, 0, TEN_MS_FRAMES), AT, AT - TEN_MS_FRAMES, "a member of a group 10 ms early"),
        (-TEN_MS, TEN_MS, (TEN_MS_FRAMES, 0, TEN_MS_FRAMES), AT + TEN_MS_FRAMES, AT, "a member 10 ms late inside a group 10 ms early"),
    ];
    for (group_ms, first_ms, wanted, live_frame, export_frame, what) in cases {
        for pre in [false, true] {
            let beside = keyed_beside(group_ms, first_ms, pre);
            let lines = track_key(&beside, SECOND);
            assert_eq!(lines, wanted, "a {} key from {what} on another member, the plan: wanted key, before and output at {wanted:?}, got {lines:?}", tap(pre));
            ducks(&beside, &waves, live_frame, export_frame, &format!("a {} key from {what} on another member", tap(pre)));
            let own = keyed_on_group(group_ms, first_ms, pre);
            let lines = track_key(&own, GROUP);
            assert_eq!(lines, wanted, "a {} key from {what} on the group itself, the plan: wanted key, before and output at {wanted:?}, got {lines:?}", tap(pre));
            ducks(&own, &waves, live_frame, export_frame, &format!("a {} key from {what} on the group itself", tap(pre)));
        }
    }
}

#[test]
fn delay_ms_round_trips_through_the_project_json_clamped_to_100_either_way_and_is_0_when_absent() {
    let original = project(vec![clicking(Vec::new(), 12.5), clicking(Vec::new(), -37.25), clicking(Vec::new(), 0.0)]);
    let saved = original.to_json();
    let written: Vec<Option<f64>> = saved.get("tracks").and_then(Json::as_array).unwrap_or_else(|| panic!("wanted tracks in {}", saved.write())).iter().map(|track| track.get("delayMs").and_then(Json::as_f64)).collect();
    assert_eq!(written, vec![Some(12.5), Some(-37.25), None], "wanted delayMs written as 12.5 and -37.25 and left out at 0, got {written:?}");
    let reopened = reopen(&saved.write());
    let delays: Vec<f32> = reopened.tracks.iter().map(|track| track.delay_ms).collect();
    assert_eq!(delays, vec![12.5, -37.25, 0.0], "wanted the delays back after a save and an open, got {delays:?}");
    let samples: Vec<isize> = Timing::of(&reopened, SAMPLE_RATE).tracks.iter().map(|track| track.delay).collect();
    assert_eq!(samples, vec![600, -1_788, 0], "wanted the reopened delays planned as 600, -1788 and 0 samples at {SAMPLE_RATE}, got {samples:?}");
    let beyond = reopen(r#"{"kind":"project","tracks":[{"kind":"audio","delayMs":250},{"kind":"audio","delayMs":-250},{"kind":"audio"},{"kind":"audio","delayMs":"soon"}]}"#);
    let delays: Vec<f32> = beyond.tracks.iter().map(|track| track.delay_ms).collect();
    assert_eq!(delays, vec![100.0, -100.0, 0.0, 0.0], "wanted 250 and -250 clamped to 100 and -100, and a track without a number at 0, got {delays:?}");
    let samples: Vec<isize> = Timing::of(&beyond, SAMPLE_RATE).tracks.iter().map(|track| track.delay).collect();
    assert_eq!(samples, vec![4_800, -4_800, 0, 0], "wanted the clamped delays planned as 4800, -4800, 0 and 0 samples, got {samples:?}");
    let again = beyond.to_json();
    let written: Vec<Option<f64>> = again.get("tracks").and_then(Json::as_array).unwrap_or_else(|| panic!("wanted tracks in {}", again.write())).iter().map(|track| track.get("delayMs").and_then(Json::as_f64)).collect();
    assert_eq!(written, vec![Some(100.0), Some(-100.0), None, None], "wanted the clamped delays saved as 100 and -100 and the others left out, got {written:?}");
}

#[test]
fn delay_ms_reads_and_writes_through_the_mcp_clamped_with_the_plan_following() {
    let (mut app, _renderer) = resumed(&Project::new());
    let read = |app: &mut App| served(app, "read", &format!(r#"{{"path":"{TRACK_PATH}"}}"#));
    let absent = read(&mut app);
    assert!(absent.contains("\"stored\": false") && absent.contains("\"default\": 0") && absent.contains("number -100 to 100, default 0"), "wanted a track without delayMs read as not stored, default 0, between -100 and 100, got {absent}");
    assert_eq!(output_samples(&mut app), 0.0, "wanted the output latency at 0 before any delay");
    let reply = served(&mut app, "write", &format!(r#"{{"path":"{TRACK_PATH}","value":-10}}"#));
    assert!(reply.starts_with(&format!("Written at {TRACK_PATH}")) && !reply.contains("Kept differently"), "wanted -10 written as sent, got {reply}");
    let back = read(&mut app);
    assert_eq!(number(&back), Some(-TEN_MS_F64), "wanted {TRACK_PATH} read back as -10, got {back}");
    assert_eq!(output_samples(&mut app), TEN_MS_FRAMES as f64, "wanted a track 10 ms early to raise the output latency to {TEN_MS_FRAMES}");
    for (sent, kept, output) in [(250, 100, 0), (-250, -100, MOST_FRAMES)] {
        let reply = served(&mut app, "write", &format!(r#"{{"path":"{TRACK_PATH}","value":{sent}}}"#));
        let named = format!("{TRACK_PATH}: sent {sent}, kept {kept}");
        assert!(reply.contains(&named), "wanted the reply to say {named:?}, got {reply}");
        let back = read(&mut app);
        assert_eq!(number(&back), Some(f64::from(kept)), "wanted {TRACK_PATH} read back as {kept} after {sent} was sent, got {back}");
        assert_eq!(output_samples(&mut app), output as f64, "wanted the output latency at {output} with the track at {kept} ms");
    }
    let whole = document(&mut app);
    served(&mut app, "write", &format!(r#"{{"value":{}}}"#, whole.write()));
    let again = document(&mut app);
    assert_eq!(again, whole, "wanted the whole document written back to keep delayMs -100");
    let opened = Project::from_json(&again).unwrap_or_else(|| panic!("wanted the read document to open as a project, got {}", again.write()));
    assert_eq!(opened.tracks[1].delay_ms, -MOST_MS, "wanted the read document to open with the track at -{MOST_MS} ms, got {}", opened.tracks[1].delay_ms);
    served(&mut app, "write", &format!(r#"{{"path":"{TRACK_PATH}","value":0}}"#));
    let cleared = read(&mut app);
    assert!(cleared.contains("\"stored\": false"), "wanted delayMs 0 left out of the document, got {cleared}");
    assert_eq!(output_samples(&mut app), 0.0, "wanted the output latency back at 0 with the delay at 0");
}

#[test]
fn every_delay_at_zero_plans_and_renders_exactly_as_before() {
    let waves = [clicks_at(QUIET_CLICK, &[AT])];
    let still = everything([0.0; 6]);
    let timing = Timing::of(&still, SAMPLE_RATE);
    let (returns_in, master_in) = (LIMITER + LIN, LIMITER + LIN + LIMITER);
    let figures: Vec<(isize, usize, usize, usize, usize, usize)> = timing.tracks.iter().map(|track| (track.delay, track.latency, track.to_master, track.to_returns, track.to_group, track.uncompensated)).collect();
    let wanted = vec![(0, 0, master_in, returns_in, 0, 0), (0, returns_in, LIMITER, 0, 0, 0), (0, returns_in, LIMITER, 0, 0, 0), (0, LIMITER, 0, LIN, 0, 0), (0, 0, 0, returns_in, LIMITER, 0), (0, returns_in, LIMITER, 0, 0, 0)];
    assert_eq!(figures, wanted, "every delay at 0, the plan: wanted each track's delay, latency, to_master, to_returns, to_group and uncompensated at {wanted:?}, got {figures:?}");
    let project_wide = (timing.tracks[GROUPED].source, timing.returns_in, timing.returns[RETURN].latency, timing.master_in, timing.output, timing.tracks[KEYED].rack.rows[0].before);
    let wanted = (LIMITER, returns_in, master_in, master_in, master_in, returns_in);
    assert_eq!(project_wide, wanted, "every delay at 0, the plan: wanted the group's source, returns_in, the return's latency, master_in, the output and the keyed row's line at {wanted:?}, got {project_wide:?}");
    for (index, track) in timing.tracks.iter().enumerate() {
        let grouped = still.group_of(index).is_some();
        let rule = (if grouped { 0 } else { timing.master_in - track.latency }, timing.returns_in - track.latency, still.group_of(index).map_or(0, |group| timing.tracks[group].source - track.latency));
        let got = (track.to_master, track.to_returns, track.to_group);
        assert_eq!(got, rule, "every delay at 0, the plan: wanted track {index} held to the master, the returns and its group by the latest latency less its own, {rule:?}, got {got:?}");
    }
    let unheard = [UNHEARD_MS, -UNHEARD_MS];
    let rounded = unheard.map(|ms| everything([ms; 6]));
    let ([(heard_live, live_timing), rounded_live @ ..], [file, rounded_files @ ..]) = played_and_bounced([&still, &rounded[0], &rounded[1]], &waves);
    assert_eq!(live_timing, timing, "every delay at 0, live output: wanted the engine's plan to be the project's");
    let all = 5.0 * QUIET;
    lands(&heard_live, &[(AT + master_in, all)], CLOSE, "every delay at 0, live output: five paths through a group, a return, a key and latent rows");
    let file = exported(file);
    lands(&file, &[(AT, all)], CLOSE, "every delay at 0, trimmed export: five paths through a group, a return, a key and latent rows");
    for (((ms, rounded), (out, _)), bounce) in unheard.into_iter().zip(&rounded).zip(rounded_live).zip(rounded_files) {
        let planned = Timing::of(rounded, SAMPLE_RATE);
        assert_eq!(planned, timing, "every track at {ms} ms, under half a sample, the plan: wanted it identical to every delay at 0");
        let divergence = out.iter().zip(&heard_live).position(|(a, b)| a.to_bits() != b.to_bits());
        assert_eq!(divergence, None, "every track at {ms} ms, live output: wanted it bit for bit the render at 0, got a difference at frame {divergence:?}");
        let exported = exported(bounce);
        let divergence = exported.iter().zip(&file).position(|(a, b)| a.to_bits() != b.to_bits());
        assert!(exported.len() == file.len() && divergence.is_none(), "every track at {ms} ms, trimmed export: wanted it bit for bit the export at 0, got {} frames against {} differing at frame {divergence:?}", exported.len(), file.len());
    }
}
