use bp::app::{App, Event};
use bp::arp::Arp;
use bp::keyboard::Key;
use bp::audio::{Audio, Renderer};
use bp::audio::vision::Seer;
use bp::draw::Tone;
use bp::fx::params::Utility;
use bp::fx::{Kind, Params, Slot};
use bp::json;
use bp::project::{Project, Row, Track, VELOCITY};
use support::{AUDIBLE, BLOCK, BLOCKS, HALF, HEIGHT, HUSH_DB, LEVEL, SAMPLE_RATE, WIDTH, core, core_layer, midi, painted, peak, remembered};

const SETTLE: usize = 24;
const ROUNDING: f32 = 1e-5;
const DUCK_FROM_DB: f64 = -40.0;
const DUCK_RATIO: f64 = 20.0;
const DUCKED_BY: f32 = 4.0;
const SEND_A: usize = 0;
const GROUP: usize = 0;
const KICK: usize = 1;
const SNARE: usize = 2;
const BASS: usize = 3;
const GROUP_NUMBER: f64 = 1.0;
const OFF: f64 = 0.0;
const NOTES: [(usize, u8); 3] = [(KICK, 36), (SNARE, 40), (BASS, 48)];
const RAIL: i32 = 45;
const TILE: i32 = 120;
const BORDER: i32 = 8;
const FOLD_MARK: (i32, i32) = (16, 62);
const STEP: usize = 2;
const RAIL_W: i32 = 1900;

fn hushing() -> Row {
    let mut slot = Slot::new(Kind::Utility);
    if let Params::Utility(Utility { gain_db, .. }) = &mut slot.params {
        *gain_db = HUSH_DB;
    }
    Row::Effect(slot)
}

fn ducking(source: f64) -> Row {
    let mut slot = Slot::new(Kind::Sidechain);
    if let Params::Sidechain(sidechain) = &mut slot.params {
        sidechain.signal.source = source;
        sidechain.threshold_db = DUCK_FROM_DB;
        sidechain.ratio = DUCK_RATIO;
    }
    Row::Effect(slot)
}

fn grouped() -> Project {
    let mut project = Project::new();
    project.tracks.clear();
    project.add_track(GROUP, Track::group());
    for track in [KICK, SNARE, BASS] {
        project.add_track(track, Track { rack: vec![midi(Arp::DEFAULT), core(LEVEL)], ..Track::blank() });
    }
    project.tracks[BASS].inside = false;
    project
}

fn ungrouped() -> Project {
    let mut project = grouped();
    project.remove_track(GROUP);
    project
}

fn open(project: &Project, send_solo: Option<usize>, notes: &[(usize, u8)]) -> (Audio, Renderer) {
    let (mut audio, renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, send_solo, &[], &[]);
    for &(track, midi) in notes {
        audio.note_on(track, midi, VELOCITY);
    }
    audio.play(0.0);
    (audio, renderer)
}

fn render(renderer: &mut Renderer, blocks: usize) -> Vec<Vec<f32>> {
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    (0..blocks)
        .map(|_| {
            renderer.render(&mut l, &mut r);
            l.clone()
        })
        .collect()
}

fn run(project: &Project) -> Vec<Vec<f32>> {
    run_soloing(project, None)
}

fn run_soloing(project: &Project, send_solo: Option<usize>) -> Vec<Vec<f32>> {
    let notes: Vec<(usize, u8)> = NOTES.iter().copied().map(|(track, midi)| (track - usize::from(project.tracks.len() == 3), midi)).collect();
    let (_audio, mut renderer) = open(project, send_solo, &notes);
    render(&mut renderer, BLOCKS)
}

fn difference(a: &[Vec<f32>], b: &[Vec<f32>]) -> Vec<Vec<f32>> {
    a.iter().zip(b).map(|(x, y)| x.iter().zip(y).map(|(p, q)| p - q).collect()).collect()
}

