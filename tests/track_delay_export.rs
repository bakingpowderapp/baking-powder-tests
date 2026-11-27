use bp::arp::{Arp, Articulation};
use bp::audio::Tail;
use bp::core::Params as CoreParams;
use bp::core::params::Envelope;
use bp::fx::{Kind, Params, Slot};
use bp::project::{Aim, Clip, Curve, LaneLfo, Node, Note, Project, Row, TICKS_PER_BEAT, Track, VELOCITY};
use bp::wav::Wave;
use std::sync::Arc;
use support::{AUDIBLE, SAMPLE_RATE, beat_frame, core_layer, midi};

const PLAIN: &str = "plain";
const LATE: &str = "late";
const EARLY: &str = "early";
const CLICK: i16 = 8_192;
const LEVEL: f32 = 0.25;
const EXACT: f32 = 1e-6;
const NEAR: f32 = 1e-4;
const WAVE_SECONDS: usize = 10;
const CLIP_BEATS: i32 = 16;
const FROM_BEAT: i32 = 1;
const SPAN_BEATS: i32 = 4;
const TEN_MS: f32 = 10.0;
const TEN_MS_FRAMES: usize = 480;
const EDGE: usize = 200;
const INSIDE: usize = 1_000;
const PAST_WINDOW: usize = 100;
const LONG_PAST: usize = 2_000;
const UNDER_THE_FADE: usize = 20;
const HEADER: usize = 68;
const BYTES_PER_FRAME: usize = 6;
const SCALE: f32 = 8_388_608.0;
const SHOWN: usize = 12;
const PITCH: u8 = 60;
const HELD_FROM: f32 = 0.5;
const HELD_UNTIL: f32 = 3.0;
const CORE_LEVEL: f64 = 0.25;
const ATTACK_MS: f64 = 10.0;
const COMPARED: usize = 24_000;
const RISE: f32 = 8.0;
const LIMITER_MS: f64 = 1.5;
const RETURN: usize = 0;
const SENT: f32 = 0.5;
const FULL_SEND: f32 = 1.0;
const FADER_FROM_BEAT: i32 = 2;
const FADER_UNTIL_BEAT: i32 = 6;
const UNITY: f32 = 0.75;
const LOWERED: f32 = 0.4;
const SWAY: f64 = 0.4;
const SWAY_LAG_MS: f64 = 5.0;
const GROUP: usize = 0;
const MEMBER: usize = 1;
const OUTSIDE: usize = 2;
const EXPORT_BLOCK: usize = 4_096;
const FADE: usize = 64;

fn clicks(name: &str, at: &[usize]) -> Arc<Wave> {
    let mut pcm = vec![0i16; WAVE_SECONDS * SAMPLE_RATE as usize];
    for &frame in at {
        pcm[frame] = CLICK;
    }
    Arc::new(Wave::sound(String::from(name), SAMPLE_RATE, 1, pcm))
}

fn clicking(name: &str, delay_ms: f32) -> Track {
    let mut track = Track { delay_ms, ..Track::audio() };
    track.clips.push(Clip::audio(0, CLIP_BEATS * TICKS_PER_BEAT, String::from(name), String::from(name)));
    track
}

fn project(tracks: Vec<Track>) -> Project {
    let mut project = Project::new();
    project.tracks.clear();
    for (at, track) in tracks.into_iter().enumerate() {
        project.add_track_outside(at, track);
    }
    project
}

fn edges() -> (usize, usize) {
    (beat_frame(FROM_BEAT as usize), beat_frame((FROM_BEAT + SPAN_BEATS) as usize))
}

fn song() -> usize {
    beat_frame(SPAN_BEATS as usize)
}

fn span() -> (i32, i32) {
    (FROM_BEAT * TICKS_PER_BEAT, (FROM_BEAT + SPAN_BEATS) * TICKS_PER_BEAT)
}

fn left(wav: &[u8]) -> Vec<f32> {
    wav[HEADER..].as_chunks::<BYTES_PER_FRAME>().0.iter().map(|frame| (i32::from_le_bytes([0, frame[0], frame[1], frame[2]]) >> 8) as f32 / SCALE).collect()
}

