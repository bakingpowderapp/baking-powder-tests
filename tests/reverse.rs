use bp::audio::Audio;
use bp::audio::vision::{self, Seer};
use bp::face::{Face, Faced, PITCH, READOUT_ROWS};
use bp::fx::params::{Delay, DelayMode, Sidechain, Signal};
use bp::fx::{Kind, Params, Slot};
use bp::project::{Clip, Host, Project, Row, TICKS_PER_BEAT, Track};
use std::time::{Duration, Instant};
use support::{BLOCK, CORE_ROW, SAMPLE_RATE};

const HUSH: f32 = 1e-9;
const REVERSED: usize = CORE_ROW + 1;

fn delay(forward: f64, reverse: f64) -> Row {
    Row::Effect(Slot { params: Params::Delay(Delay { mode: DelayMode::Clean, time_l_ms: 500.0, time_r_ms: 500.0, feedback: 0.0, forward, reverse, ..Delay::DEFAULT }), ..Slot::new(Kind::Delay) })
}

fn chord(start: i32, pitch: u8) -> Clip {
    Clip::chord(start * TICKS_PER_BEAT, String::from("chord"), &[pitch])
}

fn song() -> Project {
    let mut project = Project::new();
    project.tracks[0].clips.push(chord(4, 60));
    project.tracks[0].rack.push(delay(0.0, 1.0));
    project
}

fn recipe(project: &Project) -> vision::Recipe {
    vision::recipe(project, None, &[], |_| true).expect("a reversed row makes a recipe")
}

fn energy(samples: &[f32]) -> f32 {
    samples.iter().map(|x| x * x).sum()
}

fn play_through(project: &Project, frames: usize) -> Vec<f32> {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, None, &[], &[]);
    audio.play(0.0);
    let mut out = Vec::with_capacity(frames);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    while out.len() < frames {
        let n = BLOCK.min(frames - out.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
    }
    out
}

fn span(out: &[f32], from_seconds: f64, to_seconds: f64) -> &[f32] {
    &out[(from_seconds * SAMPLE_RATE) as usize..(to_seconds * SAMPLE_RATE) as usize]
}

#[test]
fn the_levels_survive_json_and_default_to_forward_only() {
    for kind in [Kind::Delay, Kind::Reverb] {
        let row = Row::Effect(Slot::new(kind).blended(0.35, 0.65));
        let json = row.to_json();
        assert_eq!(json.get("forwardLevel").and_then(|value| value.as_f64()), Some(0.35));
        assert_eq!(json.get("reverseLevel").and_then(|value| value.as_f64()), Some(0.65));
        assert!(json.get("direction").is_none());
        assert_eq!(Row::from_json(&json), Some(row));
        let default = Slot::new(kind);
        let band = default.reverse_band().expect("delay and reverb have reverse levels");
        assert_eq!(default.params.travel_of(band, "FORWARD"), Some(1.0));
        assert_eq!(default.params.travel_of(band, "REVERSE"), Some(0.0));
        let plain = Row::Effect(default).to_json();
        assert!(plain.get("forwardLevel").is_none());
        assert!(plain.get("reverseLevel").is_none());
        assert_eq!(Row::from_json(&plain), Some(Row::Effect(default)));
    }
}

#[test]
fn a_song_without_a_reversed_row_needs_no_seer() {
    assert!(vision::recipe(&Project::new(), None, &[], |_| true).is_none());
}

#[test]
fn only_what_feeds_the_reversed_row_is_in_its_recipe() {
    let base = song();
    let same = recipe(&base);

    let mut wet = base.clone();
    wet.tracks[0].rack[REVERSED].set_mix(0.3);
    assert!(recipe(&wet) == same, "a wet drag on the last reversed row renders nothing new");

    for (forward, reverse) in [(1.0, 1.0), (0.25, 0.75), (0.6, 0.2)] {
        let mut mixed = base.clone();
        mixed.tracks[0].rack[REVERSED] = delay(forward, reverse);
        assert!(recipe(&mixed) == same, "levels {forward}/{reverse} should reuse the rendered audio");
    }

    let mut off = base.clone();
    off.tracks[0].rack[REVERSED] = delay(1.0, 0.0);
    assert!(vision::recipe(&off, None, &[], |_| true).is_none(), "reverse at zero should need no render");

    let mut bypassed = base.clone();
    if let Row::Effect(slot) = &mut bypassed.tracks[0].rack[REVERSED] {
        slot.bypass = true;
    }
    assert!(recipe(&bypassed) == same, "a bypass keeps the vision ready");

    let mut below = base.clone();
    below.tracks[0].rack.push(Row::Effect(Slot::new(Kind::Eq)));
    assert!(recipe(&below) == same, "a row below the reversed one is not its business");

    let mut elsewhere = base.clone();
    elsewhere.tracks[1].clips.push(chord(0, 48));
    elsewhere.tracks[0].gain = 0.2;
    elsewhere.tracks[0].sends[0] = 0.5;
    elsewhere.tracks[0].name = String::from("renamed");
    assert!(recipe(&elsewhere) == same, "other tracks' notes, the fader, the sends and the name are silent to it");

    let mut muted = base.clone();
    muted.tracks[1].mute = true;
    assert!(recipe(&muted) != same, "every track's mute is kept, so the render hears solo and mute as the song sets them");

    let mut above = base.clone();
    above.tracks[0].rack.insert(REVERSED, Row::Effect(Slot::new(Kind::Eq)));
    assert!(recipe(&above) != same, "a row above it changes its input");

    let mut notes = base.clone();
    notes.tracks[0].clips.push(chord(8, 62));
    assert!(recipe(&notes) != same, "the notes are its input");

    let mut tempo = base.clone();
    tempo.bpm = 100.0;
    assert!(recipe(&tempo) != same, "the tempo moves every note");
}