fn first_difference(a: &[Vec<f32>], b: &[Vec<f32>]) -> Option<usize> {
    a.iter().zip(b).position(|(x, y)| x.iter().zip(y).any(|(p, q)| (p - q).abs() > ROUNDING))
}

fn silenced(project: &Project, tracks: &[usize]) -> Project {
    let mut quiet = project.clone();
    for &track in tracks {
        quiet.tracks[track].rack = vec![midi(Arp::DEFAULT), Row::Core(core_layer(0.0, bp::core::Params::DEFAULT))];
    }
    quiet
}

#[test]
fn members_are_heard_through_the_group_fader() {
    let mut halved = grouped();
    halved.tracks[GROUP].gain = HALF;
    let heard = run(&halved);
    let mut expected = ungrouped();
    for track in [KICK, SNARE] {
        expected.tracks[track - 1].gain = HALF;
    }
    assert!(peak(&heard[SETTLE..]) > AUDIBLE, "the grouped tracks are silent");
    assert_eq!(first_difference(&heard, &run(&expected)), None, "the group's fader at half is not the members at half, block");
}

#[test]
fn a_group_rack_processes_the_members_together() {
    let mut hushed = grouped();
    hushed.tracks[GROUP].rack = vec![hushing()];
    let heard = run(&hushed);
    let mut expected = ungrouped();
    for track in [KICK, SNARE] {
        expected.tracks[track - 1].rack.push(hushing());
    }
    assert!(peak(&heard[SETTLE..]) > AUDIBLE, "the grouped tracks are silent under the group's rack");
    assert_eq!(first_difference(&heard, &run(&expected)), None, "a Utility at {HUSH_DB} dB on the group is not the same Utility on each member, block");
}

#[test]
fn a_members_send_taps_its_own_fader() {
    let mut sent = grouped();
    sent.tracks[GROUP].gain = 0.0;
    sent.tracks[KICK].sends[SEND_A] = 1.0;
    let heard = run(&silenced(&sent, &[SNARE, BASS]));
    let kick_alone = run(&silenced(&ungrouped(), &[SNARE - 1, BASS - 1]));
    assert!(peak(&kick_alone[SETTLE..]) > AUDIBLE, "the kick is silent on its own");
    assert_eq!(first_difference(&heard, &kick_alone), None, "the kick's send to return A under a group faded to nothing is not the kick alone, block");
    sent.tracks[KICK].sends[SEND_A] = 0.0;
    assert_eq!(peak(&run(&silenced(&sent, &[SNARE, BASS]))), 0.0, "a group faded to nothing still let its member through");
}

#[test]
fn the_groups_send_carries_the_processed_sum() {
    let mut sent = grouped();
    sent.tracks[GROUP].gain = HALF;
    sent.tracks[GROUP].sends[SEND_A] = 1.0;
    let heard = run_soloing(&sent, Some(SEND_A));
    let mut expected = ungrouped();
    for track in [KICK, SNARE] {
        expected.tracks[track - 1].gain = HALF;
    }
    let expected = run(&silenced(&expected, &[BASS - 1]));
    assert!(peak(&heard[SETTLE..]) > AUDIBLE, "return A carries nothing from the group");
    assert_eq!(first_difference(&heard, &expected), None, "return A soloed does not carry the group's sum after its fader, block");
}