fn exported(project: &Project, waves: &[Arc<Wave>], tail: Tail) -> Vec<u8> {
    support::bounce(project, waves, span(), tail).expect("wanted the project to export")
}

fn export(project: &Project, waves: &[Arc<Wave>], tail: Tail) -> Vec<f32> {
    left(&exported(project, waves, tail))
}

fn heard(out: &[f32], floor: f32) -> Vec<(usize, f32)> {
    out.iter().enumerate().filter(|(_, sample)| sample.abs() > floor).map(|(frame, &sample)| (frame, sample)).collect()
}

fn misses(out: &[f32], wanted: &[(usize, f32)]) -> Option<String> {
    let mut wanted = wanted.to_vec();
    wanted.sort_by_key(|&(frame, _)| frame);
    let got = heard(out, EXACT);
    let alike = got.len() == wanted.len() && got.iter().zip(&wanted).all(|(&(frame, level), &(at, height))| frame == at && (level - height).abs() <= EXACT);
    (!alike).then(|| format!("wanted clicks at {wanted:?} as (frame, level) and nothing else above {EXACT}, got {} frames above it, the first {:?}", got.len(), &got[..got.len().min(SHOWN)]))
}

fn lands(out: &[f32], wanted: &[(usize, f32)], what: &str) {
    if let Some(missed) = misses(out, wanted) {
        panic!("{what}: {missed}");
    }
}

fn tail_name(tail: Tail) -> &'static str {
    match tail {
        Tail::Cut => "cut",
        Tail::Rings => "rings",
    }
}

fn plucked(delay_ms: f32, from: f32) -> Track {
    let mut patch = CoreParams::DEFAULT;
    patch.amp = Envelope { attack_ms: ATTACK_MS, decay_ms: 1.0, sustain: 1.0, release_ms: 1.0, ..Envelope::DEFAULT };
    let notes = vec![Note { pitch: PITCH, start: from, length: HELD_UNTIL - from, velocity: VELOCITY, articulation: Articulation::PLAIN }];
    Track { delay_ms, rack: vec![midi(Arp::DEFAULT), Row::Core(core_layer(CORE_LEVEL, patch))], clips: vec![Clip { start: 0, length: CLIP_BEATS * TICKS_PER_BEAT, label: String::new(), notes, file: None }], ..Track::blank() }
}

fn limiter() -> Row {
    let mut slot = Slot::new(Kind::Utility);
    if let Params::Utility(p) = &mut slot.params {
        p.limit = true;
        p.hard = false;
        p.ceiling_db = 0.0;
        p.lookahead_ms = LIMITER_MS;
    }
    Row::Effect(slot)
}

fn chords(delay_ms: f32) -> Track {
    let mut track = plucked(delay_ms, HELD_FROM);
    let starts = [HELD_FROM, 2.0, 4.5, 5.25];
    track.clips[0].notes = starts.iter().enumerate().map(|(at, &start)| Note { pitch: PITCH + 4 * at as u8, start, length: 0.75 + at as f32 * 0.25, velocity: VELOCITY, articulation: Articulation::PLAIN }).collect();
    track
}

fn busy(delay_ms: f32) -> Vec<Track> {
    let mut sender = clicking(PLAIN, delay_ms);
    sender.rack = vec![limiter()];
    sender.sends[RETURN] = SENT;
    let nodes = vec![Node { tick: FADER_FROM_BEAT * TICKS_PER_BEAT, travel: UNITY, bend: 0.0 }, Node { tick: FADER_UNTIL_BEAT * TICKS_PER_BEAT, travel: LOWERED, bend: 0.0 }];
    sender.automation.curves.push(Curve { aim: Aim::Gain, lfo: None, nodes });
    let mut swaying = chords(delay_ms);
    swaying.automation.lfos.push(LaneLfo { amount: SWAY, lag_ms: SWAY_LAG_MS, ..LaneLfo::blank(Aim::Gain) });
    vec![sender, swaying]
}

