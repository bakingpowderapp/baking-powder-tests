use bp::app::{App, Request};
use bp::arp::Articulation;
use bp::fx::{Kind, Slot};
use bp::json::{self, Json};
use bp::project::{Aim, Clip, Curve, Node, Note, Project, Row, TICKS_PER_BEAT, Track, VELOCITY, beats, gain_to_position, ticks};

const CLOSE: f64 = 1e-5;
const SIXTEENTH: i32 = TICKS_PER_BEAT / 4;
const EDIT: i32 = 4 * TICKS_PER_BEAT;
const SPAN: i32 = 4 * TICKS_PER_BEAT;
const FADE_END: i32 = 8 * TICKS_PER_BEAT;
const CLIP_END: i32 = 12 * TICKS_PER_BEAT;
const READ_UNTIL: i32 = 20 * TICKS_PER_BEAT;
const PLAYHEAD: i32 = 10 * TICKS_PER_BEAT;
const LOOP: (i32, i32) = (8 * TICKS_PER_BEAT, 12 * TICKS_PER_BEAT);
const COPY_FROM: i32 = 0;
const COPY_TO: i32 = 8 * TICKS_PER_BEAT;
const COPIES: i32 = 2;
const SILENCE: f32 = 0.0;
const UNITY: f32 = 1.0;
const BEFORE: (u8, f32, f32) = (60, 1.0, 1.0);
const ACROSS: (u8, f32, f32) = (64, 3.5, 1.0);
const INSIDE: (u8, f32, f32) = (67, 5.0, 1.0);
const PAST: (u8, f32, f32) = (72, 9.0, 1.0);
const AT: usize = 1;
const DRUMS: usize = 0;
const KICK: usize = 1;
const CHORDS: usize = 2;
const RETURN: usize = 0;

type Heard = (u8, i32, i32);

fn note((pitch, start, length): (u8, f32, f32)) -> Note {
    Note { pitch, start, length, velocity: VELOCITY, articulation: Articulation::PLAIN }
}

fn node(tick: i32, gain: f32) -> Node {
    Node { tick, travel: gain_to_position(gain), bend: 0.0 }
}

fn arrangement(lock: bool) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.lock = lock;
    project.tracks[0].clips.push(Clip { start: 0, length: CLIP_END, label: String::new(), notes: [BEFORE, ACROSS, INSIDE, PAST].map(note).to_vec(), file: None });
    project.tracks[0].automation.curves.push(Curve { aim: Aim::Gain, lfo: None, nodes: vec![node(0, SILENCE), node(FADE_END, UNITY)] });
    project
}

fn keyed_by(source: usize) -> Row {
    let mut slot = Slot::new(Kind::Sidechain);
    slot.wire(Some(source), false);
    Row::Effect(slot)
}

fn named(name: &str) -> Track {
    Track { name: String::from(name), ..Track::blank() }
}

fn grouped() -> Project {
    let mut project = Project::new();
    let mut bass = named("Bass");
    bass.rack.push(keyed_by(KICK));
    project.tracks = vec![Track { name: String::from("Drums"), ..Track::group() }, Track { inside: true, ..named("Kick") }, named("Chords"), Track { follows: Some(CHORDS), ..named("Arp") }, bass];
    project.sends[RETURN].rack = vec![keyed_by(DRUMS)];
    project
}

fn ungrouped() -> Project {
    let mut project = grouped();
    project.remove_track(DRUMS);
    project
}