#[test]
fn mute_and_solo_climb_the_group() {
    let mut muted = grouped();
    muted.tracks[GROUP].mute = true;
    muted.tracks[KICK].sends[SEND_A] = 1.0;
    assert_eq!(first_difference(&run(&muted), &run(&silenced(&grouped(), &[KICK, SNARE]))), None, "a muted group is not its members silent, sends included, block");
    let mut kick_muted = grouped();
    kick_muted.tracks[KICK].mute = true;
    assert_eq!(first_difference(&run(&kick_muted), &run(&silenced(&grouped(), &[KICK]))), None, "a muted member is not that member silent, block");
    let mut kick_soloed = grouped();
    kick_soloed.tracks[GROUP].gain = HALF;
    kick_soloed.tracks[KICK].solo = true;
    let mut expected = silenced(&ungrouped(), &[SNARE - 1, BASS - 1]);
    expected.tracks[KICK - 1].gain = HALF;
    assert_eq!(first_difference(&run(&kick_soloed), &run(&expected)), None, "a soloed member is not heard alone through its group's fader, block");
    let mut group_soloed = grouped();
    group_soloed.tracks[GROUP].solo = true;
    assert_eq!(first_difference(&run(&group_soloed), &run(&silenced(&grouped(), &[BASS]))), None, "a soloed group is not its members without the rest, block");
    let mut bass_soloed = grouped();
    bass_soloed.tracks[BASS].solo = true;
    assert_eq!(first_difference(&run(&bass_soloed), &run(&silenced(&grouped(), &[KICK, SNARE]))), None, "soloing a track outside the group did not silence the group, block");
    let mut mute_wins = grouped();
    mute_wins.tracks[GROUP].mute = true;
    mute_wins.tracks[KICK].solo = true;
    assert_eq!(peak(&run(&mute_wins)), 0.0, "a soloed member of a muted group was heard");
}

#[test]
fn folding_changes_nothing_audible() {
    let mut folded = grouped();
    folded.tracks[GROUP].folded = true;
    assert_eq!(first_difference(&run(&folded), &run(&grouped())), None, "folding the group changed the sound, block");
}

#[test]
fn a_sidechain_keyed_from_a_group_ducks_with_its_sum() {
    let keyed = |source: f64| {
        let mut project = grouped();
        project.tracks[BASS].rack.push(ducking(source));
        project
    };
    let drums = run(&silenced(&grouped(), &[BASS]));
    let ducked = peak(&difference(&run(&keyed(GROUP_NUMBER)), &drums)[SETTLE..]);
    let unkeyed = peak(&difference(&run(&keyed(OFF)), &drums)[SETTLE..]);
    assert!(unkeyed > AUDIBLE, "the bass is silent on its own");
    assert!(ducked * DUCKED_BY < unkeyed, "keyed from the drum group the bass peaks at {ducked} against {unkeyed} unkeyed");
}

#[test]
fn the_file_writes_group_inside_and_folded_only_when_true() {
    let mut project = grouped();
    project.tracks[GROUP].folded = true;
    let text = project.to_json().write();
    for (key, wanted) in [("\"kind\": \"group\"", 1), ("\"inGroup\": true", 2), ("\"folded\": true", 1)] {
        assert_eq!(text.matches(key).count(), wanted, "wanted {key} written {wanted} times, got {}", text.matches(key).count());
    }
    let back = Project::from_json(&json::parse(&text).expect("wanted the project to parse")).expect("wanted the project to read back");
    let flags = |project: &Project| project.tracks.iter().map(|track| (track.group, track.inside, track.folded)).collect::<Vec<(bool, bool, bool)>>();
    assert_eq!(flags(&back), flags(&project), "wanted the flags back as written");
    assert_eq!((back.group_of(KICK), back.group_of(BASS), back.members_of(GROUP)), (Some(GROUP), None, vec![KICK, SNARE]), "wanted the kick and snare inside the group and the bass outside");
    let plain = Project::new().to_json().write();
    assert!(!plain.contains("\"group\"") && !plain.contains("inGroup") && !plain.contains("folded"), "a project without groups writes a group key");
}

fn session_with(project: &Project, picked: &[usize]) -> App {
    let picks: Vec<String> = picked.iter().map(usize::to_string).collect();
    let session = format!("{{\"project\":{},\"automation\":false,\"keyboard\":false,\"palette\":false,\"picked_tracks\":[{}]}}", project.to_json().write(), picks.join(","));
    let mut app = App::new();
    app.resume(&session);
    app.draw(WIDTH, HEIGHT);
    app.take_requests();
    app
}

fn command_g(app: &mut App) {
    app.event(Event::Key { key: Key::Char('g'), shift: false, command: true, repeat: false });
}