fn busy_project(extra: Vec<Track>) -> Project {
    let mut project = project(busy(0.0).into_iter().chain(extra).collect());
    project.sends[RETURN].rack = vec![limiter()];
    project
}

fn busy_waves() -> Vec<Arc<Wave>> {
    let (start, end) = edges();
    vec![clicks(PLAIN, &[start - EDGE, start + INSIDE, end - EDGE, end + EDGE, end + LONG_PAST])]
}

fn grouped(group_ms: f32, member_ms: f32) -> Project {
    let mut project = Project::new();
    project.tracks.clear();
    project.add_track(GROUP, Track { delay_ms: group_ms, ..Track::group() });
    project.add_track(MEMBER, clicking(LATE, member_ms));
    project.add_track_outside(OUTSIDE, clicking(PLAIN, 0.0));
    assert_eq!(project.members_of(GROUP), vec![MEMBER], "wanted track {MEMBER} inside the group and track {OUTSIDE} outside it");
    project
}

fn sending(delay_ms: f32) -> Project {
    let mut sender = clicking(LATE, delay_ms);
    sender.sends[RETURN] = FULL_SEND;
    let mut project = project(vec![sender, clicking(PLAIN, 0.0)]);
    project.sends[RETURN].rack = vec![limiter()];
    project
}

fn first_byte_apart(a: &[u8], b: &[u8]) -> Option<usize> {
    a.iter().zip(b).position(|(x, y)| x != y).or((a.len() != b.len()).then_some(a.len().min(b.len())))
}

#[test]
fn a_track_ten_ms_late_carries_its_click_from_200_frames_before_a_mid_song_span_280_frames_into_the_file() {
    let (start, _) = edges();
    let waves = [clicks(PLAIN, &[start + INSIDE]), clicks(LATE, &[start - EDGE])];
    let project = project(vec![clicking(PLAIN, 0.0), clicking(LATE, TEN_MS)]);
    let at = TEN_MS_FRAMES - EDGE;
    let wrong: Vec<String> = [Tail::Cut, Tail::Rings]
        .into_iter()
        .filter_map(|tail| {
            let file = export(&project, &waves, tail);
            misses(&file, &[(INSIDE, LEVEL), (at, LEVEL)]).map(|missed| format!("{} export, {} frames long, {} at frame {at}: {missed}", tail_name(tail), file.len(), file.get(at).copied().unwrap_or(0.0)))
        })
        .collect();
    assert!(wrong.is_empty(), "a track 10 ms late beside a dry one, exported from beat {FROM_BEAT}: its click {EDGE} frames before the span's start, played {TEN_MS_FRAMES} late, belongs {at} frames into the file at {LEVEL}, got {wrong:?}");
}

#[test]
fn a_track_ten_ms_early_carries_its_click_from_200_frames_past_a_mid_song_span_280_frames_before_the_files_end_and_nothing_later() {
    let (start, end) = edges();
    let later = [end + TEN_MS_FRAMES + PAST_WINDOW, end + LONG_PAST];
    let waves = [clicks(PLAIN, &[start + INSIDE]), clicks(EARLY, &[end + EDGE, later[0], later[1]])];
    let project = project(vec![clicking(PLAIN, 0.0), clicking(EARLY, -TEN_MS)]);
    let at = song() - (TEN_MS_FRAMES - EDGE);
    let wrong: Vec<String> = [Tail::Cut, Tail::Rings]
        .into_iter()
        .filter_map(|tail| {
            let file = export(&project, &waves, tail);
            let long = (tail == Tail::Cut && file.len() != song()).then(|| format!("{} frames where the span holds {}", file.len(), song()));
            let missed = misses(&file, &[(INSIDE, LEVEL), (at, LEVEL)]);
            (long.is_some() || missed.is_some()).then(|| format!("{} export, {} frames long, {} at frame {at}: {}", tail_name(tail), file.len(), file.get(at).copied().unwrap_or(0.0), long.into_iter().chain(missed).collect::<Vec<String>>().join("; ")))
        })
        .collect();
    assert!(wrong.is_empty(), "a track 10 ms early beside a dry one, exported from beat {FROM_BEAT}: its click {EDGE} frames past the span's end, played {TEN_MS_FRAMES} early, belongs {} frames before the file's end at {LEVEL}, and its clicks {} and {} frames past the end, beyond its window, nowhere, got {wrong:?}", TEN_MS_FRAMES - EDGE, later[0] - end, later[1] - end);
}

