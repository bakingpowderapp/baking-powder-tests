use bp::app::{App, Event};
use bp::arp::Articulation;
use bp::draw::Rect;
use bp::grid::{BAR_W, BEAT_W};
use bp::json::Json;
use bp::project::{Clip, Note, Project, TICKS_PER_BAR, TICKS_PER_BEAT, VELOCITY, beats};
use support::{CHORD, clip_block, lane_rows, painted, session};

type Played = (u8, f32, f32);
type Laid = Vec<(i32, i32, Vec<Played>)>;

const CUT_HOLD_SECONDS: f32 = 1.5;
const PAST_THE_HOLD: f32 = CUT_HOLD_SECONDS + 0.25;
const TICK: f32 = 1.0 / 60.0;
const QUARTER: &str = "1/4";
const BARS: i32 = 2;
const LENGTH: i32 = BARS * TICKS_PER_BAR;
const CUT: i32 = 2 * TICKS_PER_BEAT;
const BEFORE: Played = (CHORD[0], 0.0, 1.0);
const ACROSS: Played = (CHORD[1], 1.5, 1.0);
const AFTER: Played = (CHORD[2], 3.0, 1.0);

fn note((pitch, start, length): Played) -> Note {
    Note { pitch, start, length, velocity: VELOCITY, articulation: Articulation::PLAIN }
}

fn open() -> App {
    let mut project = Project::new();
    project.tracks[0].clips.push(Clip { start: 0, length: LENGTH, label: String::new(), notes: [BEFORE, ACROSS, AFTER].into_iter().map(note).collect(), file: None });
    let session = format!("{{\"project\":{},\"snap\":\"{QUARTER}\",\"keyboard\":false}}", project.to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app
}

fn laid(session: &Json) -> Laid {
    let project = Project::from_json(session.get("project").expect("wanted the remembered session to hold the project")).expect("wanted the remembered project to read");
    project.tracks[0].clips.iter().map(|clip| (clip.start, clip.length, clip.notes.iter().map(|note| (note.pitch, note.start, note.length)).collect())).collect()
}

fn picked(session: &Json) -> Vec<(i64, f64)> {
    let mut picks: Vec<(i64, f64)> = session
        .get("picked")
        .and_then(Json::as_array)
        .unwrap_or(&[])
        .iter()
        .filter_map(|pick| match pick.as_array()? {
            [track, beat] => Some((track.as_i64()?, beat.as_f64()?)),
            _ => None,
        })
        .collect();
    picks.sort_by(|a, b| a.1.total_cmp(&b.1));
    picks
}

fn column(rows: &[Rect]) -> i32 {
    rows.iter().map(|row| row.x).min().expect("wanted a lane row") - BAR_W
}

fn beat_x(rows: &[Rect], beats: f64) -> i32 {
    column(rows) + (beats * BEAT_W) as i32
}

fn press_at_cut(app: &mut App) -> (i32, i32) {
    let fills = painted(app);
    let block = clip_block(&fills);
    let (x, y) = (beat_x(&lane_rows(&fills), beats(CUT)), block.y + block.h / 2);
    app.event(Event::Down { x, y, clicks: 1 });
    (x, y)
}

fn hold(app: &mut App, seconds: f32) {
    for _ in 0..(seconds / TICK).ceil() as usize {
        app.tick(TICK);
    }
}

#[test]
fn a_still_press_held_past_the_hold_cuts_the_clip_at_the_pressed_beat() {
    let mut app = open();
    let (x, y) = press_at_cut(&mut app);
    hold(&mut app, PAST_THE_HOLD);
    app.event(Event::Up { x, y });
    let session = session(&mut app);
    let cut = beats(CUT) as f32;
    let head = vec![BEFORE, (ACROSS.0, ACROSS.1, cut - ACROSS.1)];
    let tail = vec![(ACROSS.0, 0.0, ACROSS.1 + ACROSS.2 - cut), (AFTER.0, AFTER.1 - cut, AFTER.2)];
    let wanted: Laid = vec![(0, CUT, head), (CUT, LENGTH - CUT, tail)];
    let got = laid(&session);
    assert_eq!(got, wanted, "wanted a still press held {PAST_THE_HOLD} s at tick {CUT} to cut the {BARS}-bar clip there, the note across the cut split between the two, got {got:?}");
    let picks = picked(&session);
    let both = vec![(0, 0.0), (0, beats(CUT))];
    assert_eq!(picks, both, "wanted both clips selected after the cut, at beats 0 and {}, got {picks:?}", beats(CUT));
}

#[test]
fn a_press_that_moves_before_the_hold_drags_the_clip_whole() {
    let mut app = open();
    let (x, y) = press_at_cut(&mut app);
    let early = CUT_HOLD_SECONDS / 2.0;
    hold(&mut app, early);
    app.event(Event::Move { x: x + BAR_W, y });
    hold(&mut app, PAST_THE_HOLD);
    app.event(Event::Up { x: x + BAR_W, y });
    let got = laid(&session(&mut app));
    let wanted: Laid = vec![(TICKS_PER_BAR, LENGTH, vec![BEFORE, ACROSS, AFTER])];
    assert_eq!(got, wanted, "wanted a press moved a bar {early} s in, then held {PAST_THE_HOLD} s more, to drag the clip whole to tick {TICKS_PER_BAR} and never cut it, got {got:?}");
}
