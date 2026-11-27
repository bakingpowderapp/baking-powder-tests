use bp::app::{App, Event, Opened};
use bp::arp::Articulation;
use bp::draw::{Fill, Rect, Tone};
use bp::grid::PLAYHEAD_W;
use bp::keyboard::Key;
use bp::project::{Aim, Clip, Curve, Node, Note, Project, TICKS_PER_BAR, TICKS_PER_BEAT, VELOCITY};
use support::{CORE_ROW, centre, painted, remembered};

const RAIL: i32 = 45;
const TILE: i32 = 120;
const BORDER: i32 = 8;
const PITCH: i32 = TILE - BORDER;
const VOL_W: i32 = 28;
const COLUMN: i32 = PITCH + VOL_W + 2 * BORDER;
const BAR_W: i32 = 120;
const LANE_H: i32 = TILE - 2 * BORDER;
const TRACKS: usize = 6;
const UPPER: usize = 0;
const LOWER: usize = 5;
const TARGET: usize = 1;
const ROOT: u8 = 60;
const FIFTH: u8 = 67;
const SCROLL: i32 = 480;
const INTO_CELL: i32 = 10;
const CAP: i32 = 8;
const NEXT: &str = "/paste/next.json";
const DEFAULT: &str = "/paste/default.json";
const REMOVE: (i32, i32) = (20, 100);

fn lane_y(slot: usize) -> i32 {
    RAIL + slot as i32 * PITCH + BORDER
}

fn clip(pitch: u8) -> Clip {
    Clip { start: 0, length: TICKS_PER_BAR, label: String::new(), notes: vec![Note { pitch, start: 0.0, length: 1.0, velocity: VELOCITY, articulation: Articulation::PLAIN }], file: None }
}

fn project() -> Project {
    let mut project = Project::new();
    while project.tracks.len() < TRACKS {
        project.add_midi_track(project.tracks.len());
    }
    project.tracks[UPPER].clips.push(clip(ROOT));
    project.tracks[LOWER].clips.push(clip(FIFTH));
    let nodes = vec![Node { tick: 0, travel: 0.5, bend: 0.0 }, Node { tick: 2 * TICKS_PER_BEAT, travel: 0.75, bend: 0.0 }];
    project.tracks[UPPER].automation.curves.push(Curve { aim: Aim::knob(CORE_ROW, "OSC 1", "LEVEL"), lfo: None, nodes });
    project
}

fn open(project: &Project) -> App {
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false}}", project.to_json().write());
    let mut app = App::new();
    app.resume(&session);
    painted(&mut app);
    app
}

fn click(app: &mut App, (x, y): (i32, i32)) {
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    app.event(Event::Leave);
}

fn shift_click(app: &mut App, at: (i32, i32)) {
    app.event(Event::Modifiers { shift: true, command: false });
    click(app, at);
    app.event(Event::Modifiers { shift: false, command: false });
}

fn scroll(app: &mut App, dy: i32) {
    app.event(Event::Scroll { x: COLUMN + 3 * BAR_W, y: RAIL + 2 * PITCH, dx: 0, dy });
    app.event(Event::Leave);
}

fn command(app: &mut App, key: char) {
    app.event(Event::Key { key: Key::Char(key), shift: false, command: true, repeat: false });
}

fn escape(app: &mut App) {
    app.event(Event::Key { key: Key::Escape, shift: false, command: false, repeat: false });
}

fn clip_blocks(fills: &[Fill]) -> Vec<Rect> {
    let mut found: Vec<Rect> = fills.iter().filter(|fill| fill.tone == Tone::Ink25 && fill.rect.x >= COLUMN && (BAR_W - 10..=BAR_W).contains(&fill.rect.w) && fill.rect.h > LANE_H / 2 && fill.rect.h < TILE).map(|fill| fill.rect).collect();
    found.sort_by_key(|rect| (rect.y, rect.x));
    found
}

fn only_clip(app: &mut App) -> Rect {
    let blocks = clip_blocks(&painted(app));
    assert_eq!(blocks.len(), 1, "wanted one plain clip block in view, got {blocks:?}");
    blocks[0]
}

fn marker(fills: &[Fill], slot: usize, tick: i32) -> bool {
    let x = COLUMN + (tick * BAR_W) / TICKS_PER_BAR;
    let bar = Rect::new(x, lane_y(slot), PLAYHEAD_W, LANE_H);
    let cap = Rect::new(x - CAP / 2, lane_y(slot), CAP, PLAYHEAD_W);
    [bar, cap].iter().all(|wanted| fills.iter().any(|fill| fill.tone == Tone::Ink100 && fill.rect == *wanted))
}

fn starts(project: &Project, track: usize) -> Vec<i32> {
    project.tracks.get(track).map_or_else(Vec::new, |owner| owner.clips.iter().map(|clip| clip.start).collect())
}