#[test]
fn a_delayed_groups_member_and_a_delayed_tracks_send_carry_their_clicks_across_a_mid_song_spans_edges() {
    let (start, end) = edges();
    let waves: &[Arc<Wave>] = &[clicks(LATE, &[start - EDGE, end + EDGE]), clicks(PLAIN, &[start + INSIDE])];
    let (late, early) = (TEN_MS_FRAMES - EDGE, song() - (TEN_MS_FRAMES - EDGE));
    let cases = [
        (grouped(TEN_MS, 0.0), late, LEVEL, "a member of a group 10 ms late"),
        (grouped(-TEN_MS, 0.0), early, LEVEL, "a member of a group 10 ms early"),
        (grouped(TEN_MS, -2.0 * TEN_MS), early, LEVEL, "a member 20 ms early inside a group 10 ms late"),
        (sending(TEN_MS), late, 2.0 * LEVEL, "a track 10 ms late sent at unity to a limited return"),
        (sending(-TEN_MS), early, 2.0 * LEVEL, "a track 10 ms early sent at unity to a limited return"),
    ];
    let wrong: Vec<String> = cases
        .iter()
        .flat_map(|(project, at, level, what)| {
            [Tail::Cut, Tail::Rings].into_iter().filter_map(move |tail| {
                let file = export(project, waves, tail);
                misses(&file, &[(INSIDE, LEVEL), (*at, *level)]).map(|missed| format!("{what}, {} export: {} at frame {at}: {missed}", tail_name(tail), file.get(*at).copied().unwrap_or(0.0)))
            })
        })
        .collect();
    assert!(wrong.is_empty(), "clicks {EDGE} frames before the span's start and past its end, moved 10 ms by a group's delay or carried through a send, exported from beat {FROM_BEAT}: wanted the late one {late} frames into the file and the early one {} frames before its end, each with its direct path, beside a dry track's click at {INSIDE}, got {wrong:?}", TEN_MS_FRAMES - EDGE);
}

#[test]
fn a_click_on_a_track_at_delay_0_just_past_the_end_fades_out_within_the_stops_64_frames() {
    let (start, end) = edges();
    let waves = [clicks(PLAIN, &[end - EDGE, end + UNDER_THE_FADE, end + EDGE]), clicks(LATE, &[start + INSIDE]), clicks(EARLY, &[start + 2 * INSIDE])];
    let mixed = project(vec![clicking(PLAIN, 0.0), clicking(LATE, TEN_MS), clicking(EARLY, -TEN_MS)]);
    let alone = project(vec![clicking(PLAIN, 0.0)]);
    let last = (song() - EDGE, LEVEL);
    let neighbours = [(INSIDE + TEN_MS_FRAMES, LEVEL), (2 * INSIDE - TEN_MS_FRAMES, LEVEL)];
    lands(&export(&alone, &waves, Tail::Cut), &[last], &format!("a dry track alone, cut export: its click {EDGE} frames before the span's end and none from the two past it, past the file's end"));
    lands(&export(&mixed, &waves, Tail::Cut), &[neighbours[0], neighbours[1], last], &format!("a dry track beside tracks 10 ms late and early, cut export: the neighbours' clicks, its own {EDGE} frames before the span's end and none from the two past it"));
    let by_itself = export(&alone, &waves, Tail::Rings);
    let beside = export(&mixed, &waves, Tail::Rings);
    let faded = LEVEL * (FADE - UNDER_THE_FADE) as f32 / FADE as f32;
    let leaked = by_itself.get(song() + UNDER_THE_FADE).copied().unwrap_or(0.0);
    assert!((leaked - faded).abs() <= NEAR, "a dry track alone, rings export: wanted its click {UNDER_THE_FADE} frames past the span's end at {faded} under the stop's {FADE}-frame fade, got {leaked}");
    lands(&by_itself, &[last, (song() + UNDER_THE_FADE, leaked)], &format!("a dry track alone, rings export: its click before the end, its faded click {UNDER_THE_FADE} frames past it and none from the one {EDGE} frames past, beyond the fade"));
    let next_to = beside.get(song() + UNDER_THE_FADE).copied().unwrap_or(0.0);
    assert_eq!(next_to.to_bits(), leaked.to_bits(), "a dry track beside tracks 10 ms late and early, rings export: wanted its click {UNDER_THE_FADE} frames past the span's end at {leaked} bit for bit as alone, got {next_to}");
    lands(&beside, &[neighbours[0], neighbours[1], last, (song() + UNDER_THE_FADE, leaked)], &format!("a dry track beside tracks 10 ms late and early, rings export: the neighbours' clicks, its click before the end, its faded click past it and none from the one {EDGE} frames past"));
}