#[test]
fn a_reversed_group_listens_to_its_members() {
    let mut project = Project::new();
    project.add_track(0, Track::group());
    project.tracks[1].inside = true;
    project.tracks[2].inside = true;
    project.tracks[1].clips.push(chord(0, 60));
    project.tracks[0].rack.push(delay(0.0, 1.0));
    let same = recipe(&project);

    let mut member = project.clone();
    member.tracks[1].clips.push(chord(4, 64));
    assert!(recipe(&member) != same, "a member's notes reach the group's row");

    let mut muted = project.clone();
    muted.tracks[2].mute = true;
    assert!(recipe(&muted) != same, "a muted member drops out of the group's sum");

    let mut fader = project.clone();
    fader.tracks[1].gain = 0.5;
    assert!(recipe(&fader) != same, "members pour after their faders");
}

#[test]
fn a_key_track_feeds_a_reversed_row_through_a_sidechain_above_it() {
    let key = Row::Effect(Slot { params: Params::Sidechain(Sidechain { signal: Signal { source: 2.0, ..Signal::OFF }, ..Sidechain::DEFAULT }), ..Slot::new(Kind::Sidechain) });
    let mut keyed = song();
    keyed.tracks[0].rack.insert(REVERSED, key);
    let same = recipe(&keyed);
    let mut louder = keyed.clone();
    louder.tracks[1].clips.push(chord(0, 36));
    assert!(recipe(&louder) != same, "the key's notes shape the duck the row hears");

    let mut below = song();
    below.tracks[0].rack.push(key);
    let same = recipe(&below);
    let mut louder = below.clone();
    louder.tracks[1].clips.push(chord(0, 36));
    assert!(recipe(&louder) == same, "a sidechain below the reversed row keys nothing it hears");
}

#[test]
fn a_reversed_return_hears_the_tracks_that_send_to_it_but_not_the_master_rack() {
    let mut project = Project::new();
    project.tracks[0].clips.push(chord(0, 60));
    project.sends[0].rack.push(delay(0.0, 1.0));
    let same = recipe(&project);

    let mut fader = project.clone();
    fader.tracks[1].gain = 0.5;
    assert!(recipe(&fader) == same, "a track sending nothing to the return is not upstream of it");

    let mut send = project.clone();
    send.tracks[0].sends[0] = 0.7;
    assert!(recipe(&send) != same, "the send amount is the return's input");

    let mut both = send.clone();
    both.tracks[1].sends[0] = 0.7;
    let pair = recipe(&both);
    let mut faded = both.clone();
    faded.tracks[1].gain = 0.5;
    assert!(recipe(&faded) != pair, "the fader of a track sending to the return is upstream of it");

    let mut master = project.clone();
    master.master.rack.push(Row::Effect(Slot::new(Kind::Eq)));
    assert!(recipe(&master) == same, "the master's rack is below the return");

    let mut wet = project.clone();
    wet.sends[0].rack[0].set_mix(0.2);
    assert!(recipe(&wet) == same, "the return's own wet is applied live");
}

