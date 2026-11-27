use bp::app::{App, Event};
use bp::arp::Arp;
use bp::audio::Renderer;
use bp::draw::{Fill, Rect, Tone};
use bp::json::Json;
use bp::keyboard::Key;
use bp::project::{Clip, Project};
use support::{BLOCK, CHORD, SAMPLE_RATE, core, midi, painted, session};

const MAX_FRAMES: usize = 4_096;
const HEARD: usize = 24_000;
const A: usize = 0;
const B: usize = 1;
const C: usize = 2;
const F: usize = 5;
const FULL: f32 = 1.0;
const LOW: f32 = 0.25;
const SOFT: f64 = 0.1;
const SEND_PITCH: i32 = 36;
const SWAP_H: i32 = 44;

fn playing(sends: &[(usize, f32)], state: &str) -> (App, Renderer) {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = vec![midi(Arp::DEFAULT), core(SOFT)];
    project.tracks[0].clips.push(Clip::chord(0, String::new(), &CHORD));
    for &(send, level) in sends {
        project.tracks[0].sends[send] = level;
    }
    let session = format!("{{\"project\":{},\"automation\":false,\"keyboard\":false,\"palette\":false{state}}}", project.to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    let mut renderer = app.audio_started(SAMPLE_RATE, MAX_FRAMES, BLOCK as u32);
    app.take_requests();
    app.event(Event::Key { key: Key::Space, shift: false, command: false, repeat: false });
    advance(&mut app, &mut renderer, HEARD);
    (app, renderer)
}

fn advance(app: &mut App, renderer: &mut Renderer, total: usize) {
    let (mut l, mut r) = (vec![0.0; MAX_FRAMES], vec![0.0; MAX_FRAMES]);
    let mut done = 0;
    while done < total {
        let n = (total - done).min(MAX_FRAMES);
        renderer.render(&mut l[..n], &mut r[..n]);
        done += n;
        app.tick(n as f32 / SAMPLE_RATE as f32);
    }
}

fn lighter(fills: &[Fill]) -> Vec<Rect> {
    fills.iter().filter(|fill| fill.tone == Tone::Ink75).map(|fill| fill.rect).collect()
}

fn bars(app: &mut App) -> Vec<Rect> {
    let (mut quiet, _renderer) = playing(&[], "");
    let known = lighter(&painted(&mut quiet));
    let mut found: Vec<Rect> = lighter(&painted(app)).into_iter().filter(|rect| !known.contains(rect)).collect();
    found.sort_by_key(|rect| rect.x);
    found
}

fn sounding(sends: &[(usize, f32)], state: &str) -> Vec<Rect> {
    let (mut app, _renderer) = playing(sends, state);
    bars(&mut app)
}

fn one(found: Vec<Rect>, what: &str) -> Rect {
    let [bar] = found.as_slice() else {
        panic!("wanted one bar for {what}, got {found:?}");
    };
    *bar
}

fn swap_box(bar: Rect) -> (i32, i32) {
    (bar.x + bar.w / 2, bar.y + bar.h + SEND_PITCH + SWAP_H / 2)
}

#[test]
fn only_the_returns_that_sound_draw_a_bar_over_the_masters_meter_each_as_tall_as_it_is_loud() {
    assert!(sounding(&[], "").is_empty(), "wanted no bar with no return sounding");
    let (first, second, third) = (one(sounding(&[(A, FULL)], ""), "return A"), one(sounding(&[(B, FULL)], ""), "return B"), one(sounding(&[(C, FULL)], ""), "return C"));
    assert!(first.x < second.x && second.x < third.x && second.x - first.x == third.x - second.x && second.x - first.x == SEND_PITCH, "wanted each return's bar over its own letter, a letter apart, got {first:?}, {second:?} and {third:?}");
    assert!(first.w > 0 && (first.w, first.h, first.y) == (third.w, third.h, third.y), "wanted returns fed alike to draw bars alike, got {first:?} and {third:?}");
    let both = sounding(&[(A, FULL), (C, LOW)], "");
    let [loud, soft] = both.as_slice() else {
        panic!("wanted a bar for each of the two returns that sound and none for the two that do not, got {both:?}");
    };
    assert_eq!((loud.x, soft.x), (first.x, third.x), "wanted the two bars over A and C");
    assert!(loud.h > soft.h && loud.y + loud.h == soft.y + soft.h, "wanted the louder return's bar taller, both rising from the bottom of the box, got {loud:?} and {soft:?}");
    assert_eq!(*loud, first, "wanted return A's bar the same whatever return C does");
}

#[test]
fn the_swap_box_turns_the_bars_with_the_letters() {
    let second = one(sounding(&[(B, FULL)], ""), "return B");
    let (mut app, mut renderer) = playing(&[(A, FULL), (F, FULL)], "");
    let first = one(bars(&mut app), "return A on the first page, return F being on the next");
    let (x, y) = swap_box(first);
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    assert_eq!(session(&mut app).get("bank").and_then(Json::as_f64), Some(1.0), "wanted a press on the swap box to turn to the next four returns");
    advance(&mut app, &mut renderer, HEARD);
    let turned = one(bars(&mut app), "return F on the second page, return A being on the first");
    assert_eq!((turned.x, turned.w), (second.x, second.w), "wanted return F's bar over the second letter of its page, where return B's stands on the first");
    assert!(sounding(&[(A, FULL)], ",\"bank\":1").is_empty(), "wanted no bar on a page none of whose returns sound");
}

#[test]
fn with_a_return_soloed_only_its_own_bar_shows() {
    let third = one(sounding(&[(C, FULL)], ""), "return C");
    let soloed = one(sounding(&[(A, FULL), (C, FULL)], &format!(",\"send_solo\":{C}")), "the soloed return C, return A sounding too");
    assert_eq!((soloed.x, soloed.w), (third.x, third.w), "wanted the soloed return's bar over its own letter and no bar for the other");
}