#[test]
fn a_note_held_under_a_delayed_tracks_window_start_begins_where_its_window_starts() {
    let reference = export(&project(vec![plucked(0.0, FROM_BEAT as f32)]), &[], Tail::Cut);
    let peak = support::peak_of(&reference[..COMPARED]);
    assert!(peak > AUDIBLE && reference[0].abs() < peak / RISE, "a note beginning on the span's start at delay 0: wanted it to rise from near silence at frame 0 to a peak over {AUDIBLE}, got {} there and a peak of {peak}", reference[0]);
    let unlike: Vec<String> = [TEN_MS, -TEN_MS]
        .into_iter()
        .filter_map(|delay_ms| {
            let file = export(&project(vec![plucked(delay_ms, HELD_FROM)]), &[], Tail::Cut);
            let (frame, delta) = reference[..COMPARED].iter().zip(&file[..COMPARED]).enumerate().map(|(frame, (a, b))| (frame, (a - b).abs())).fold((0, 0.0f32), |worst, next| if next.1 > worst.1 { next } else { worst });
            (delta > NEAR).then(|| format!("{delay_ms} ms off: a difference of {delta} at frame {frame}, {} against {}", file[frame], reference[frame]))
        })
        .collect();
    assert!(unlike.is_empty(), "a note held from beat {HELD_FROM} on a track 10 ms late or early, cut export from beat {FROM_BEAT}: wanted it to begin where the track's window begins, so its first {COMPARED} frames match a note beginning on the span's start at delay 0 within {NEAR}, got {unlike:?}");
}

#[test]
fn tracks_at_delay_0_export_bit_for_bit_beside_silent_tracks_10_ms_late_and_early() {
    let waves = busy_waves();
    let alone = busy_project(Vec::new());
    let (cut, rings) = (exported(&alone, &waves, Tail::Cut), export(&alone, &waves, Tail::Rings));
    let mut wrong = Vec::new();
    for delays in [&[TEN_MS][..], &[-TEN_MS], &[TEN_MS, -TEN_MS]] {
        let beside = busy_project(delays.iter().map(|&delay_ms| Track { delay_ms, ..Track::audio() }).collect());
        let with = exported(&beside, &waves, Tail::Cut);
        if let Some(apart) = first_byte_apart(&with, &cut) {
            wrong.push(format!("beside {delays:?} ms, cut export: {} bytes against {} first apart at byte {apart}", with.len(), cut.len()));
        }
        let with = export(&beside, &waves, Tail::Rings);
        let apart = with.iter().zip(&rings).position(|(a, b)| a.to_bits() != b.to_bits());
        if apart.is_some() || with.len().abs_diff(rings.len()) >= EXPORT_BLOCK {
            wrong.push(format!("beside {delays:?} ms, rings export: {} frames against {} first apart at frame {apart:?}", with.len(), rings.len()));
        }
    }
    assert!(wrong.is_empty(), "tracks at delay 0 with clips, notes, a fader curve, an LFO, a limiter and a send, exported from beat {FROM_BEAT}: wanted the cut file byte for byte the same beside silent tracks 10 ms late, early or both, and every frame of the rings file both hold bit for bit the same, the tail's end moving less than a block with the output latency, got {wrong:?}");
}
