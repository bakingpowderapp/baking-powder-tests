use bp::audio::Tail;
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::fx::{Kind, Params, Slot};
use bp::project::{Clip, Project, Row, TICKS_PER_BEAT, Timing, Track};
use bp::wav::Wave;
use std::sync::Arc;
use support::{BLOCK, SAMPLE_RATE, SILENT, beat_frame};

const NAME: &str = "click";
const CLICK: i16 = 8_192;
const LEVEL: f32 = 0.25;
const AT: usize = 10_000;
const CLIP_BEATS: i32 = 8;
const SPAN_BEATS: i32 = 4;
const WAVE_SECONDS: usize = 10;
const LOOKAHEAD_MS: f64 = 10.0;
const ONE: usize = 479;
const TWENTY: usize = 20;
const CAP: usize = 24_000;
const MEMBER: usize = TWENTY * ONE;
const GROUP: usize = 2 * MEMBER;
const MASTER_IN: usize = 3 * MEMBER;
const EXCESS: usize = MASTER_IN - CAP;
const DRY: usize = 0;
const GROUPED: usize = 1;
const INSIDE: usize = 2;
const SECONDS: usize = 2;
const HEADER: usize = 68;
const BYTES_PER_FRAME: usize = 6;
const SCALE: f32 = 8_388_608.0;

fn utility() -> Row {
    let mut slot = Slot::new(Kind::Utility);
    if let Params::Utility(p) = &mut slot.params {
        p.limit = true;
        p.hard = false;
        p.ceiling_db = 0.0;
        p.lookahead_ms = LOOKAHEAD_MS;
    }
    Row::Effect(slot)
}

fn twenty() -> Vec<Row> {
    (0..TWENTY).map(|_| utility()).collect()
}

fn click() -> Arc<Wave> {
    let mut pcm = vec![0i16; WAVE_SECONDS * SAMPLE_RATE as usize];
    pcm[AT] = CLICK;
    Arc::new(Wave::sound(String::from(NAME), SAMPLE_RATE, 1, pcm))
}

fn audio_track(rack: Vec<Row>) -> Track {
    let mut track = Track::audio();
    track.clips.push(Clip::audio(0, CLIP_BEATS * TICKS_PER_BEAT, String::from(NAME), String::from(NAME)));
    track.rack = rack;
    track
}

fn project() -> Project {
    let mut group = Track::group();
    group.rack = twenty();
    let mut member = audio_track(twenty());
    member.inside = true;
    let mut project = Project::new();
    project.tracks = vec![audio_track(Vec::new()), group, member];
    project.sends[0].rack = twenty();
    project
}

fn render(renderer: &mut Renderer, frames: usize, out: &mut Vec<f32>) {
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut done = 0;
    while done < frames {
        let n = BLOCK.min(frames - done);
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
        done += n;
    }
}

fn loud(out: &[f32]) -> Vec<(usize, f32)> {
    out.iter().enumerate().filter(|(_, sample)| sample.abs() > SILENT).map(|(frame, &sample)| (frame, sample)).collect()
}

fn left(wav: &[u8]) -> Vec<f32> {
    wav[HEADER..].as_chunks::<BYTES_PER_FRAME>().0.iter().map(|frame| (i32::from_le_bytes([0, frame[0], frame[1], frame[2]]) >> 8) as f32 / SCALE).collect()
}

#[test]
fn the_plan_clamps_the_dry_track_at_the_cap_and_counts_the_excess() {
    let project = project();
    let timing = Timing::of(&project, SAMPLE_RATE);
    assert_eq!(timing.cap, CAP, "wanted the cap at half a second, {CAP} at {SAMPLE_RATE}, got {}", timing.cap);
    assert_eq!(project.group_of(INSIDE), Some(GROUPED), "wanted the member inside the group, got {:?}", project.group_of(INSIDE));
    let (member, group) = (&timing.tracks[INSIDE], &timing.tracks[GROUPED]);
    assert_eq!((member.latency, member.to_group, member.uncompensated), (MEMBER, 0, 0), "wanted the member {MEMBER} late with nothing to add before its group, got latency {} to_group {} uncompensated {}", member.latency, member.to_group, member.uncompensated);
    assert_eq!((group.latency, group.to_master), (GROUP, MASTER_IN - GROUP), "wanted the group {GROUP} late and delayed {} into the master, got latency {} to_master {}", MASTER_IN - GROUP, group.latency, group.to_master);
    assert_eq!((timing.returns_in, timing.master_in, timing.output), (GROUP, MASTER_IN, MASTER_IN), "wanted the returns fed at {GROUP}, the master at {MASTER_IN} and the output {MASTER_IN} late, got {} {} {}", timing.returns_in, timing.master_in, timing.output);
    let dry = &timing.tracks[DRY];
    assert_eq!(dry.to_master, CAP, "wanted the dry track's line to the master clamped at the cap {CAP}, got {}", dry.to_master);
    assert_eq!(dry.uncompensated, EXCESS, "wanted the dry track's uncompensated excess {EXCESS}, got {}", dry.uncompensated);
    assert_eq!(timing.longest, Some(GROUPED), "wanted the group named as the path that sets the latency, got {:?}", timing.longest);
}

#[test]
fn a_path_past_the_cap_plays_late_by_the_excess_the_plan_reports() {
    let waves = [click()];
    let project = project();
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(&project, None, &[], &waves);
    let timing = audio.timing();
    let (dry_at, group_at) = (AT + timing.tracks[DRY].to_master, AT + timing.output);
    let excess = timing.tracks[DRY].uncompensated;
    audio.play(0.0);
    let total = SECONDS * SAMPLE_RATE as usize;
    let mut out = Vec::with_capacity(total);
    render(&mut renderer, total, &mut out);
    let heard = loud(&out);
    let frames: Vec<usize> = heard.iter().map(|(frame, _)| *frame).collect();
    assert_eq!(frames, vec![dry_at, group_at], "wanted the dry click at frame {dry_at} and the group's at {group_at}, nothing else, got sound at {frames:?}");
    for (frame, sample) in heard {
        assert_eq!(sample, LEVEL, "wanted the click at frame {frame} at {LEVEL} through transparent limiters, got {sample}");
    }
    assert_eq!(group_at - dry_at, excess, "wanted the dry click {excess} frames before the group's, the excess the plan reports, got {}", group_at - dry_at);
    assert_eq!(excess, EXCESS, "wanted the excess past the cap {EXCESS}, got {excess}");
}

#[test]
fn an_export_past_the_cap_still_renders_today() {
    let waves = [click()];
    let project = project();
    let song = beat_frame(SPAN_BEATS as usize);
    let bytes = support::bounce(&project, &waves, (0, SPAN_BEATS * TICKS_PER_BEAT), Tail::Cut);
    let out = left(&bytes.expect("wanted the export past the cap to render rather than refuse, which is not built yet"));
    assert_eq!(out.len(), song, "wanted the export the span's {song} frames long, got {}", out.len());
    let heard = loud(&out);
    let frames: Vec<usize> = heard.iter().map(|(frame, _)| *frame).collect();
    println!("an export past the cap renders {} frames with its clicks at {heard:?}: the group's at {AT} and the dry track's {EXCESS} frames early", out.len());
    assert!(frames.contains(&AT), "wanted the group's click at frame {AT} in the export, got sound at {frames:?}");
}