fn pitches(project: &Project, track: usize, start: i32) -> Vec<u8> {
    project.tracks[track].clips.iter().find(|clip| clip.start == start).map_or_else(Vec::new, |clip| clip.notes.iter().map(|note| note.pitch).collect())
}

fn level_nodes(project: &Project, track: usize) -> Vec<i32> {
    project.tracks[track].automation.curves.iter().filter(|curve| matches!(&curve.aim, Aim::Knob { band, name, .. } if band == "OSC 1" && name == "LEVEL")).flat_map(|curve| curve.nodes.iter().map(|node| node.tick)).collect()
}

fn gather_both(app: &mut App) {
    let upper = only_clip(app);
    click(app, centre(upper));
    scroll(app, -SCROLL);
    let lower = only_clip(app);
    shift_click(app, centre(lower));
    command(app, 'c');
    scroll(app, SCROLL);
}

fn copy_and_point(app: &mut App) {
    let upper = only_clip(app);
    click(app, centre(upper));
    command(app, 'c');
    click(app, (COLUMN + 2 * BAR_W + INTO_CELL, lane_y(TARGET) + LANE_H / 2));
    assert!(marker(&painted(app), TARGET, 2 * TICKS_PER_BAR), "wanted the paste marker standing at bar 3 in the clicked lane before the song changes");
}

fn remove(app: &mut App, slot: usize) {
    let (x, y) = (REMOVE.0, RAIL + slot as i32 * PITCH + REMOVE.1);
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    app.event(Event::Leave);
}

fn load(app: &mut App, project: &Project) {
    let opened = app.opened(String::from(NEXT), Some(&project.to_json().write()), None);
    assert_eq!(opened, Opened::Project, "wanted {NEXT} to open as a song, got {opened:?}");
}

fn assert_pasted_at_the_start(app: &mut App, after: &str) {
    let standing = marker(&painted(app), TARGET, 2 * TICKS_PER_BAR);
    command(app, 'v');
    let project = remembered(app);
    let landed = (standing, starts(&project, UPPER), starts(&project, TARGET));
    assert_eq!(landed, (false, vec![0], Vec::new()), "wanted no paste marker left at the last song's point and the paste at the first track's start after {after}, got the marker standing: {}, the first track's clips at {:?} and the pointed track's at {:?}", landed.0, landed.1, landed.2);
}

#[test]
fn clips_gathered_across_offscreen_tracks_paste_where_the_lane_was_clicked() {
    let mut app = open(&project());
    gather_both(&mut app);
    click(&mut app, (COLUMN + 2 * BAR_W + INTO_CELL, lane_y(TARGET) + LANE_H / 2));
    assert!(marker(&painted(&mut app), TARGET, 2 * TICKS_PER_BAR), "wanted the paste marker standing at bar 3 in the clicked lane");
    command(&mut app, 'v');
    let project = remembered(&mut app);
    assert_eq!(project.tracks.len(), TRACKS + 1, "wanted a track added below for the lower clip, got {} tracks", project.tracks.len());
    assert_eq!(starts(&project, TARGET), vec![2 * TICKS_PER_BAR], "wanted the upper clip pasted at bar 3 on the clicked track, got {:?}", starts(&project, TARGET));
    assert_eq!(starts(&project, TARGET + LOWER), vec![2 * TICKS_PER_BAR], "wanted the lower clip five tracks down at bar 3, got {:?}", starts(&project, TARGET + LOWER));
    assert_eq!(pitches(&project, TARGET, 2 * TICKS_PER_BAR), vec![ROOT], "wanted the upper clip's note carried");
    assert_eq!(pitches(&project, TARGET + LOWER, 2 * TICKS_PER_BAR), vec![FIFTH], "wanted the lower clip's note carried");
    assert_eq!(level_nodes(&project, TARGET), vec![2 * TICKS_PER_BAR, 2 * TICKS_PER_BAR + 2 * TICKS_PER_BEAT], "wanted the level curve under the upper clip carried to bar 3, got {:?}", level_nodes(&project, TARGET));
    assert_eq!((starts(&project, UPPER), starts(&project, LOWER)), (vec![0], vec![0]), "wanted the originals left in place");
    assert!(marker(&painted(&mut app), TARGET, 3 * TICKS_PER_BAR), "wanted the paste marker stepped past the pasted bar");
    command(&mut app, 'v');
    let tiled = remembered(&mut app);
    assert_eq!(starts(&tiled, TARGET), vec![2 * TICKS_PER_BAR, 3 * TICKS_PER_BAR], "wanted a second paste to tile after the first, got {:?}", starts(&tiled, TARGET));
    assert_eq!(starts(&tiled, TARGET + LOWER), vec![2 * TICKS_PER_BAR, 3 * TICKS_PER_BAR], "wanted the lower clip tiled too, got {:?}", starts(&tiled, TARGET + LOWER));
    command(&mut app, 'z');
    let undone = remembered(&mut app);
    assert_eq!(starts(&undone, TARGET), vec![2 * TICKS_PER_BAR], "wanted one undo to take back the second paste alone, got {:?}", starts(&undone, TARGET));
    assert_eq!(starts(&undone, TARGET + LOWER), vec![2 * TICKS_PER_BAR], "wanted the lower track's second paste undone with it, got {:?}", starts(&undone, TARGET + LOWER));
}

