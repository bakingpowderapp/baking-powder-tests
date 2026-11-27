use bp::app::{App, Event, Request};
use bp::draw::{Fill, Rect, Tone};
use bp::face::{CELL, FRAME};
use bp::grid::PLAYHEAD_W;
use bp::json::{self, Json};
use bp::project::{Clip, DEFAULT_BPM, Project, TICKS_PER_BEAT, Track};
use support::{RATE, call, centre, clip_block, painted, wav};

const PATH: &str = "/x/tone.wav";
const NAME: &str = "tone";
const WAVE: &str = "tracks/0/clips/0/wave";
const FILE_SECONDS: f64 = 4.0;
const HELD: i16 = 16_384;
const NODE: i32 = 9;
const HEAD: f64 = 1.0;
const BEATS: i32 = 4;
const START: f64 = HEAD;
const END: f64 = HEAD + 2.0;
const AWAY: (i32, i32) = (5, 5);
const ARM: i32 = 6;
const RISE_BOTTOM: usize = 0;
const RISE_TOP: usize = 1;
const FALL_TOP: usize = 2;
const FALL_BOTTOM: usize = 3;
const LIFT: i32 = 80;
const SHORTER: (i32, f64, i32) = (8, HEAD, 2);
const LONGER: (i32, f64, i32) = (12, 0.5, 6);
const SECONDS_PER_BEAT: f64 = 60.0 / DEFAULT_BPM;
const ROUNDED: f64 = 1e-9;

fn clip(beat: i32, head: f64, beats: i32) -> Clip {
    let mut clip = Clip::audio(beat * TICKS_PER_BEAT, beats * TICKS_PER_BEAT, String::from(NAME), String::from(NAME));
    if let Some(file) = &mut clip.file {
        file.head = head;
    }
    clip
}

fn loaded(clips: Vec<Clip>, picked: &[i32]) -> App {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0] = Track::audio();
    project.tracks[0].clips = clips;
    let picks: Vec<String> = picked.iter().map(|beat| format!("[0,{beat}]")).collect();
    let aside = if picked.is_empty() { "spectrum" } else { "wave" };
    let session = format!("{{\"project\":{},\"keyboard\":false,\"palette\":false,\"automation\":false,\"beat_w\":120,\"aside\":\"{aside}\",\"picked\":[{}],\"origins\":[{{\"name\":\"{NAME}\",\"from\":\"{PATH}\"}}]}}", project.to_json().write(), picks.join(","));
    let mut app = App::new();
    app.resume(&session);
    loop {
        let requests = app.take_requests();
        if requests.is_empty() {
            break;
        }
        for request in requests {
            if let Request::Read { path } = request {
                app.read(path, Some(wav(&vec![HELD; (FILE_SECONDS * f64::from(RATE)) as usize])));
            }
        }
    }
    app
}

fn open(head: f64, beats: i32) -> App {
    let mut app = loaded(vec![clip(0, head, beats)], &[]);
    let (x, y) = centre(clip_block(&painted(&mut app)));
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    park(&mut app);
    app
}

fn trimmed() -> App {
    open(HEAD, BEATS)
}

fn park(app: &mut App) {
    app.event(Event::Move { x: AWAY.0, y: AWAY.1 });
}

fn body() -> Rect {
    let fills = painted(&mut open(0.0, 2 * BEATS));
    let inked = |fill: &&Fill| fill.tone == Tone::Ink100;
    let shelf_under = |line: Rect| fills.iter().filter(inked).find(|fill| fill.rect.h == FRAME && fill.rect.w > CELL && fill.rect.x == line.x - FRAME && fill.rect.y == line.y + line.h).map(|fill| fill.rect);
    let lines: Vec<(Rect, Rect)> = fills.iter().filter(inked).filter(|fill| fill.rect.w == PLAYHEAD_W && fill.rect.h >= CELL).filter_map(|fill| Some((fill.rect, shelf_under(fill.rect)?))).collect();
    let [(entry, shelf)] = lines.as_slice() else {
        panic!("wanted one entry line standing on the span lane's frame, found {}: {lines:?}", lines.len());
    };
    Rect::new(entry.x, entry.y, shelf.w - 2 * FRAME, entry.h)
}

