pub mod heap;
use bp::app::{App, Request};
use bp::arp::{Arp, MidiRow};
use bp::audio::vision::Seer;
use bp::audio::{self, Audio, Export, Tail};
use bp::core::Params;
use bp::draw::{Fill, Rect, Tone};
use bp::face::{CELL, FRAME, H, PITCH, W};
use bp::flatten;
use bp::fx::params::motion::MotionBand;
use bp::fx::params::{Motion, MotionMode};
use bp::fx::{Kind, Slot};
use bp::grid::{BAR_W, BEAT_W};
use bp::json::{self, Json};
use bp::mcp::Reply;
use bp::project::{DEFAULT_BPM, Layer, Project, Row, SongNote, VELOCITY};
use bp::wav::Wave;
use std::f64::consts::TAU;
use std::sync::Arc;

pub const SAMPLE_RATE: f64 = 48_000.0;
pub const RATE: u32 = 48_000;
pub const BLOCK: usize = 256;
pub const BLOCKS: usize = 200;
pub const FRAMES_PER_BEAT: f64 = SAMPLE_RATE * 60.0 / DEFAULT_BPM;
pub const AUDIBLE: f32 = 0.01;
pub const SILENT: f32 = 1e-6;
pub const GAP: usize = 1_000;
pub const WITHIN: usize = 1;
pub const WINDOW: usize = 240;
pub const LEVEL: f64 = 0.25;
pub const HALF: f32 = 0.5;
pub const SEND: f32 = 0.5;
pub const HUSH_DB: f64 = -20.0;
pub const CHORD: [u8; 3] = [60, 64, 67];
pub const BOTH: [(usize, u8); 2] = [(0, 60), (1, 67)];
pub const WIDTH: i32 = 1440;
pub const HEIGHT: i32 = 900;
pub const FOLD_OPEN: (i32, i32) = (14, 2);
pub const FOLD_OPEN_INSET: (i32, i32) = (9, 12);
pub const FOLD_CLOSED: (i32, i32) = (2, 14);
pub const FOLD_CLOSED_INSET: (i32, i32) = (12, 9);
pub const GRIP_LINE: (i32, i32) = (13, 1);
pub const GRIP_LINE_INSET: (i32, i32) = (10, 16);
pub const CORE_ROW: usize = 1;
pub const LANE_LINE: i32 = 2;
pub const LANE_LEAST_H: i32 = 40;
pub const LINES_PER_LANE: i32 = 8;
pub const NO_END: f32 = 0.0;

pub fn frames(seconds: f64) -> usize {
    (seconds * SAMPLE_RATE).round() as usize
}

pub fn wav(samples: &[i16]) -> Vec<u8> {
    let data: Vec<u8> = samples.iter().flat_map(|sample| sample.to_le_bytes()).collect();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&((36 + data.len()) as u32).to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&1u16.to_le_bytes());
    bytes.extend_from_slice(&RATE.to_le_bytes());
    bytes.extend_from_slice(&(RATE * 2).to_le_bytes());
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&16u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&(data.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&data);
    bytes
}

pub fn session(app: &mut App) -> Json {
    let text = app.take_requests().into_iter().rev().find_map(|request| if let Request::Remember { json } = request { Some(json) } else { None }).expect("wanted the session remembered after the gesture");
    json::parse(&text).expect("wanted the remembered session to parse")
}

pub fn remembered(app: &mut App) -> Project {
    Project::from_json(session(app).get("project").expect("wanted the remembered session to hold the project")).expect("wanted the remembered project to read")
}

pub fn call(app: &mut App, tool: &str, args: &str) -> Reply {
    let args = json::parse(args).unwrap_or_else(|| panic!("wanted {args} to parse"));
    app.serve_call(tool, &args).unwrap_or_else(|| panic!("wanted {tool} served at once, no gesture or export holding it"))
}

pub fn bounce(project: &Project, waves: &[Arc<Wave>], span: (i32, i32), tail: Tail) -> Option<Vec<u8>> {
    Export::start(project, waves, span, tail)?.take()
}

