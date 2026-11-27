use bp::app::{App, Event};
use bp::arp::{Arp, MidiRow};
use bp::draw::Rect;
use bp::face::{CELL, H, PITCH};
use bp::fx::{self, Kind, Slot};
use bp::json::{self, Json};
use bp::keyboard::Key;
use bp::project::{Aim, Bus, Clip, Cores, Denial, Host, LaneLfo, Node, Plan, Preset, Project, RACK_ROWS, Row, Sound, TICKS_PER_BEAT, Timing, next_times};
use bp::rack::{Action, Offer, Rack};
use support::{AUDIBLE, CHORD, CORE_ROW, HEIGHT, LEVEL, SAMPLE_RATE, WIDTH, call, channels_apart, core, effect, face_rect, frames, half_wet, midi, painted, peak_of, remembered, song_render, unseeded, worst_delta};

const SECONDS: f64 = 2.0;
const MOST: usize = 4;
const SOFT: u8 = 64;
const FIFTH: i8 = 7;
const DELAY: usize = 2;
const REVERB: usize = 3;
const FEEDBACK: (&str, &str) = ("LOOP", "FEEDBACK");
const SIZE: (&str, &str) = ("SPACE", "SIZE");
const NODE: Node = Node { tick: TICKS_PER_BEAT, travel: 0.5, bend: 0.0 };
const AMOUNT: f64 = 0.4;
const W: i32 = 200;
const RACK_X: i32 = 6;
const RACK_SHORT_OF_FACE: i32 = 10;
const MIDI_SLOT: i32 = 1;
const HEAD: i32 = 2;
const KEPT_DIFFERENTLY: &str = "Kept differently";
const FULL_BUT_ONE: usize = RACK_ROWS - 1;

fn timed(mut row: Row, times: usize) -> Row {
    row.set_times(times);
    row
}

fn song(rack: Vec<Row>) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0].rack = rack;
    project.tracks[0].clips.push(Clip::chord(0, String::new(), &CHORD));
    project
}

fn heard(rack: Vec<Row>) -> [Vec<f32>; 2] {
    song_render(&song(rack), frames(SECONDS))
}

fn apart(got: &[Vec<f32>; 2], wanted: &[Vec<f32>; 2]) -> f32 {
    worst_delta(&got[0], &wanted[0]).max(worst_delta(&got[1], &wanted[1]))
}

fn limiter() -> Row {
    let mut slot = Slot::new(Kind::Utility);
    if let fx::Params::Utility(utility) = &mut slot.params {
        utility.limit = true;
    }
    Row::Effect(slot)
}

fn moving(transpose: i8, times: usize) -> Row {
    Row::Midi(MidiRow { times, transpose, velocity: SOFT, ..MidiRow::DEFAULT })
}

fn open(project: &Project) -> App {
    let session = format!("{{\"project\":{},\"palette\":false,\"keyboard\":false,\"selected\":0,\"open\":{CORE_ROW}}}", project.to_json().write());
    let mut app = App::new();
    assert!(app.resume(&session), "wanted the session to resume");
    app.draw(WIDTH, HEIGHT);
    app.take_requests();
    app
}

fn rack_rect(app: &mut App) -> Rect {
    let face = face_rect(&painted(app));
    Rect::new(RACK_X, face.y, face.x - RACK_SHORT_OF_FACE, H)
}

fn slot(row: usize) -> i32 {
    if row == 0 { MIDI_SLOT } else { HEAD + row as i32 - 1 }
}

fn handle(rack: Rect, row: usize) -> (i32, i32) {
    (rack.x + CELL / 2, rack.y + slot(row) * PITCH + CELL / 2)
}

fn name_of(rack: Rect, row: usize) -> (i32, i32) {
    (rack.x + rack.w / 2, rack.y + slot(row) * PITCH + CELL / 2)
}

fn drag(app: &mut App, (x, y): (i32, i32), (to_x, to_y): (i32, i32)) {
    app.event(Event::Move { x, y });
    app.event(Event::Down { x, y, clicks: 1 });
    app.event(Event::Move { x, y: (y + to_y) / 2 });
    app.event(Event::Move { x: to_x, y: to_y });
    app.event(Event::Up { x: to_x, y: to_y });
    app.draw(WIDTH, HEIGHT);
}

