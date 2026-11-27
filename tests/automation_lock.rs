use bp::app::{App, Event, Request};
use bp::arp::Articulation;
use bp::keyboard::Key;
use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::draw::{Fill, Rect, Tone};
use bp::grid;
use bp::json;
use bp::project::{Aim, Clip, Curve, Node, Note, Project, TICKS_PER_BAR, TICKS_PER_BEAT, Track, VELOCITY};
use support::{BLOCK, CHORD, CORE_ROW, FRAMES_PER_BEAT, SAMPLE_RATE, clip_block, painted, remembered};

const MOVE_BEATS: i32 = 8;
const LISTEN_BEATS: f64 = 0.5;

fn swelling_project(lock: bool) -> Project {
    let notes = CHORD.iter().map(|&pitch| Note { pitch, start: 0.0, length: 4.0, velocity: VELOCITY, articulation: Articulation::PLAIN }).collect();
    let swell = Curve { aim: Aim::knob(CORE_ROW, "OSC 1", "LEVEL"), lfo: None, nodes: vec![Node { tick: 0, travel: 0.0, bend: 0.0 }, Node { tick: 3 * TICKS_PER_BEAT, travel: 1.0, bend: 0.0 }] };
    let mut project = Project::new();
    project.lock = lock;
    project.tracks.truncate(1);
    project.tracks[0].automation.curves.push(swell);
    project.tracks[0].clips.push(Clip { start: 0, length: TICKS_PER_BAR, label: String::new(), notes, file: None });
    project
}

fn open(project: &Project, automation: bool) -> App {
    let auto = if automation { "track" } else { "off" };
    let session = format!("{{\"project\":{},\"auto\":\"{auto}\",\"keyboard\":false}}", project.to_json().write());
    let mut app = App::new();
    app.resume(&session);
    app
}

fn header_washes(fills: &[Fill]) -> Vec<Rect> {
    fills.iter().filter(|fill| fill.tone == Tone::Ink100 && fill.rect.y < 45 && fill.rect.h >= 30 && fill.rect.w >= 30).map(|fill| fill.rect).collect()
}

fn click(app: &mut App, x: i32, y: i32, clicks: i32) {
    app.event(Event::Down { x, y, clicks });
    app.event(Event::Up { x, y });
}

fn drag(app: &mut App, from: Rect, dx: i32) {
    let (x, y) = (from.x + from.w / 2, from.y + from.h / 2);
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Move { x: x + dx, y });
    app.event(Event::Up { x: x + dx, y });
}

fn saved(app: &mut App) -> Project {
    app.event(Event::Key { key: Key::Char('s'), shift: false, command: true, repeat: false });
    let text = app.take_requests().into_iter().rev().find_map(|request| if let Request::SaveAs { json, .. } = request { Some(json) } else { None }).expect("wanted a save after Cmd+S");
    Project::from_json(&json::parse(&text).expect("wanted the saved project to parse")).expect("wanted the saved project to read")
}

fn level_ticks(track: &Track) -> Vec<(i32, f32)> {
    let curves = &track.automation.curves;
    let swell = curves.iter().find(|curve| matches!(&curve.aim, Aim::Knob { band, name, .. } if band == "OSC 1" && name == "LEVEL") && curve.lfo.is_none());
    swell.map_or_else(Vec::new, |curve| curve.nodes.iter().map(|node| (node.tick, node.travel)).collect())
}

fn noted_starts(track: &Track) -> Vec<i32> {
    track.clips.iter().filter(|clip| !clip.notes.is_empty()).map(|clip| clip.start).collect()
}

fn play(project: &Project, beats: f64) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, None, &[], &[]);
    audio.play(0.0);
    let total = (beats * FRAMES_PER_BEAT) as usize;
    let mut out = Vec::with_capacity(total + BLOCK);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    while out.len() < total {
        renderer.render(&mut l, &mut r);
        out.extend_from_slice(&l);
    }
    out
}

fn heard_from(out: &[f32], beat: f64) -> f32 {
    let from = (beat * FRAMES_PER_BEAT) as usize;
    out[from..from + (LISTEN_BEATS * FRAMES_PER_BEAT) as usize].iter().fold(0.0f32, |peak, sample| peak.max(sample.abs()))
}

#[test]
fn the_lock_button_sits_beside_automation_and_toggles() {
    let mut plain = open(&Project::new(), true);
    let mut locked_project = Project::new();
    locked_project.lock = true;
    let mut locked = open(&locked_project, true);
    let without = header_washes(&painted(&mut plain));
    let with = header_washes(&painted(&mut locked));
    let grown: Vec<Rect> = with.iter().copied().filter(|rect| !without.contains(rect)).collect();
    assert_eq!(grown.len(), 1, "wanted the lock to light exactly one new header button, got {grown:?} beyond {without:?}");
    let lock = grown[0];
    let automation = *without.iter().filter(|rect| rect.x < lock.x).max_by_key(|rect| rect.x).expect("wanted the automation button lit to the left of the lock");
    let gap = lock.x - (automation.x + automation.w);
    assert!((0..=16).contains(&gap), "wanted the lock button beside automation, got a {gap}px gap after x {} among {without:?}", automation.x + automation.w);
    click(&mut locked, lock.x + lock.w / 2, lock.y + lock.h / 2, 1);
    assert!(!remembered(&mut locked).lock, "wanted a click on the lit lock button to release it");
    click(&mut plain, lock.x + lock.w / 2, lock.y + lock.h / 2, 1);
    assert!(remembered(&mut plain).lock, "wanted a click on the dark lock button to engage it");
}