pub fn song_render(project: &Project, frames: usize) -> [Vec<f32>; 2] {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(project, None, &[], &[]);
    audio.play(0.0);
    let (mut left, mut right) = (Vec::with_capacity(frames), Vec::with_capacity(frames));
    let (mut l, mut r) = ([0.0; BLOCK], [0.0; BLOCK]);
    while left.len() < frames {
        let n = BLOCK.min(frames - left.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        left.extend_from_slice(&l[..n]);
        right.extend_from_slice(&r[..n]);
    }
    [left, right]
}

pub fn mixed(a: &[Vec<f32>; 2], b: &[Vec<f32>; 2]) -> [Vec<f32>; 2] {
    [0, 1].map(|channel| a[channel].iter().zip(&b[channel]).map(|(x, y)| x + y).collect())
}

pub fn first_frame_apart(got: &[f32], wanted: &[f32]) -> Option<usize> {
    got.iter().zip(wanted).position(|(x, y)| x.to_bits() != y.to_bits()).or_else(|| (got.len() != wanted.len()).then(|| got.len().min(wanted.len())))
}

pub fn channels_apart(got: &[Vec<f32>; 2], wanted: &[Vec<f32>; 2]) -> [Option<usize>; 2] {
    [0, 1].map(|channel| first_frame_apart(&got[channel], &wanted[channel]))
}

pub fn peak(blocks: &[Vec<f32>]) -> f32 {
    if blocks.iter().flatten().any(|sample| !sample.is_finite()) {
        return f32::NAN;
    }
    blocks.iter().flatten().fold(0.0f32, |peak, sample| peak.max(sample.abs()))
}

pub fn peak_of(samples: &[f32]) -> f32 {
    if samples.iter().any(|sample| !sample.is_finite()) {
        return f32::NAN;
    }
    samples.iter().fold(0.0f32, |peak, sample| peak.max(sample.abs()))
}

pub fn first_difference(a: &[Vec<f32>], b: &[Vec<f32>]) -> Option<usize> {
    let differing = a.iter().zip(b).position(|(x, y)| x.len() != y.len() || x.iter().zip(y).any(|(p, q)| p.to_bits() != q.to_bits() || !p.is_finite()));
    differing.or((a.len() != b.len()).then(|| a.len().min(b.len())))
}

pub fn worst_delta(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return f32::INFINITY;
    }
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0f32, |worst, delta| if delta.is_nan() { f32::INFINITY } else { worst.max(delta) })
}

pub fn amplitude(samples: &[f32], hz: f64) -> f64 {
    let n = samples.len() as f64;
    let (mut re, mut im, mut weight) = (0.0, 0.0, 0.0);
    for (i, sample) in samples.iter().enumerate() {
        let window = 0.5 - 0.5 * (TAU * i as f64 / n).cos();
        let angle = TAU * hz * i as f64 / SAMPLE_RATE;
        re += f64::from(*sample) * window * angle.cos();
        im -= f64::from(*sample) * window * angle.sin();
        weight += window;
    }
    2.0 * re.hypot(im) / weight
}

pub fn scaled(blocks: &[Vec<f32>], by: f32) -> Vec<Vec<f32>> {
    blocks.iter().map(|block| block.iter().map(|sample| sample * by).collect()).collect()
}

pub fn beat_frame(beat: usize) -> usize {
    (beat as f64 * FRAMES_PER_BEAT).round() as usize
}

pub fn midi(arp: Arp) -> Row {
    Row::Midi(MidiRow { arp, ..MidiRow::DEFAULT })
}

pub fn struck(arp: Arp) -> Row {
    Row::Midi(MidiRow { arp, velocity: VELOCITY, ..MidiRow::DEFAULT })
}

pub fn sent_notes(project: &Project, track: usize, row: usize) -> Vec<SongNote> {
    audio::sent(project).notes(track, row).to_vec()
}

pub fn arp_print(song: &[SongNote], arp: Arp, bpm: f64) -> Vec<SongNote> {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.bpm = bpm;
    project.tracks[0].rack = vec![struck(arp), core(LEVEL)];
    project.tracks[0].clips = flatten::print(&[], song);
    sent_notes(&project, 0, 0)
}

pub fn core_layer(level: f64, patch: Params) -> Layer {
    Layer { level, patch, seed: 0, times: 1 }
}

pub fn core(level: f64) -> Row {
    Row::Core(core_layer(level, Params::DEFAULT))
}

pub fn unseeded(rack: &[Row]) -> Vec<Row> {
    rack.iter().map(|row| row.unseeded()).collect()
}

pub fn effect(kind: Kind) -> Row {
    Row::Effect(Slot::new(kind))
}

pub fn half_wet(kind: Kind) -> Row {
    Row::Effect(Slot { wet: 0.5, ..Slot::new(kind) })
}

pub fn one_band(band: MotionBand) -> Slot {
    let mut motion = Motion { band_count: 1, ..Motion::DEFAULT };
    motion.bands[0] = band;
    Slot { params: bp::fx::Params::Motion(motion), ..Slot::new(Kind::Motion) }
}

pub fn stage(mode: MotionMode) -> Slot {
    one_band(MotionBand { mode, ..MotionBand::DEFAULT })
}