#[test]
fn a_paste_without_a_point_lands_at_the_playhead_on_the_selected_track() {
    let mut app = open(&project());
    let upper = only_clip(&mut app);
    click(&mut app, centre(upper));
    command(&mut app, 'c');
    command(&mut app, 'v');
    let project = remembered(&mut app);
    assert_eq!(project.tracks.len(), TRACKS, "wanted no track added by a paste over the selected track, got {}", project.tracks.len());
    for track in 0..TRACKS {
        let wanted = if track == UPPER || track == LOWER { vec![0] } else { Vec::new() };
        assert_eq!(starts(&project, track), wanted, "wanted track {track} as it was after pasting at the playhead on the selected track, got {:?}", starts(&project, track));
    }
    assert!(marker(&painted(&mut app), UPPER, TICKS_PER_BAR), "wanted the paste marker to appear after the pasted bar on the selected track");
}

#[test]
fn the_marker_leaves_on_escape_and_when_a_clip_is_picked() {
    let mut app = open(&project());
    let at = (COLUMN + BAR_W + INTO_CELL, lane_y(TARGET) + LANE_H / 2);
    click(&mut app, at);
    assert!(marker(&painted(&mut app), TARGET, TICKS_PER_BAR), "wanted the marker at bar 2 after the lane click");
    escape(&mut app);
    assert!(!marker(&painted(&mut app), TARGET, TICKS_PER_BAR), "wanted escape to clear the paste marker");
    click(&mut app, at);
    let upper = only_clip(&mut app);
    click(&mut app, centre(upper));
    assert!(!marker(&painted(&mut app), TARGET, TICKS_PER_BAR), "wanted picking a clip to clear the paste marker");
}

#[test]
fn a_paste_point_does_not_outlive_its_song_when_another_is_opened() {
    let mut app = open(&project());
    copy_and_point(&mut app);
    load(&mut app, &Project::new());
    assert_pasted_at_the_start(&mut app, "another song was opened");
}

#[test]
fn a_paste_point_does_not_outlive_its_song_when_a_default_project_begins() {
    let mut app = open(&project());
    copy_and_point(&mut app);
    app.begin(String::from(DEFAULT), Some(&Project::new().to_json().write()));
    assert_pasted_at_the_start(&mut app, "a default project began");
}

#[test]
fn a_paste_point_does_not_outlive_its_song_when_an_undo_goes_back_across_its_load() {
    let mut app = open(&Project::new());
    load(&mut app, &project());
    copy_and_point(&mut app);
    command(&mut app, 'z');
    let tracks = remembered(&mut app).tracks.len();
    assert_eq!(tracks, Project::new().tracks.len(), "wanted one undo to go back across the load to the song before it, got a song of {tracks} tracks");
    assert_pasted_at_the_start(&mut app, "an undo went back across the load");
}

#[test]
fn a_paste_point_follows_its_track_when_a_track_above_is_removed() {
    let mut app = open(&project());
    copy_and_point(&mut app);
    remove(&mut app, UPPER);
    let tracks = remembered(&mut app).tracks.len();
    assert_eq!(tracks, TRACKS - 1, "wanted the press on the first track's X to remove it, got a song of {tracks} tracks");
    assert!(marker(&painted(&mut app), TARGET - 1, 2 * TICKS_PER_BAR), "wanted the paste marker still on its own track at bar 3, one slot up once the track above was removed");
    command(&mut app, 'v');
    let project = remembered(&mut app);
    let landed = (starts(&project, TARGET - 1), starts(&project, TARGET));
    assert_eq!(landed, (vec![2 * TICKS_PER_BAR], Vec::new()), "wanted the paste on the pointed track at bar 3 and nothing on the track under it, got the pointed track's clips at {:?} and the next track's at {:?}", landed.0, landed.1);
}

#[test]
fn a_paste_point_leaves_with_its_track() {
    let mut app = open(&project());
    copy_and_point(&mut app);
    remove(&mut app, TARGET);
    let tracks = remembered(&mut app).tracks.len();
    assert_eq!(tracks, TRACKS - 1, "wanted the press on the pointed track's X to remove it, got a song of {tracks} tracks");
    assert!(!marker(&painted(&mut app), TARGET, 2 * TICKS_PER_BAR), "wanted no paste marker left on the track that took the removed track's slot");
    command(&mut app, 'v');
    let project = remembered(&mut app);
    assert!(!starts(&project, TARGET).contains(&(2 * TICKS_PER_BAR)), "wanted no paste at the removed track's point on the track that took its slot, got its clips at {:?}", starts(&project, TARGET));
}