#[test]
fn a_locked_clip_move_leaves_the_automation_anchored() {
    let mut locked = open(&swelling_project(true), false);
    let mut free = open(&swelling_project(false), false);
    let block = clip_block(&painted(&mut locked));
    painted(&mut free);
    let dx = (f64::from(MOVE_BEATS) * grid::BEAT_W) as i32;
    drag(&mut locked, block, dx);
    drag(&mut free, block, dx);
    let (anchored, moved) = (remembered(&mut locked), remembered(&mut free));
    assert_eq!(noted_starts(&anchored.tracks[0]), vec![MOVE_BEATS * TICKS_PER_BEAT], "wanted the locked move to carry the notes to beat {MOVE_BEATS}");
    assert_eq!(noted_starts(&moved.tracks[0]), vec![MOVE_BEATS * TICKS_PER_BEAT], "wanted the free move to carry the notes to beat {MOVE_BEATS}");
    assert_eq!(level_ticks(&anchored.tracks[0]), vec![(0, 0.0), (3 * TICKS_PER_BEAT, 1.0)], "wanted the locked move to leave the swell on beats 0 and 3");
    assert_eq!(level_ticks(&moved.tracks[0]), vec![(MOVE_BEATS * TICKS_PER_BEAT, 0.0), ((MOVE_BEATS + 3) * TICKS_PER_BEAT, 1.0)], "wanted the free move to carry the swell to beats {MOVE_BEATS} and {}", MOVE_BEATS + 3);
    let onset = f64::from(MOVE_BEATS) + 0.25;
    let (held, swelling) = (play(&anchored, 12.5), play(&moved, 12.5));
    let (loud, quiet) = (heard_from(&held, onset), heard_from(&swelling, onset));
    assert!(loud > 0.05, "wanted the anchored swell already open at beat {onset}, peak {loud}");
    assert!(quiet < loud * 0.5, "wanted the travelling swell still closed at beat {onset}: peak {quiet} against {loud} anchored");
    let late = heard_from(&swelling, f64::from(MOVE_BEATS) + 3.25);
    assert!(late > 0.05, "wanted the travelling swell open by beat {}, peak {late}", f64::from(MOVE_BEATS) + 3.25);
}

#[test]
fn deleting_a_clip_under_lock_keeps_the_automation() {
    let mut locked = open(&swelling_project(true), false);
    let mut free = open(&swelling_project(false), false);
    let block = clip_block(&painted(&mut locked));
    for app in [&mut locked, &mut free] {
        painted(app);
        click(app, block.x + block.w / 2, block.y + block.h / 2, 1);
        app.event(Event::Key { key: Key::Delete, shift: false, command: false, repeat: false });
    }
    let (kept, cleared) = (remembered(&mut locked), remembered(&mut free));
    assert_eq!(noted_starts(&kept.tracks[0]), Vec::<i32>::new(), "wanted the locked delete to take the notes");
    assert_eq!(level_ticks(&kept.tracks[0]), vec![(0, 0.0), (3 * TICKS_PER_BEAT, 1.0)], "wanted the locked delete to leave the swell on beats 0 and 3");
    assert_eq!(level_ticks(&cleared.tracks[0]), Vec::new(), "wanted the free delete to take the swell with the clip");
}

#[test]
fn a_locked_lane_refuses_the_pencil() {
    let mut plain = open(&swelling_project(false), false);
    let block = clip_block(&painted(&mut plain));
    let (x, y) = (block.x + (2.0 * grid::BEAT_W) as i32, block.y + block.h + 60);
    let mut locked = open(&swelling_project(true), true);
    let mut free = open(&swelling_project(false), true);
    for app in [&mut locked, &mut free] {
        painted(app);
        click(app, x, y, 1);
    }
    let refused = level_ticks(&saved(&mut locked).tracks[0]);
    assert_eq!(refused, vec![(0, 0.0), (3 * TICKS_PER_BEAT, 1.0)], "wanted the locked lane to refuse the new node, got {refused:?}");
    let drawn: Vec<i32> = level_ticks(&saved(&mut free).tracks[0]).into_iter().map(|(tick, _)| tick).collect();
    assert_eq!(drawn, vec![0, 2 * TICKS_PER_BEAT, 3 * TICKS_PER_BEAT], "wanted the free lane to take a node on beat 2, got {drawn:?}");
}