fn nodes(app: &mut App, body: Rect) -> [Rect; 4] {
    park(app);
    let found: Vec<Rect> = painted(app).iter().filter(|fill| fill.tone == Tone::Ink100 && (fill.rect.w, fill.rect.h) == (NODE, NODE) && body.intersect(fill.rect) == fill.rect).map(|fill| fill.rect).collect();
    let [rise_bottom, rise_top, fall_top, fall_bottom] = found.as_slice() else {
        panic!("wanted the wave's four fade nodes painted, found {}: {found:?}", found.len());
    };
    assert!(rise_top.y == fall_top.y && rise_bottom.y == fall_bottom.y && rise_top.y < rise_bottom.y, "wanted two upper nodes at the wave's top and two lower at its foot, got {found:?}");
    [*rise_bottom, *rise_top, *fall_top, *fall_bottom]
}

fn seconds(body: Rect, node: Rect) -> f64 {
    f64::from(node.x + NODE / 2 - body.x) / f64::from(body.w) * FILE_SECONDS
}

fn x_at(body: Rect, seconds: f64) -> i32 {
    body.x + (seconds / FILE_SECONDS * f64::from(body.w)).round() as i32
}

fn pixel(body: Rect) -> f64 {
    1.5 * FILE_SECONDS / f64::from(body.w)
}

fn resting(app: &mut App, body: Rect) -> [f64; 4] {
    nodes(app, body).map(|node| seconds(body, node))
}

fn near(got: f64, wanted: f64, body: Rect) -> bool {
    (got - wanted).abs() <= pixel(body)
}

fn all_near(got: [f64; 4], wanted: [f64; 4], body: Rect) -> bool {
    got.iter().zip(wanted).all(|(got, wanted)| near(*got, wanted, body))
}

fn drag(app: &mut App, node: Rect, to_x: i32) {
    let (x, y) = centre(node);
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Move { x: x + if to_x >= x { ARM } else { -ARM }, y });
    app.event(Event::Move { x: to_x, y });
    app.event(Event::Up { x: to_x, y });
    park(app);
}

fn shift_press(app: &mut App, node: Rect) {
    let (x, y) = centre(node);
    app.event(Event::Modifiers { shift: true, command: false });
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    app.event(Event::Modifiers { shift: false, command: false });
    park(app);
}

fn fade(app: &mut App, key: &str) -> Option<(f64, f64)> {
    fade_on(app, WAVE, key)
}