pub struct Onsets {
    pub frame: usize,
    pub quiet: usize,
    pub at: Vec<usize>,
}

impl Default for Onsets {
    fn default() -> Onsets {
        Onsets { frame: 0, quiet: GAP, at: Vec::new() }
    }
}

impl Onsets {
    pub fn feed(&mut self, samples: &[f32]) {
        for &sample in samples {
            if sample.abs() < SILENT {
                self.quiet += 1;
            } else {
                if self.quiet >= GAP {
                    self.at.push(self.frame);
                }
                self.quiet = 0;
            }
            self.frame += 1;
        }
    }
}

pub fn painted(app: &mut App) -> Vec<Fill> {
    app.draw(WIDTH, HEIGHT).to_vec()
}

pub fn inked(fills: &[Fill], rect: Rect) -> bool {
    fills.iter().any(|fill| fill.tone == Tone::Ink100 && fill.rect == rect)
}

pub fn centre(rect: Rect) -> (i32, i32) {
    (rect.x + rect.w / 2, rect.y + rect.h / 2)
}

pub fn lane_rows(fills: &[Fill]) -> Vec<Rect> {
    let mut lines: Vec<Rect> = fills.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.w == LANE_LINE && fill.rect.h >= LANE_LEAST_H).map(|fill| fill.rect).collect();
    lines.sort_by_key(|line| (line.y, line.h, line.x));
    let mut rows: Vec<Rect> = Vec::new();
    for line in lines {
        let alike = |row: &Rect| row.y == line.y && row.h == line.h;
        match rows.iter_mut().find(|row| alike(row)) {
            Some(row) if line.x > row.x + row.w => row.w = line.x - row.x,
            Some(_) => {}
            None => rows.push(Rect::new(line.x, line.y, LANE_LINE, line.h)),
        }
    }
    rows.retain(|row| row.w >= LINES_PER_LANE * BAR_W);
    rows
}

pub fn beat_x(rows: &[Rect], beat: i32) -> i32 {
    rows.iter().map(|row| row.x).min().expect("wanted a lane row") - BAR_W + (f64::from(beat) * BEAT_W) as i32
}

pub fn clip_block(fills: &[Fill]) -> Rect {
    fills.iter().filter(|fill| fill.tone == Tone::Ink25).map(|fill| fill.rect).max_by_key(|rect| rect.w * rect.h).expect("wanted the clip painted on its lane")
}

pub fn face_rect(fills: &[Fill]) -> Rect {
    let top = fills.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.w == W && fill.rect.h == FRAME).map(|fill| fill.rect).min_by_key(|rect| rect.y).expect("wanted the Core face's readout framed");
    Rect::new(top.x, top.y, W, H)
}

pub fn chevrons(fills: &[Fill]) -> Vec<(Rect, bool)> {
    let stage = face_rect(fills).y;
    let mut marks: Vec<(Rect, bool)> = fills
        .iter()
        .filter(|fill| fill.tone == Tone::Ink100 && fill.rect.y >= stage)
        .filter_map(|fill| {
            let Rect { x, y, w, h } = fill.rect;
            if (w, h) == FOLD_OPEN && inked(fills, Rect::new(x + 2, y + 2, 10, 2)) {
                Some((Rect::new(x - FOLD_OPEN_INSET.0, y - FOLD_OPEN_INSET.1, CELL, CELL), true))
            } else if (w, h) == FOLD_CLOSED && inked(fills, Rect::new(x + 2, y + 2, 2, 10)) {
                Some((Rect::new(x - FOLD_CLOSED_INSET.0, y - FOLD_CLOSED_INSET.1, CELL, CELL), false))
            } else {
                None
            }
        })
        .collect();
    marks.sort_by_key(|(mark, _)| (mark.y, mark.x));
    marks
}

pub fn browser_rect(fills: &[Fill]) -> Rect {
    let (root, _) = chevrons(fills).first().copied().expect("wanted the root folder's chevron painted in the browser");
    let left = root.x + PITCH;
    let right = fills.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.y == root.y + FRAME && fill.rect.h == CELL - 2 * FRAME && fill.rect.w == FRAME && fill.rect.x > left + FRAME).map(|fill| fill.rect.x).min().expect("wanted the root row's name cell framed beside its chevron");
    let gripped = inked(fills, Rect::new(right + PITCH + GRIP_LINE_INSET.0, root.y + GRIP_LINE_INSET.1, GRIP_LINE.0, GRIP_LINE.1));
    let handles = if gripped { 4 * PITCH } else { 2 * PITCH };
    Rect::new(root.x, root.y, PITCH + right + FRAME - left + handles, H)
}