#[test]
fn the_seer_lays_the_echo_before_the_note() {
    let rendered = Seer::Inline.dispatch(Some(recipe(&song())), SAMPLE_RATE);
    assert_eq!(rendered.len(), 1, "an inline seer answers at once with the one reversed row");
    let rendered = &rendered[0];
    assert_eq!((rendered.host, rendered.index), (Host::Track(0), REVERSED));
    let heard_at = |beat: f64| {
        let (mut l, mut r) = (vec![0.0; 2400], vec![0.0; 2400]);
        rendered.vision.clone().lay(beat, &mut l, &mut r, 1.0);
        energy(&l) + energy(&r)
    };
    assert!(heard_at(3.5) > HUSH, "half a second before the note the echo already sounds");
    assert!(heard_at(0.5) < HUSH, "long before the note there is nothing");
    assert!(heard_at(2.5) < HUSH, "before the echo starts there is nothing");
}

#[test]
fn playback_hears_the_echo_before_the_note_and_forward_does_not() {
    let frames = (SAMPLE_RATE * 2.0) as usize;
    let heard = play_through(&song(), frames);
    assert!(energy(span(&heard, 1.6, 1.9)) > 1e-6, "the reversed delay sounds half a second before the note");
    assert!(energy(span(&heard, 0.0, 1.4)) < HUSH, "and nothing sounds before that");
    let mut forward = song();
    forward.tracks[0].rack[REVERSED] = delay(1.0, 0.0);
    let plain = play_through(&forward, frames);
    assert!(energy(span(&plain, 0.0, 1.9)) < HUSH, "forward, nothing sounds before the note");
    let mut both = song();
    both.tracks[0].rack[REVERSED] = delay(1.0, 1.0);
    let twice = play_through(&both, (SAMPLE_RATE * 3.0) as usize);
    assert!(energy(span(&twice, 1.6, 1.9)) > 1e-6, "both levels at full keep the swell");
    assert!(energy(span(&twice, 2.0, 2.9)) > energy(span(&heard, 1.6, 1.9)), "and the note itself is heard under it");
}

#[test]
fn the_seer_thread_answers_the_newest_job() {
    let mut seer = Seer::spawn();
    let mut later = song();
    later.tracks[0].clips[0] = chord(8, 60);
    assert!(seer.dispatch(Some(recipe(&song())), SAMPLE_RATE).is_empty(), "a thread answers later, not now");
    assert!(seer.dispatch(Some(recipe(&later)), SAMPLE_RATE).is_empty());
    let deadline = Instant::now() + Duration::from_secs(30);
    let rendered = loop {
        let rendered = seer.poll();
        if !rendered.is_empty() {
            break rendered;
        }
        assert!(Instant::now() < deadline, "the seer never answered");
        std::thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(rendered.len(), 1);
    assert_eq!((rendered[0].host, rendered[0].index), (Host::Track(0), REVERSED));
    let heard_at = |beat: f64| {
        let (mut l, mut r) = (vec![0.0; 2400], vec![0.0; 2400]);
        rendered[0].vision.clone().lay(beat, &mut l, &mut r, 1.0);
        energy(&l) + energy(&r)
    };
    assert!(heard_at(7.5) > HUSH, "the thread hands back the newest job's echo, before its note at beat 8");
    assert!(heard_at(3.5) < HUSH, "and not the older job's, whose note was at beat 4");
}

#[test]
fn the_two_level_cells_drag_independently_and_restore_their_defaults() {
    for (kind, band, page, row, forward, reverse) in [(Kind::Reverb, "SPACE", 0, 0, 7, 8), (Kind::Delay, "TIME", 2, 4, 0, 3)] {
        let y = (READOUT_ROWS + row) * PITCH + PITCH / 2;
        let mut face = Face::new();
        face.turn(page);
        let view = face.view();
        let mut slot = Slot::new(kind);
        for (column, param, change, expected) in [(forward, "FORWARD", 75, 0.5), (reverse, "REVERSE", -45, 0.3)] {
            let x = column * PITCH + PITCH / 2;
            assert_eq!(slot.armable_at(x, y, view), Some((band, param)));
            assert!(slot.down(&mut face, x, y, 120.0), "{kind:?} {param} should answer a press");
            assert!(slot.moved(&mut face, x, y + change));
            slot.up(&mut face, x, y + change);
            let actual = slot.params.travel_of(band, param).expect("a level can be automated");
            assert!((actual - expected).abs() < 1e-9, "{kind:?} {param}: wanted {expected}, got {actual}");
        }
        assert_eq!(slot.params.travel_of(band, "FORWARD"), Some(0.5));
        assert_eq!(slot.params.travel_of(band, "REVERSE"), Some(0.3));
        for (column, param) in [(forward, "FORWARD"), (reverse, "REVERSE")] {
            let x = column * PITCH + PITCH / 2;
            let cell = slot.index_at(x, y, view).expect("the level has its own square");
            assert_eq!(slot.restore(cell), Some((band, param)));
        }
        assert_eq!(slot, Slot::new(kind));
    }
}