fn open(project: &Project) -> App {
    let session = format!(r#"{{"project":{},"palette":false,"keyboard":false,"playheadBeat":{},"loopStartBeat":{},"loopEndBeat":{}}}"#, project.to_json().write(), beats(PLAYHEAD), beats(LOOP.0), beats(LOOP.1));
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.take_requests();
    app
}

fn written(project: &Project, args: &str) -> (Project, Json) {
    let mut app = open(project);
    let reply = app.serve_call("write", &json::parse(args).expect("wanted the arguments to parse")).expect("wanted the write served, no drag holding it");
    assert!(!reply.failed, "wanted {args} written, got {}", reply.text);
    let text = app.take_requests().into_iter().rev().find_map(|request| if let Request::Remember { json } = request { Some(json) } else { None }).expect("wanted the session remembered after the write");
    let session = json::parse(&text).expect("wanted the remembered session to parse");
    let project = Project::from_json(session.get("project").expect("wanted the remembered session to hold the project")).expect("wanted the remembered project to read");
    (project, session)
}


fn refused(project: &Project, args: &str) -> String {
    let mut app = open(project);
    let before = app.serve_call("read", &json::parse(r#"{"path":"tracks"}"#).expect("wanted the read to parse")).map(|reply| reply.text);
    let reply = app.serve_call("write", &json::parse(args).expect("wanted the arguments to parse")).expect("wanted the write served, no drag holding it");
    assert!(reply.failed, "wanted {args} refused, got {}", reply.text);
    let after = app.serve_call("read", &json::parse(r#"{"path":"tracks"}"#).expect("wanted the read to parse")).map(|reply| reply.text);
    assert_eq!(after, before, "wanted the refused write to leave every track as it was");
    reply.text
}

fn two_members() -> Project {
    let mut project = grouped();
    project.tracks[CHORDS].inside = true;
    project
}

fn group_at(at: usize) -> String {
    format!(r#"{{"path":"tracks/{at}","insert":{{"name":"Keys","kind":"group"}}}}"#)
}
fn spanning(verb: &str) -> String {
    format!(r#"{{"{verb}":{},"fromBeat":{}}}"#, beats(SPAN), beats(EDIT))
}

fn copying() -> String {
    format!(r#"{{"copy":{},"fromBeat":{},"toBeat":{},"copies":{COPIES}}}"#, beats(SPAN), beats(COPY_FROM), beats(COPY_TO))
}

fn pad_following(source: usize) -> String {
    format!(r#"{{"path":"tracks/{AT}","insert":{{"name":"Pad","kind":"midi","follows":{source}}}}}"#)
}

fn marks(session: &Json) -> [Option<f64>; 3] {
    ["playheadBeat", "loopStartBeat", "loopEndBeat"].map(|key| session.get(key).and_then(Json::as_f64))
}

fn moved_marks(by: i32) -> [Option<f64>; 3] {
    [PLAYHEAD, LOOP.0, LOOP.1].map(|tick| Some(beats(tick + by)))
}

fn spans(project: &Project) -> Vec<(i32, i32)> {
    project.tracks[0].clips.iter().map(|clip| (clip.start, clip.end())).collect()
}

fn heard(project: &Project) -> Vec<Heard> {
    let placed = project.tracks[0].clips.iter().flat_map(|clip| {
        clip.notes.iter().map(move |note| {
            let start = clip.start + ticks(f64::from(note.start));
            (note.pitch, start, (start + ticks(f64::from(note.length))).min(clip.end()))
        })
    });
    let mut notes: Vec<Heard> = placed.filter(|(_, start, end)| end > start).collect();
    notes.sort_unstable();
    notes
}

fn part(notes: &[Heard], (from, until): (i32, i32), by: i32) -> Vec<Heard> {
    notes.iter().filter(|(_, start, end)| *start < until && *end > from).map(|&(pitch, start, end)| (pitch, start.max(from) + by, end.min(until) + by)).collect()
}

fn joined(parts: impl IntoIterator<Item = Vec<Heard>>) -> Vec<Heard> {
    let mut notes: Vec<Heard> = parts.into_iter().flatten().collect();
    notes.sort_unstable();
    notes
}

fn copies() -> impl Iterator<Item = i32> {
    (0..COPIES).map(|copy| COPY_TO + copy * SPAN)
}

fn copied_spans() -> Vec<(i32, i32)> {
    std::iter::once((0, COPY_TO)).chain(copies().map(|at| (at, at + SPAN))).collect()
}

fn fade(project: &Project) -> Curve {
    project.tracks[0].automation.curves.iter().find(|curve| curve.aim == Aim::Gain && curve.lfo.is_none()).cloned().expect("wanted the gain curve kept")
}

fn reads(curve: &Curve, tick: i32) -> f64 {
    curve.at(beats(tick))
}

fn mid_sixteenths((from, until): (i32, i32)) -> impl Iterator<Item = i32> {
    (from..until).step_by(SIXTEENTH as usize).map(|tick| tick + SIXTEENTH / 2)
}

fn quarter_way_up(curve: &Curve, what: &str) {
    let (low, high) = (f64::from(gain_to_position(SILENCE)), f64::from(gain_to_position(UNITY)));
    let (wanted, got) = (low + (high - low) / 4.0, reads(curve, FADE_END / 4));
    assert!((wanted - got).abs() <= CLOSE, "wanted the fade a quarter of the way up at beat {}, {wanted:.4}, {what}, got {got:.4}", beats(FADE_END / 4));
}

fn reads_as(curve: &Curve, original: &Curve, span: (i32, i32), moved: i32, what: &str) {
    for tick in mid_sixteenths(span) {
        let (wanted, got) = (reads(original, tick - moved), reads(curve, tick));
        assert!((wanted - got).abs() <= CLOSE, "wanted {what} at beat {:.3} to read {wanted:.4}, as the fade did at beat {:.3}, got {got:.4}", beats(tick), beats(tick - moved));
    }
}

fn held(curve: &Curve, original: &Curve, (from, until): (i32, i32), what: &str) {
    let (last, edge) = (reads(original, from - 1), reads(original, from));
    let level = last.min(edge) - CLOSE..=last.max(edge) + CLOSE;
    for tick in mid_sixteenths((from, until)) {
        let got = reads(curve, tick);
        assert!(level.contains(&got), "wanted {what} held at {edge:.4}, where the fade stood at beat {}, at beat {:.3}, got {got:.4}", beats(from), beats(tick));
    }
}

fn nodes_of(curve: &Curve) -> Vec<(i32, f32)> {
    curve.nodes.iter().map(|node| (node.tick, node.travel)).collect()
}

fn names(project: &Project) -> Vec<&str> {
    project.tracks.iter().map(|track| track.name.as_str()).collect()
}

fn index_of(project: &Project, name: &str) -> usize {
    project.tracks.iter().position(|track| track.name == name).unwrap_or_else(|| panic!("wanted a track named {name}"))
}

fn name_at(project: &Project, index: Option<usize>) -> Option<&str> {
    index.and_then(|index| project.tracks.get(index)).map(|track| track.name.as_str())
}

fn followed<'a>(project: &'a Project, name: &str) -> Option<&'a str> {
    name_at(project, project.tracks[index_of(project, name)].follows)
}

fn key_of(rack: &[Row]) -> Option<usize> {
    rack.iter().find_map(|row| if let Row::Effect(slot) = row { slot.source().map(|(track, _)| track) } else { None })
}

fn keying<'a>(project: &'a Project, name: &str) -> Option<&'a str> {
    name_at(project, key_of(&project.tracks[index_of(project, name)].rack))
}

#[test]
fn an_insert_splits_the_clip_and_moves_its_later_half_and_the_marks_four_beats_on() {
    let before = arrangement(false);
    let (project, session) = written(&before, &spanning("insert"));
    assert_eq!(spans(&project), vec![(0, EDIT), (EDIT + SPAN, CLIP_END + SPAN)], "wanted the clip split at beat {} and its later half moved {} beats on", beats(EDIT), beats(SPAN));
    let notes = heard(&before);
    assert_eq!(heard(&project), joined([part(&notes, (0, EDIT), 0), part(&notes, (EDIT, CLIP_END), SPAN)]), "wanted the notes before beat {} heard as they were and the rest {} beats later", beats(EDIT), beats(SPAN));
    assert_eq!(marks(&session), moved_marks(SPAN), "wanted the playhead and loop moved {} beats on", beats(SPAN));
}

#[test]
fn an_insert_keeps_the_fade_before_it_holds_it_across_the_new_beats_and_moves_the_rest_on() {
    let before = arrangement(false);
    let (project, _) = written(&before, &spanning("insert"));
    let (original, curve) = (fade(&before), fade(&project));
    quarter_way_up(&curve, "after the insert as before it");
    reads_as(&curve, &original, (0, EDIT), 0, "the fade before the inserted beats");
    held(&curve, &original, (EDIT, EDIT + SPAN), "the fade across the inserted beats");
    reads_as(&curve, &original, (EDIT + SPAN, READ_UNTIL), SPAN, "the fade after the inserted beats");
}

#[test]
fn a_remove_cuts_the_span_from_the_clip_trims_its_notes_and_closes_the_gap() {
    let before = arrangement(false);
    let (project, session) = written(&before, &spanning("remove"));
    assert_eq!(spans(&project), vec![(0, EDIT), (EDIT, CLIP_END - SPAN)], "wanted beats {} to {} cut from the clip and its tail brought back to close the gap", beats(EDIT), beats(EDIT + SPAN));
    let notes = heard(&before);
    assert_eq!(heard(&project), joined([part(&notes, (0, EDIT), 0), part(&notes, (EDIT + SPAN, CLIP_END), -SPAN)]), "wanted the notes trimmed at beat {}, those inside the span gone and the rest heard {} beats sooner", beats(EDIT), beats(SPAN));
    assert_eq!(marks(&session), moved_marks(-SPAN), "wanted the playhead and loop brought {} beats back", beats(SPAN));
}

#[test]
fn a_remove_keeps_the_fade_before_it_and_brings_the_rest_back() {
    let before = arrangement(false);
    let (project, _) = written(&before, &spanning("remove"));
    let (original, curve) = (fade(&before), fade(&project));
    quarter_way_up(&curve, "after the remove as before it");
    reads_as(&curve, &original, (0, EDIT), 0, "the fade before the removed beats");
    reads_as(&curve, &original, (EDIT, READ_UNTIL), -SPAN, "the fade after the removed beats");
}

#[test]
fn a_copy_lays_two_copies_over_what_was_there_carrying_the_fade() {
    let before = arrangement(false);
    let (project, _) = written(&before, &copying());
    assert_eq!(spans(&project), copied_spans(), "wanted the clip cut at beat {} and {COPIES} copies of beats {} to {} laid over the rest", beats(COPY_TO), beats(COPY_FROM), beats(COPY_FROM + SPAN));
    let notes = heard(&before);
    let wanted = joined(std::iter::once(part(&notes, (0, COPY_TO), 0)).chain(copies().map(|at| part(&notes, (COPY_FROM, COPY_FROM + SPAN), at - COPY_FROM))));
    assert_eq!(heard(&project), wanted, "wanted the notes before beat {} kept and each copy to play beats {} to {} again", beats(COPY_TO), beats(COPY_FROM), beats(COPY_FROM + SPAN));
    let (original, curve) = (fade(&before), fade(&project));
    reads_as(&curve, &original, (0, COPY_TO), 0, "the fade before the copies");
    for at in copies() {
        reads_as(&curve, &original, (at, at + SPAN), at - COPY_FROM, &format!("the fade under the copy at beat {}", beats(at)));
    }
    reads_as(&curve, &original, (COPY_TO + COPIES * SPAN, READ_UNTIL), 0, "the fade past the copies");
}

#[test]
fn a_locked_copy_lays_the_clips_and_moves_no_node() {
    let locked = arrangement(true);
    let (project, _) = written(&locked, &copying());
    assert_eq!(spans(&project), copied_spans(), "wanted the copies laid under automationLock as without it");
    assert_eq!(nodes_of(&fade(&project)), nodes_of(&fade(&locked)), "wanted automationLock to keep every node where it was");
}

#[test]
fn a_track_inserted_under_a_group_joins_it_and_every_reference_still_names_its_track() {
    let before = grouped();
    let (project, _) = written(&before, &pad_following(index_of(&before, "Chords")));
    assert_eq!(names(&project), ["Drums", "Pad", "Kick", "Chords", "Arp", "Bass"], "wanted Pad in at tracks/{AT}, the tracks from there moved down one");
    let members: Vec<&str> = project.members_of(DRUMS).into_iter().map(|member| project.tracks[member].name.as_str()).collect();
    assert_eq!(members, ["Pad", "Kick"], "wanted Pad to join Drums, the group above it, beside Kick");
    assert_eq!(followed(&project, "Pad"), Some("Chords"), "wanted Pad's follows, written as the tracks stood before it, to name Chords");
    assert_eq!(followed(&project, "Arp"), Some("Chords"), "wanted Arp to follow Chords still");
    assert_eq!(keying(&project, "Bass"), Some("Kick"), "wanted Bass's Sidechain keyed from Kick still");
    assert_eq!(name_at(&project, key_of(&project.sends[RETURN].rack)), Some("Drums"), "wanted the return's Sidechain keyed from Drums still");
}

#[test]
fn a_track_inserted_under_a_track_in_no_group_stays_out_of_groups() {
    let before = ungrouped();
    let (project, _) = written(&before, &pad_following(index_of(&before, "Kick")));
    assert_eq!(names(&project), ["Kick", "Pad", "Chords", "Arp", "Bass"], "wanted Pad in at tracks/{AT}, the tracks from there moved down one");
    assert!(!project.tracks[AT].inside, "wanted Pad outside any group, as Kick above it is, got it inside one");
    assert_eq!(followed(&project, "Pad"), Some("Kick"), "wanted Pad's follows, pointing above it, left on Kick");
    assert_eq!(followed(&project, "Arp"), Some("Chords"), "wanted Arp to follow Chords still");
    assert_eq!(keying(&project, "Bass"), Some("Kick"), "wanted Bass's Sidechain keyed from Kick still");
}

#[test]
fn a_group_inserted_among_another_groups_members_or_right_under_its_head_is_refused() {
    let before = two_members();
    for (at, where_) in [(KICK, "right under the group's head"), (CHORDS, "between its two members")] {
        let reply = refused(&before, &group_at(at));
        assert!(reply.contains(&format!("tracks/{at} lies among the members of another group")), "wanted a group inserted {where_} refused, got {reply}");
    }
    let (after, _) = written(&before, &group_at(CHORDS + 1));
    assert_eq!(names(&after), ["Drums", "Kick", "Chords", "Keys", "Arp", "Bass"], "wanted a group inserted after the last member to land there");
    let members: Vec<&str> = after.members_of(DRUMS).into_iter().map(|member| after.tracks[member].name.as_str()).collect();
    assert_eq!(members, ["Kick", "Chords"], "wanted Drums to keep both its members and take no other");
    assert!(after.members_of(index_of(&after, "Keys")).is_empty(), "wanted the new group to take none of them");
    let (above, _) = written(&before, &group_at(DRUMS));
    assert_eq!(names(&above), ["Keys", "Drums", "Kick", "Chords", "Arp", "Bass"], "wanted a group inserted above the other group's head to land there");
    assert!(above.members_of(0).is_empty() && above.members_of(1).len() == 2, "wanted the new group empty and Drums whole below it");
}

#[test]
fn a_clip_path_in_an_edits_list_names_the_clip_as_the_list_stands_after_clips_written_out_of_order() {
    let mut before = Project::new();
    before.tracks.truncate(1);
    let clips = r#"{"path":"tracks/0/clips","value":[{"startBeat":8,"lengthBeats":4,"name":"late","notes":[[67,0,1]]},{"startBeat":0,"lengthBeats":4,"name":"early","notes":[[60,0,1]]}]}"#;
    let (after, _) = written(&before, &format!(r#"{{"edits":[{clips},{{"path":"tracks/0/clips/0","duplicate":true}}]}}"#));
    let laid: Vec<(&str, i32)> = after.tracks[0].clips.iter().map(|clip| (clip.label.as_str(), clip.start)).collect();
    assert_eq!(laid, [("early", 0), ("late", 8 * TICKS_PER_BEAT), ("late", 12 * TICKS_PER_BEAT)], "wanted the clip written first in the list, the late one, played again after itself");
}