fn fade_on(app: &mut App, wave: &str, key: &str) -> Option<(f64, f64)> {
    let reply = call(app, "read", &format!(r#"{{"path":"{wave}"}}"#));
    let wave = json::parse(&reply.text).unwrap_or_else(|| panic!("wanted the clip's wave read, got {}", reply.text));
    let fade = wave.get(key)?;
    Some((fade.get("fromSeconds").and_then(Json::as_f64)?, fade.get("toSeconds").and_then(Json::as_f64)?))
}

fn written(app: &mut App, path: &str, value: &str) {
    let reply = call(app, "write", &format!(r#"{{"path":"{path}","value":{value}}}"#));
    assert!(!reply.failed, "wanted {value} written at {path}, got {}", reply.text);
}

fn risen(app: &mut App, body: Rect, to: f64) {
    let upper = nodes(app, body)[RISE_TOP];
    drag(app, upper, x_at(body, to));
}

#[test]
fn an_undrawn_fades_nodes_rest_at_the_clips_own_start_and_end_and_follow_its_trims() {
    let body = body();
    let mut app = trimmed();
    assert!(all_near(resting(&mut app, body), [START, START, END, END], body), "wanted the left pair where the clip starts in its file and the right pair where it ends, got {:?} s", resting(&mut app, body));
    written(&mut app, &format!("{WAVE}/headSeconds"), "1.5");
    assert!(all_near(resting(&mut app, body), [1.5, 1.5, 3.5, 3.5], body), "wanted both pairs moved with the clip's start, got {:?} s", resting(&mut app, body));
    written(&mut app, "tracks/0/clips/0/lengthBeats", "2");
    assert!(all_near(resting(&mut app, body), [1.5, 1.5, 2.5, 2.5], body), "wanted the right pair moved in with the shortened clip, got {:?} s", resting(&mut app, body));
    assert_eq!((fade(&mut app, "fadeIn"), fade(&mut app, "fadeOut")), (None, None), "wanted no fade made by resting nodes");
}

#[test]
fn a_drag_on_an_upper_node_draws_a_fade_from_the_clips_edge() {
    let body = body();
    let mut app = trimmed();
    risen(&mut app, body, 2.0);
    let rise = fade(&mut app, "fadeIn").expect("wanted a rise drawn by the drag on the left upper node");
    assert!(rise.0 == START && near(rise.1, 2.0, body), "wanted the rise to start where the clip starts and reach full level under the pointer, got {rise:?}");
    let upper = nodes(&mut app, body)[FALL_TOP];
    drag(&mut app, upper, x_at(body, 2.5));
    let fall = fade(&mut app, "fadeOut").expect("wanted a fall drawn by the drag on the right upper node");
    assert!(near(fall.0, 2.5, body) && near(fall.1, END, body), "wanted the fall to start under the pointer and end where the clip ends, got {fall:?}");
    assert!(all_near(resting(&mut app, body), [START, rise.1, fall.0, END], body), "wanted the four nodes on the two fades' ends, got {:?} s", resting(&mut app, body));
    let lower = nodes(&mut app, body)[RISE_BOTTOM];
    drag(&mut app, lower, x_at(body, 1.5));
    let moved = fade(&mut app, "fadeIn").expect("wanted the rise kept");
    assert!(near(moved.0, 1.5, body) && moved.1 == rise.1, "wanted the lower node to move where the sound starts to rise and leave where it reaches full level, got {moved:?}");
}

#[test]
fn the_two_fades_cannot_cross_and_a_seam_parts_the_square_their_upper_nodes_share() {
    let body = body();
    let mut app = trimmed();
    risen(&mut app, body, 2.0);
    let rise = fade(&mut app, "fadeIn").expect("wanted the rise drawn");
    let upper = nodes(&mut app, body)[FALL_TOP];
    drag(&mut app, upper, x_at(body, 1.25));
    let fall = fade(&mut app, "fadeOut").expect("wanted a fall drawn by the drag");
    assert_eq!(fall.0, rise.1, "wanted the fall's upper node pushed against the rise to stop where the rise reaches full level");
    assert_eq!(fade(&mut app, "fadeIn"), Some(rise), "wanted the rise left where it was");
    let met = nodes(&mut app, body);
    assert_eq!(met[RISE_TOP], met[FALL_TOP], "wanted the two upper nodes in one square where the fades meet");
    let seam = |app: &mut App, node: Rect| painted(app).iter().any(|fill| fill.tone == Tone::Ground && fill.rect.h == NODE && fill.rect.w < NODE / 2 && fill.rect.y == node.y && fill.rect.x > node.x && fill.rect.x + fill.rect.w < node.x + NODE);
    assert!(seam(&mut app, met[RISE_TOP]), "wanted a line down the middle of the square the two upper nodes share");
    assert!(!seam(&mut app, met[RISE_BOTTOM]) && !seam(&mut app, met[FALL_BOTTOM]), "wanted no seam on a lower node that stands alone");
    drag(&mut app, met[RISE_TOP], x_at(body, 1.5));
    let pulled = fade(&mut app, "fadeIn").expect("wanted the rise kept");
    assert!(near(pulled.1, 1.5, body) && fade(&mut app, "fadeOut") == Some(fall), "wanted a pull left on the shared square to take the rise's node and leave the fall, got {pulled:?}");
}

#[test]
fn an_upper_node_pulled_back_to_its_edge_or_shift_pressed_removes_the_fade_and_a_lower_one_returns() {
    let body = body();
    let mut app = trimmed();
    risen(&mut app, body, 2.0);
    let upper = nodes(&mut app, body)[RISE_TOP];
    drag(&mut app, upper, x_at(body, START) - 3 * ARM);
    assert_eq!(fade(&mut app, "fadeIn"), None, "wanted the rise gone once its upper node is pulled back to the clip's start");
    risen(&mut app, body, 2.0);
    let lower = nodes(&mut app, body)[RISE_BOTTOM];
    drag(&mut app, lower, x_at(body, 1.5));
    let lower = nodes(&mut app, body)[RISE_BOTTOM];
    shift_press(&mut app, lower);
    let back = fade(&mut app, "fadeIn").expect("wanted the rise kept by a SHIFT-press on its lower node");
    assert!(back.0 == START && near(back.1, 2.0, body), "wanted the lower node returned to the clip's start and the upper left where it was, got {back:?}");
    let upper = nodes(&mut app, body)[RISE_TOP];
    shift_press(&mut app, upper);
    assert_eq!(fade(&mut app, "fadeIn"), None, "wanted a SHIFT-press on the upper node to remove the rise");
    assert!(all_near(resting(&mut app, body), [START, START, END, END], body), "wanted all four nodes back at rest on the clip's edges, got {:?} s", resting(&mut app, body));
}

#[test]
fn a_drawn_fade_stays_where_it_was_drawn_through_a_trim_and_returns_to_the_clips_edge_when_touched() {
    let body = body();
    let mut app = trimmed();
    risen(&mut app, body, 1.5);
    let drawn = fade(&mut app, "fadeIn").expect("wanted the rise drawn");
    written(&mut app, &format!("{WAVE}/headSeconds"), "2");
    assert_eq!(fade(&mut app, "fadeIn"), Some(drawn), "wanted the rise left where it was drawn in the file when the clip's start is trimmed past it");
    let placed = resting(&mut app, body);
    assert!(near(placed[RISE_BOTTOM], drawn.0, body) && near(placed[RISE_TOP], drawn.1, body), "wanted its nodes still where it was drawn, got {placed:?} s");
    let upper = nodes(&mut app, body)[RISE_TOP];
    drag(&mut app, upper, x_at(body, 2.5));
    let redrawn = fade(&mut app, "fadeIn").expect("wanted a rise after the drag");
    assert!(redrawn.0 == 2.0 && near(redrawn.1, 2.5, body), "wanted the touched rise drawn anew from the clip's edge to the pointer, got {redrawn:?}");
}

fn bend(app: &mut App) -> f64 {
    let reply = call(app, "read", &format!(r#"{{"path":"{WAVE}/fadeIn"}}"#));
    json::parse(&reply.text).and_then(|fade| fade.get("bend").and_then(Json::as_f64)).unwrap_or(0.0)
}

fn shift_drag(app: &mut App, (x, y): (i32, i32), by: i32, clicks: i32) {
    app.event(Event::Modifiers { shift: true, command: false });
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks });
    app.event(Event::Move { x, y: y - by });
    app.event(Event::Up { x, y: y - by });
    app.event(Event::Modifiers { shift: false, command: false });
    park(app);
}

#[test]
fn a_shift_drag_between_a_fades_nodes_bends_it_and_a_shift_double_press_straightens_it() {
    let body = body();
    let mut app = trimmed();
    risen(&mut app, body, 2.0);
    let ends = fade(&mut app, "fadeIn");
    let middle = (x_at(body, 1.5), body.y + body.h / 2);
    assert_eq!(bend(&mut app), 0.0, "wanted a fresh rise straight");
    shift_drag(&mut app, middle, LIFT, 1);
    let lifted = bend(&mut app);
    assert!(lifted > 0.0, "wanted a SHIFT-drag up inside the rise to lift its level sooner, got a bend of {lifted}");
    shift_drag(&mut app, middle, -2 * LIFT, 1);
    let held = bend(&mut app);
    assert!(held < lifted, "wanted a SHIFT-drag down to hold the level back, got a bend of {held} after {lifted}");
    assert_eq!(fade(&mut app, "fadeIn"), ends, "wanted the bend to leave the rise's two ends where they were");
    shift_drag(&mut app, middle, 0, 2);
    assert_eq!(bend(&mut app), 0.0, "wanted a SHIFT double-press inside the rise to straighten it");
}

#[test]
fn a_fade_out_drawn_on_the_shown_clip_ends_as_far_before_each_picked_clips_own_end() {
    let body = body();
    let mut app = loaded(vec![clip(0, HEAD, BEATS), clip(SHORTER.0, SHORTER.1, SHORTER.2), clip(LONGER.0, LONGER.1, LONGER.2)], &[LONGER.0, SHORTER.0, 0]);
    let upper = nodes(&mut app, body)[FALL_TOP];
    drag(&mut app, upper, x_at(body, 2.5));
    let shown = fade(&mut app, "fadeOut").expect("wanted a fall drawn on the shown clip");
    assert!(near(shown.0, 2.5, body) && shown.1 == END, "wanted the shown clip's fall to start under the pointer and end where the clip ends, got {shown:?}");
    for (index, (_, head, beats), what) in [(1, SHORTER, "the shorter picked clip"), (2, LONGER, "the longer picked clip")] {
        let end = head + f64::from(beats) * SECONDS_PER_BEAT;
        let fall = fade_on(&mut app, &format!("tracks/0/clips/{index}/wave"), "fadeOut").unwrap_or_else(|| panic!("wanted {what} to take the fall too"));
        let wanted = (shown.0 + end - END, end);
        assert!((fall.0 - wanted.0).abs() < ROUNDED && fall.1 == wanted.1, "wanted {what} to fall over the same stretch and stop where it ends, {end} s into the file, {wanted:?}, got {fall:?}");
    }
}