fn click(app: &mut App, x: i32, y: i32) {
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Up { x, y });
    app.draw(WIDTH, HEIGHT);
}

fn rail_buttons(app: &mut App) -> usize {
    let mut found: Vec<(i32, i32, i32, i32)> = Vec::new();
    for x in (0..RAIL_W).step_by(STEP) {
        app.event(Event::Move { x, y: RAIL / 2 });
        if let Some(rect) = app.draw(RAIL_W, HEIGHT).iter().find(|fill| fill.tone == Tone::Ink25 && fill.rect.y < RAIL && fill.rect.h >= 30 && fill.rect.x + fill.rect.w < RAIL_W - BORDER).map(|fill| (fill.rect.x, fill.rect.y, fill.rect.w, fill.rect.h))
            && found.last() != Some(&rect)
        {
            found.push(rect);
        }
    }
    found.len()
}

fn title_lit(app: &mut App, slot: usize) -> bool {
    let (x, y) = (BORDER, RAIL + slot as i32 * (TILE - BORDER) + BORDER);
    painted(app).iter().any(|fill| fill.tone == Tone::Ink100 && fill.rect.x == x && fill.rect.y == y && fill.rect.w == TILE - 2 * BORDER)
}

#[test]
fn command_g_groups_the_picked_tracks_and_again_ungroups() {
    let mut three = Project::new();
    three.add_midi_track(2);
    let mut app = session_with(&three, &[1, 2]);
    command_g(&mut app);
    let grouped = remembered(&mut app);
    let shape: Vec<(bool, bool)> = grouped.tracks.iter().map(|track| (track.group, track.inside)).collect();
    assert_eq!(shape, vec![(true, false), (false, true), (false, true), (false, true)], "wanted a group above the three picked tracks with all three inside, got {shape:?}");
    assert!(grouped.tracks[GROUP].name.starts_with("GROUP"), "wanted the group named as its kind, got {}", grouped.tracks[GROUP].name);
    assert!(grouped.tracks[GROUP].rack.is_empty(), "wanted the group's rack empty, got {} rows", grouped.tracks[GROUP].rack.len());
    command_g(&mut app);
    let ungrouped = remembered(&mut app);
    let shape: Vec<(bool, bool)> = ungrouped.tracks.iter().map(|track| (track.group, track.inside)).collect();
    assert_eq!(shape, vec![(false, false); 3], "wanted the group gone and its members outside again, got {shape:?}");
}

#[test]
fn a_group_button_joins_the_rail_while_tracks_are_picked() {
    let alone = rail_buttons(&mut session_with(&Project::new(), &[]));
    let picked = rail_buttons(&mut session_with(&Project::new(), &[1]));
    assert_eq!(picked, alone + 1, "wanted one more rail button with two tracks picked, got {picked} against {alone}");
    let one_group = rail_buttons(&mut session_with(&grouped(), &[]));
    assert_eq!(one_group, alone + 1, "wanted an UNGROUP button with a group selected, got {one_group} against {alone}");
}

#[test]
fn folding_hides_the_members_from_the_column() {
    let mut drums = grouped();
    drums.remove_track(BASS);
    let mut app = session_with(&drums, &[]);
    let (x, y) = FOLD_MARK;
    click(&mut app, x, y);
    assert!(remembered(&mut app).tracks[GROUP].folded, "wanted the group folded after a press on its mark");
    click(&mut app, TILE + 2 * BORDER, RAIL + (TILE - BORDER) + 2 * BORDER);
    assert!(!title_lit(&mut app, 1) && title_lit(&mut app, 0), "wanted no track in the second slot while folded, only the column's tail, so the group stays selected");
    click(&mut app, x, y);
    assert!(!remembered(&mut app).tracks[GROUP].folded, "wanted the group unfolded after a second press");
    click(&mut app, TILE / 2, RAIL + (TILE - BORDER) + 2 * BORDER);
    assert!(title_lit(&mut app, 1), "wanted the kick back in the second slot and selected by a press");
}