fn press(app: &mut App, at: (i32, i32)) {
    drag(app, at, at);
}

fn shift(app: &mut App, held: bool) {
    app.event(Event::Modifiers { shift: held, command: false });
}

fn rows_of(app: &mut App) -> Vec<(String, usize)> {
    let kind = |row: &Json| String::from(row.get("kind").and_then(Json::as_str).unwrap_or("?"));
    read(app, "tracks/0/rack").as_array().unwrap_or(&[]).iter().map(|row| (kind(row), row.get("times").and_then(Json::as_f64).map_or(1, |times| times as usize))).collect()
}

fn rows(wanted: &[(&str, usize)]) -> Vec<(String, usize)> {
    wanted.iter().map(|(kind, times)| (String::from(*kind), *times)).collect()
}

fn written(app: &mut App, args: &str) -> String {
    let reply = call(app, "write", args);
    assert!(!reply.failed, "wanted the write taken: {args}, got {}", reply.text);
    reply.text
}

fn read(app: &mut App, path: &str) -> Json {
    let reply = call(app, "read", &format!(r#"{{"path":"{path}"}}"#));
    assert!(!reply.failed, "wanted {path} read, got {}", reply.text);
    json::parse(&reply.text).unwrap_or_else(|| panic!("wanted {path} read as JSON, got {}", reply.text))
}

fn offer() -> Offer {
    Offer { cores: Cores::Refused, paste: false }
}

#[test]
fn an_effect_at_two_sounds_as_two_of_itself_one_after_the_other() {
    let stacked = heard(vec![midi(Arp::DEFAULT), core(LEVEL), timed(half_wet(Kind::Delay), 2)]);
    let twice = heard(vec![midi(Arp::DEFAULT), core(LEVEL), half_wet(Kind::Delay), half_wet(Kind::Delay)]);
    assert!(peak_of(&stacked[0]) > AUDIBLE, "wanted the rack heard, got a peak of {}", peak_of(&stacked[0]));
    assert_eq!(channels_apart(&stacked, &twice), [None, None], "wanted a delay at 2 to sound as two delays one under the other, first frames apart");
    let once = heard(vec![midi(Arp::DEFAULT), core(LEVEL), half_wet(Kind::Delay)]);
    let off = apart(&stacked, &once);
    assert!(off > AUDIBLE, "wanted a delay at 2 to differ from the delay at 1, got {off} apart");
}

#[test]
fn a_core_at_three_sounds_as_three_cores_each_with_its_own_random_draws() {
    let project = song(vec![midi(Arp::DEFAULT), timed(core(LEVEL), 3)]);
    let played = project.expanded(|_| &[]).into_owned();
    let rack = &played.tracks[0].rack;
    assert_eq!(unseeded(rack), vec![midi(Arp::DEFAULT), core(LEVEL), core(LEVEL), core(LEVEL)], "wanted the Core at 3 played as three Cores at 1 under the MIDI row");
    let mut seeds: Vec<u64> = rack.iter().filter_map(|row| if let Row::Core(layer) = row { Some(layer.seed) } else { None }).collect();
    seeds.sort_unstable();
    seeds.dedup();
    assert!(seeds.len() == 3 && !seeds.contains(&0), "wanted each copy to take a run of random draws of its own, got seeds {seeds:?}");
    let (three, expanded) = (song_render(&project, frames(SECONDS)), song_render(&played, frames(SECONDS)));
    assert_eq!(channels_apart(&three, &expanded), [None, None], "wanted the Core at 3 to sound as its three copies written out, first frames apart");
    let one = heard(vec![midi(Arp::DEFAULT), core(LEVEL)]);
    assert!(peak_of(&three[0]) > 2.0 * peak_of(&one[0]), "wanted three Cores together well above one, got peaks of {} against {}", peak_of(&three[0]), peak_of(&one[0]));
}

#[test]
fn a_midi_row_at_two_moves_the_notes_twice_and_scales_their_velocity_once() {
    let twice = heard(vec![moving(FIFTH, 2), core(LEVEL)]);
    let once = heard(vec![moving(2 * FIFTH, 1), core(LEVEL)]);
    assert!(peak_of(&twice[0]) > AUDIBLE, "wanted the rack heard, got a peak of {}", peak_of(&twice[0]));
    assert_eq!(channels_apart(&twice, &once), [None, None], "wanted the first MIDI row at 2 to move every note up twice and scale its velocity once, as one row moving them the whole way, first frames apart");
    let unmoved = heard(vec![moving(FIFTH, 1), core(LEVEL)]);
    let off = apart(&twice, &unmoved);
    assert!(off > AUDIBLE, "wanted the row at 2 to differ from the row at 1, got {off} apart");
}

#[test]
fn a_curve_and_an_lfo_reach_every_copy_and_those_on_the_rows_below_move_down_with_them() {
    let mut project = song(vec![midi(Arp::DEFAULT), core(LEVEL), timed(effect(Kind::Delay), 3), effect(Kind::Reverb)]);
    let lanes = &mut project.tracks[0].automation;
    lanes.set_node(&Aim::knob(DELAY, FEEDBACK.0, FEEDBACK.1), None, NODE);
    lanes.set_node(&Aim::Wet(REVERB), None, NODE);
    for aim in [Aim::Wet(DELAY), Aim::knob(REVERB, SIZE.0, SIZE.1)] {
        let mut lfo = LaneLfo::blank(aim);
        lfo.amount = AMOUNT;
        lanes.lfos.push(lfo);
    }
    let played = project.expanded(|_| &[]).into_owned();
    let track = &played.tracks[0];
    assert_eq!(track.rack.iter().map(Row::label).collect::<Vec<_>>(), [MidiRow::LABEL, "Core", Kind::Delay.label(), Kind::Delay.label(), Kind::Delay.label(), Kind::Reverb.label()], "wanted the delay at 3 played as three delays above the reverb");
    assert!(track.rack.iter().all(|row| row.times() == 1), "wanted every played row at 1");
    let mut curves: Vec<Aim> = track.automation.curves.iter().map(|curve| curve.aim.clone()).collect();
    curves.sort_by_key(|aim| aim.row());
    let feedback = |row: usize| Aim::knob(row, FEEDBACK.0, FEEDBACK.1);
    assert_eq!(curves, [feedback(DELAY), feedback(DELAY + 1), feedback(DELAY + 2), Aim::Wet(REVERB + 2)], "wanted the delay's curve on each of its three copies and the reverb's wet curve two rows down with its reverb");
    let mut lfos: Vec<(Aim, f64)> = track.automation.lfos.iter().map(|lfo| (lfo.aim.clone(), lfo.amount)).collect();
    lfos.sort_by_key(|(aim, _)| aim.row());
    assert_eq!(lfos, [(Aim::Wet(DELAY), AMOUNT), (Aim::Wet(DELAY + 1), AMOUNT), (Aim::Wet(DELAY + 2), AMOUNT), (Aim::knob(REVERB + 2, SIZE.0, SIZE.1), AMOUNT)], "wanted the delay's LFO on each of its three copies and the reverb's two rows down");
    assert_eq!(project.tracks[0].automation.curves.len(), 2, "wanted the song itself left with the two curves it was given");
}

#[test]
fn a_rows_times_is_saved_above_one_and_read_back_between_one_and_four() {
    for row in [midi(Arp::DEFAULT), core(LEVEL), effect(Kind::Delay)] {
        let saved = timed(row, 3).to_json();
        assert_eq!(saved.get("times").and_then(Json::as_f64), Some(3.0), "wanted a {} row at 3 saved with times 3, got {}", row.label(), saved.write());
        assert_eq!(Row::from_json(&saved).map(|read| read.times()), Some(3), "wanted the {} row read back at 3", row.label());
        assert!(row.to_json().get("times").is_none(), "wanted a {} row at 1 saved without times, got {}", row.label(), row.to_json().write());
    }
    let read = |text: &str| Row::from_json(&json::parse(text).expect("wanted the row to parse")).map(|row| row.times());
    assert_eq!(read(r#"{"kind":"delay","times":9}"#), Some(MOST), "wanted a times past {MOST} read as {MOST}");
    assert_eq!(read(r#"{"kind":"delay","times":0}"#), Some(1), "wanted a times of 0 read as 1");
    assert_eq!(timed(Row::Bus(Bus::DEFAULT), 3).times(), 1, "wanted a bus row to stay at 1");
}

#[test]
fn an_effects_latency_counts_every_copy() {
    let single = limiter().latency(SAMPLE_RATE);
    assert!(single > 0, "wanted the limiter to look ahead, so the test has a latency to count");
    assert_eq!(timed(limiter(), 3).latency(SAMPLE_RATE), 3 * single, "wanted a limiter at 3 to report three times its latency");
    let planned = |row: Row| Timing::of(&song(vec![midi(Arp::DEFAULT), core(LEVEL), row]), SAMPLE_RATE).output;
    assert_eq!((planned(limiter()), planned(timed(limiter(), 3))), (single, 3 * single), "wanted the song's plan to count every copy's latency");
}

#[test]
fn an_agent_reads_and_writes_a_rows_times_and_the_manual_lists_it() {
    let mut app = open(&song(vec![midi(Arp::DEFAULT), core(LEVEL), effect(Kind::Delay)]));
    for (row, kind) in ["midi", "core", "delay"].into_iter().enumerate() {
        let path = format!("tracks/0/rack/{row}/times");
        written(&mut app, &format!(r#"{{"path":"{path}","value":3}}"#));
        assert_eq!(read(&mut app, &path).as_f64(), Some(3.0), "wanted the {kind} row's times read back as written");
        let manual = call(&mut app, "manual", &format!(r#"{{"row":"{kind}"}}"#));
        let listed = json::parse(&manual.text).and_then(|manual| manual.get("times").and_then(Json::as_str).map(String::from));
        assert!(listed.is_some_and(|line| line.contains("1 to 4")), "wanted the {kind} row's manual to list times, 1 to 4, got {}", manual.text);
    }
    assert_eq!(remembered(&mut app).tracks[0].rack.iter().map(Row::times).collect::<Vec<_>>(), [3, 3, 3], "wanted all three rows at 3 in the song");
    let reply = written(&mut app, r#"{"path":"tracks/0/rack/2/times","value":7}"#);
    let told: Vec<&str> = reply.lines().skip_while(|line| !line.starts_with(KEPT_DIFFERENTLY)).skip(1).take_while(|line| line.starts_with(' ')).map(str::trim).collect();
    assert_eq!(told, ["tracks/0/rack/2/times: sent 7, kept 4"], "wanted a times past 4 told as kept at 4, got {reply}");
}

#[test]
fn a_press_on_a_rows_handle_steps_its_times_round_and_the_first_midi_row_steps_too() {
    let mut app = open(&song(vec![midi(Arp::DEFAULT), core(LEVEL), effect(Kind::Delay), effect(Kind::Reverb)]));
    let rack = rack_rect(&mut app);
    let mut stepped = Vec::new();
    for _ in 0..MOST {
        press(&mut app, handle(rack, DELAY));
        stepped.push(rows_of(&mut app)[DELAY].1);
    }
    assert_eq!(stepped, [2, 3, 4, 1], "wanted each press on the delay's handle let go without moving to step it 2, 3, 4 and back to 1");
    press(&mut app, handle(rack, 0));
    assert_eq!(rows_of(&mut app), rows(&[("midi", 2), ("core", 1), ("delay", 1), ("reverb", 1)]), "wanted a press on the first MIDI row's handle, beneath the tabs, to step that row alone");
}

#[test]
fn a_handle_dragged_still_moves_its_row_and_a_shift_press_only_picks_it() {
    let mut app = open(&song(vec![midi(Arp::DEFAULT), core(LEVEL), effect(Kind::Delay), effect(Kind::Reverb)]));
    let rack = rack_rect(&mut app);
    drag(&mut app, handle(rack, REVERB), handle(rack, DELAY));
    let moved = rows(&[("midi", 1), ("core", 1), ("reverb", 1), ("delay", 1)]);
    assert_eq!(rows_of(&mut app), moved, "wanted the reverb dragged by its handle to land above the delay and no row stepped");
    shift(&mut app, true);
    press(&mut app, handle(rack, REVERB));
    shift(&mut app, false);
    assert_eq!(rows_of(&mut app), moved, "wanted a SHIFT-press on a handle to step no row");
    app.event(Event::Key { key: Key::Delete, shift: false, command: false, repeat: false });
    assert_eq!(rows_of(&mut app), moved[..REVERB], "wanted the SHIFT-pressed delay, now the last row, picked, so Delete removes it");
}

#[test]
fn a_handle_press_asks_for_its_rows_times_and_a_moved_one_does_not() {
    let mut rows = vec![effect(Kind::Eq), effect(Kind::Utility)];
    let mut rack = Rack::new(W);
    let (x, y) = (CELL / 2, PITCH + CELL / 2);
    rack.down(x, y, 1, false, offer(), &mut rows);
    let (_, action) = rack.up(x, y, &rows, offer());
    assert_eq!(action, Some(Action::Times(1)), "wanted a handle pressed and let go in place to ask for its row's times");
    rack.down(x, y, 1, false, offer(), &mut rows);
    rack.moved(x, y - PITCH, offer(), &mut rows, false);
    let (_, action) = rack.up(x, y - PITCH, &rows, offer());
    assert_eq!(action, Some(Action::Move(1, 0)), "wanted a handle let go a row higher to move its row and ask for no times");
}

#[test]
fn the_add_box_leaves_once_forty_rows_are_taken_with_every_copy_counted() {
    let full: Vec<Row> = (0..RACK_ROWS / MOST).map(|_| timed(effect(Kind::Utility), MOST)).collect();
    assert!(!Rack::new(W).search(&full), "wanted no room to add a row to {} rows at {MOST}, {RACK_ROWS} taken", full.len());
    let mut short = full.clone();
    short[0] = timed(effect(Kind::Utility), MOST - 1);
    assert!(Rack::new(W).search(&short), "wanted room to add a row with {} of {RACK_ROWS} taken", RACK_ROWS - 1);
}

fn nearly_full(rest: usize) -> Vec<Row> {
    let fours = (RACK_ROWS - rest) / MOST;
    (0..fours).map(|_| timed(effect(Kind::Utility), MOST)).collect()
}

fn crowded_master(last: usize) -> Project {
    let mut project = song(vec![midi(Arp::DEFAULT), core(LEVEL)]);
    project.master.rack = nearly_full(MOST).into_iter().chain([timed(effect(Kind::Delay), last)]).collect();
    project
}

fn refused(app: &mut App, args: &str) -> String {
    let before = call(app, "read", r#"{"path":"master/rack"}"#).text;
    let reply = call(app, "write", args);
    assert!(reply.failed, "wanted the write refused: {args}, got {}", reply.text);
    assert_eq!(call(app, "read", r#"{"path":"master/rack"}"#).text, before, "wanted the refused write to leave every row and its times as they were");
    reply.text
}

fn command(app: &mut App, letter: char) {
    app.event(Event::Key { key: Key::Char(letter), shift: false, command: true, repeat: false });
    app.draw(WIDTH, HEIGHT);
}

#[test]
fn an_agents_write_that_would_overfill_a_rack_is_refused_whole_and_says_by_how_many_rows() {
    let mut app = open(&crowded_master(MOST - 1));
    let appended = refused(&mut app, r#"{"path":"master/rack/end","value":{"kind":"reverb","times":2}}"#);
    assert_eq!(appended, format!("Nothing written: master/rack would be 1 row more than the {RACK_ROWS} a rack holds, a row counting once for each of its times"), "wanted a row of two added at the end of {FULL_BUT_ONE} refused, one row too many");
    let inserted = refused(&mut app, r#"{"path":"master/rack/0","insert":{"kind":"reverb","times":3}}"#);
    assert!(inserted.contains(&format!("master/rack has no room for the row: it would be 2 rows more than the {RACK_ROWS} a rack holds")), "wanted a row of three inserted into {FULL_BUT_ONE} refused, two rows too many, got {inserted}");
    written(&mut app, r#"{"path":"master/rack/end","value":{"kind":"reverb"}}"#);
    let stepped = refused(&mut app, r#"{"path":"master/rack/10/times","value":2}"#);
    assert!(stepped.contains("master/rack would be 1 row more"), "wanted a times write that overfills the rack refused, got {stepped}");
    let whole: Vec<String> = (0..=RACK_ROWS / MOST).map(|_| format!(r#"{{"kind":"utility","times":{MOST}}}"#)).collect();
    let rack = refused(&mut app, &format!(r#"{{"path":"master/rack","value":[{}]}}"#, whole.join(",")));
    assert!(rack.contains(&format!("master/rack would be {MOST} rows more")), "wanted a whole rack written past {RACK_ROWS} refused, got {rack}");
}

#[test]
fn a_paste_that_will_not_fit_a_rack_is_refused_whole_and_one_that_fits_lands() {
    let rack: Vec<Row> = [midi(Arp::DEFAULT), core(LEVEL)].into_iter().chain(nearly_full(2 * MOST)).chain([effect(Kind::Delay), effect(Kind::Reverb)]).collect();
    let before = rows(&rack.iter().map(|row| (match row.label() { "MIDI" => "midi", "Core" => "core", "Utility" => "utility", "Delay" => "delay", _ => "reverb" }, row.times())).collect::<Vec<_>>());
    let mut app = open(&song(rack));
    command(&mut app, 'a');
    command(&mut app, 'c');
    command(&mut app, 'v');
    assert_eq!(rows_of(&mut app), before, "wanted a paste of the whole rack onto itself, far too many rows, to add none and lower no row's times");
    let screen = rack_rect(&mut app);
    press(&mut app, name_of(screen, before.len() - 1));
    command(&mut app, 'c');
    let free = RACK_ROWS - before.iter().map(|(_, times)| times).sum::<usize>();
    assert_eq!(free, MOST, "wanted the rack {MOST} rows short of full, so the test pastes into the last of them");
    for pasted in 1..=free + 2 {
        command(&mut app, 'v');
        assert_eq!(rows_of(&mut app).len(), before.len() + pasted.min(free), "wanted the copied reverb pasted while a row is free and refused once the rack holds {RACK_ROWS}, at paste {pasted}");
    }
    assert_eq!(rows_of(&mut app)[..before.len()], before[..], "wanted every row that was there left with its times");
}

#[test]
fn a_rows_number_that_cannot_go_up_returns_to_one_and_no_other_row_gives_way() {
    let full: Vec<Row> = nearly_full(MOST).into_iter().chain([timed(effect(Kind::Delay), 2), timed(effect(Kind::Reverb), 2)]).collect();
    let delay = full.len() - 2;
    assert_eq!(full.iter().map(Row::times).sum::<usize>(), RACK_ROWS, "wanted the rack full, so the test steps a row with nowhere to go");
    assert_eq!(next_times(&full, delay), (1, true), "wanted a row at 2 in a full rack to go back to 1, stopped for want of a row");
    assert_eq!(next_times(&full, 0), (1, false), "wanted a row at {MOST} to go round to 1 as it always does");
    assert_eq!(next_times(&full[..=delay], delay), (3, false), "wanted a row at 2 to step to 3 where the rack has room");
    assert_eq!(next_times(&[Row::Bus(Bus::DEFAULT)], 0), (1, false), "wanted a bus row to stay at 1");
    let rack: Vec<Row> = [midi(Arp::DEFAULT), core(LEVEL)].into_iter().chain(nearly_full(MOST)).chain([timed(effect(Kind::Delay), 2)]).collect();
    let mut app = open(&song(rack));
    let before = rows_of(&mut app);
    let screen = rack_rect(&mut app);
    press(&mut app, handle(screen, before.len() - 1));
    let after = rows_of(&mut app);
    assert_eq!(after[before.len() - 1], (String::from("delay"), 1), "wanted a press on the handle of a row at 2 in a full rack to return it to 1");
    assert_eq!(after[..before.len() - 1], before[..before.len() - 1], "wanted every other row left at its times");
}

#[test]
fn a_preset_that_will_not_fit_is_refused_with_the_rows_it_lacks_and_one_pressed_onto_a_row_counts_the_row_it_frees() {
    let project = crowded_master(2);
    let taken = RACK_ROWS - 2;
    assert_eq!(project.master.rack.iter().map(Row::times).sum::<usize>(), taken, "wanted the master holding {taken} of its {RACK_ROWS} rows");
    let row = Preset::Row(timed(effect(Kind::Reverb), MOST), Vec::new());
    assert_eq!(project.plan(Host::Master, &row, None, false), Err(Denial::NoRoom(2)), "wanted a row of {MOST} joining a rack with two free refused, two rows short");
    let last = project.master.rack.len() - 1;
    assert_eq!(project.plan(Host::Master, &row, Some(last), false), Ok(Plan::Joins { stripped: false }), "wanted the same row pressed onto a row of 2 to fit, that row's two freed");
    let fits = Preset::Row(timed(effect(Kind::Reverb), 2), Vec::new());
    assert_eq!(project.plan(Host::Master, &fits, None, false), Ok(Plan::Joins { stripped: false }), "wanted a row of 2 to join a rack with two free");
    let sound = Preset::Rack(Sound { rack: [timed(core(LEVEL), MOST)].into_iter().chain(nearly_full(0)).collect(), lfos: Vec::new() });
    assert_eq!(project.plan(Host::Track(0), &sound, None, false), Err(Denial::NoRoom(MOST + 1)), "wanted a sound of a Core at {MOST} and {RACK_ROWS} rows of effects refused, the track's own MIDI row counted with them");
    assert_eq!(Denial::NoRoom(1).reason(), "NO ROOM", "wanted the refusal read as NO ROOM");
}

fn reread(preset: &Preset) -> Preset {
    Preset::from_json(&json::parse(&preset.to_json().write()).expect("wanted the preset to parse")).expect("wanted the preset to read")
}

#[test]
fn a_preset_recalls_the_times_its_rows_were_saved_at() {
    let row = reread(&Preset::Row(timed(effect(Kind::Delay), 3), Vec::new()));
    let mut project = song(vec![midi(Arp::DEFAULT), core(LEVEL), effect(Kind::Reverb)]);
    let host = Host::Track(0);
    let plan = project.plan(host, &row, None, false).expect("wanted the row preset to join the rack");
    project.land(host, row, plan, None, ("Thrice", ""));
    assert_eq!(project.tracks[0].rack.iter().map(|row| (row.label(), row.times())).collect::<Vec<_>>(), [("MIDI", 1), ("Core", 1), ("Reverb", 1), ("Delay", 3)], "wanted the row preset landed at the 3 it was saved at, the rows that were there left at theirs");
    let sound = reread(&Preset::Rack(Sound { rack: vec![timed(midi(Arp::DEFAULT), 2), timed(core(LEVEL), 2), timed(effect(Kind::Delay), MOST)], lfos: Vec::new() }));
    let plan = project.plan(host, &sound, None, false).expect("wanted the sound to replace the track's");
    project.land(host, sound, plan, None, ("Stacked", ""));
    assert_eq!(project.tracks[0].rack.iter().map(Row::times).collect::<Vec<_>>(), [2, 2, MOST], "wanted the sound's MIDI row, Core and delay landed at the times they were saved at");
    let pressed = reread(&Preset::Row(timed(effect(Kind::Delay), 2), Vec::new()));
    let plan = project.plan(host, &pressed, Some(2), false).expect("wanted the row preset pressed onto the delay's row");
    project.land(host, pressed, plan, Some(2), ("Twice", ""));
    assert_eq!(project.tracks[0].rack[2].times(), 2, "wanted a row preset pressed onto a row to bring its own times in place of that row's");
}
