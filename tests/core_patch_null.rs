use bp::arp::{Arp, Join, Lane, Pattern};
use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::core::params::{Lfo, Modulation, Osc};
use bp::core::{FilterMode, Params, Shape, Target};
use bp::factory::PRESETS;
use bp::json;
use bp::project::{Preset, Project, Row, Track, VELOCITY};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{iter, panic, thread};
use support::{BLOCK, CHORD, SAMPLE_RATE, core_layer, frames, struck};

const HELD_SECONDS: f64 = 2.0;
const TAIL_SECONDS: f64 = 1.0;
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
const KEY_HZ: f64 = 261.6256;
const RESONANCE: f64 = 0.9;
const DETUNE_CENTS: f64 = 10.0;
const FIFTH: f64 = 7.0;
const LINEAR_FM: f64 = 0.3;
const TWO_OCTAVES: f64 = 24.0;
const CHAOS_HZ: f64 = 2.0;
const CHAOS_AMOUNT: f64 = 0.3;
const TWO_VOICES: usize = 2;
const STEP_MS: f64 = 250.0;
const FULL: i8 = 100;

const BANK_BEFORE_THE_CORE_PATCHES: [(&str, u64); 72] = [
    ("Instruments/Basses/Gas.json", 0x9a35f3e171f5838d),
    ("Instruments/Basses/Growl.json", 0x1c08a56cfbc2c533),
    ("Instruments/Basses/Moss.json", 0x78c3aaeeaf9173df),
    ("Instruments/Chords/Airy.json", 0xae489951979e5da7),
    ("Instruments/Chords/Beacon.json", 0xc8ee62d05b072c2a),
    ("Instruments/Chords/Haze.json", 0x163bd3bad15076f6),
    ("Instruments/Chords/Rush.json", 0xa11fe6dcd6583f4a),
    ("Instruments/Chords/Summit.json", 0xd089e7838c76baf2),
    ("Instruments/Plucks/Vista.json", 0xf8079e8a8f79016b),
    ("Instruments/Keys/Chapel.json", 0xf0811ab6277a2428),
    ("Instruments/Keys/Lumen.json", 0x6de44d01beaafaad),
    ("Instruments/Keys/Organ.json", 0x5373acee008d5036),
    ("Instruments/Leads/Ascent.json", 0x148302fbdac51b8a),
    ("Instruments/Leads/Banshee.json", 0x218ca633c39c91f1),
    ("Instruments/Leads/Mural.json", 0x2279cf133933eb21),
    ("Instruments/Leads/Relay.json", 0x1ba07e9792b61254),
    ("Instruments/Leads/Siren.json", 0xa655185b0dec1167),
    ("Instruments/Leads/Spire.json", 0xff6989070ead67d9),
    ("Instruments/Leads/Square.json", 0x0d50940f6abe3d75),
    ("Instruments/Leads/Yellow.json", 0x661e3a8dcb5a1ff5),
    ("Instruments/Plucks/Amber.json", 0x11bf8de2dfd824cb),
    ("Instruments/Plucks/Classic.json", 0x9250d507f653c794),
    ("Instruments/Plucks/Ripple.json", 0x993d651848e9ad48),
    ("Instruments/Plucks/Skate.json", 0xc1b15f00dd880181),
    ("Instruments/Plucks/Static.json", 0x33797dc5a05724af),
    ("Instruments/Plucks/Umber.json", 0x37bdb31f4849d14c),
    ("Instruments/Sequences/Acid Spring.json", 0x459c401825cd0f89),
    ("Instruments/Sequences/Aeon.json", 0xe3728655f6863be2),
    ("Instruments/Sequences/Blood.json", 0x2c063b632dc9479e),
    ("Instruments/Sequences/Crushed.json", 0x7df444a155121599),
    ("Instruments/Sequences/Dance Floor.json", 0x9eb82e40a5c6d241),
    ("Instruments/Sequences/Floating.json", 0xe150f81877352df5),
    ("Instruments/Sequences/Folder.json", 0x0a99e6e6fae6cf5c),
    ("Instruments/Sequences/Glisten.json", 0x073151d1554b40d1),
    ("Instruments/Sequences/Ibiza.json", 0xa6c13a87759911e4),
    ("Instruments/Sequences/Iris.json", 0xe633a3406f192ea0),
    ("Instruments/Sequences/Locked.json", 0x524c1dd680d03853),
    ("Instruments/Sequences/Mint.json", 0x000ce93924b077c4),
    ("Instruments/Sequences/Never Sleep.json", 0xd24f247e29ed5bda),
    ("Instruments/Sequences/Orbit.json", 0x5be437ab7084a07d),
    ("Instruments/Sequences/Overcast.json", 0x3cefea26d6a08c18),
    ("Instruments/Sequences/Radar Click.json", 0x84515242d4bbcae3),
    ("Instruments/Sequences/Release.json", 0x3b0515750b592050),
    ("Instruments/Sequences/Seq Riser.json", 0xde6dee6b0cac7350),
    ("Instruments/Sequences/Servo.json", 0xb83055153fc01d36),
    ("Instruments/Sequences/Stargazer I.json", 0x9c4d701396363465),
    ("Instruments/Sequences/Summer Fun.json", 0xd45f1b8a0eecdcbe),
    ("Instruments/Sequences/They Know.json", 0xc795230f2fe10ab3),
    ("Instruments/Sequences/Trait.json", 0x0c7abe5627a9ccac),
    ("Instruments/Sequences/Wonky Town.json", 0xdb5a5730d8ae10e0),
    ("Instruments/Sequences/Zero Thru.json", 0x15baf5097766deb7),
    ("Instruments/Effects/Afterburner.json", 0xa41a17832c4b1108),
    ("Instruments/Effects/Approach.json", 0xd6a2a92bc363d871),
    ("Instruments/Effects/Aurora.json", 0x3cba9c53778f8ddf),
    ("Instruments/Effects/Avalanche.json", 0x161a2456c54b28ef),
    ("Instruments/Effects/Bellows.json", 0x93c47a4d92108c94),
    ("Instruments/Effects/Brownout.json", 0xc1d2052498435e94),
    ("Instruments/Effects/Downdraft.json", 0x58a916d92c4f55cf),
    ("Instruments/Effects/Escalator.json", 0x726e31d966a10228),
    ("Instruments/Effects/Geiger.json", 0x4ba6c9254a90c86d),
    ("Instruments/Effects/Gust.json", 0xd026acecdea4d9d3),
    ("Instruments/Effects/Hiss.json", 0x4ca6d93bb897387f),
    ("Instruments/Effects/Landing.json", 0x817cd0222e482b03),
    ("Instruments/Effects/Plume.json", 0xa286f4e75bd65d6c),
    ("Instruments/Effects/Plummet.json", 0xe17b1f7dccbc6673),
    ("Instruments/Effects/Rocket.json", 0xa26be52695766bec),
    ("Instruments/Effects/Skyward.json", 0xe59691bc3f8f072a),
    ("Instruments/Effects/Slipstream.json", 0xd72245127b8a9b0a),
    ("Instruments/Effects/Surf.json", 0x95aac10eb6418ed0),
    ("Instruments/Effects/Turbine.json", 0xd95950127c75abc1),
    ("Instruments/Effects/Updraft.json", 0x1b40b3386121af33),
    ("Instruments/Effects/Vortex.json", 0xb2c265360a63f527),
];

const PROBES_BEFORE_THE_CORE_PATCHES: [(&str, u64); 8] = [
    ("a chaos LFO on pitch", 0x90365813201cd309),
    ("a comb on one copy", 0xc35aec18e00c0be9),
    ("a comb on two copies spread from the start", 0x78200a8df0efbb68),
    ("hard sync without FM", 0xc8f6f5b79f68901d),
    ("hard sync under linear FM", 0xde913d9cd2fc9b09),
    ("tuned two octaves up", 0x335b18c1c91a2bb5),
    ("a chord on two voices at one velocity", 0xabc105405de8cf7d),
    ("a same-pitch tie that keeps its mod", 0xf8aa8ecd341ef001),
];

pub(crate) fn fingerprint(left: &[f32], right: &[f32]) -> u64 {
    let mut hash = FNV_OFFSET;
    for sample in left.iter().chain(right) {
        for byte in sample.to_bits().to_le_bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME);
        }
    }
    hash
}

fn render(renderer: &mut Renderer, left: &mut Vec<f32>, right: &mut Vec<f32>, end: usize) {
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    while left.len() < end {
        let n = BLOCK.min(end - left.len());
        renderer.render(&mut l[..n], &mut r[..n]);
        left.extend_from_slice(&l[..n]);
        right.extend_from_slice(&r[..n]);
    }
}

pub(crate) fn heard(project: &Project) -> (Vec<f32>, Vec<f32>) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    renderer.patient();
    renderer.start_workers(0, || None);
    audio.sync(project, None, &[], &[]);
    for midi in CHORD {
        audio.note_on(0, midi, VELOCITY);
    }
    audio.play(0.0);
    let (mut left, mut right) = (Vec::new(), Vec::new());
    render(&mut renderer, &mut left, &mut right, frames(HELD_SECONDS));
    for midi in CHORD {
        audio.note_off(0, midi);
    }
    render(&mut renderer, &mut left, &mut right, frames(HELD_SECONDS + TAIL_SECONDS));
    (left, right)
}

fn alone(track: Track) -> Project {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0] = track;
    project
}

pub(crate) fn placed(name: &str, text: &str) -> Option<Project> {
    let json = json::parse(text).unwrap_or_else(|| panic!("wanted {name} to parse as JSON"));
    let preset = Preset::from_json(&json).unwrap_or_else(|| panic!("wanted {name} to read as a preset"));
    let mut track = Track::blank();
    match preset {
        Preset::Rack(sound) if sound.rack.iter().any(Row::is_core) => {
            track.take_sound(sound);
        }
        Preset::Row(row, _) if row.is_core() => track.rack = vec![struck(Arp::DEFAULT), row],
        _ => return None,
    }
    Some(alone(track))
}

pub(crate) fn solo(arp: Arp, patch: Params) -> Project {
    let mut track = Track::blank();
    track.rack = vec![struck(arp), Row::Core(core_layer(1.0, patch))];
    alone(track)
}

fn chaos_on_pitch() -> Params {
    let mut patch = Params::DEFAULT;
    patch.motion.lfo1 = Lfo { target: Target::Pitch, amount: CHAOS_AMOUNT, shape: Shape::Chaos, rate_hz: CHAOS_HZ, ..Lfo::DEFAULT };
    patch
}

fn comb(unison: usize, width: f64) -> Params {
    let mut patch = Params::DEFAULT;
    patch.osc1 = Osc { unison, detune: DETUNE_CENTS, width, ..Osc::with_level(1.0) };
    patch.filter.on = true;
    patch.filter.mode = FilterMode::Comb;
    patch.filter.cutoff_hz = KEY_HZ;
    patch.filter.resonance = RESONANCE;
    patch
}

fn synced(fm: f64, linear_fm: bool) -> Params {
    let mut patch = Params::DEFAULT;
    patch.osc1 = Osc { levels: [0.0, 0.0, 0.0, 1.0], tune: FIFTH, sync: true, fm, linear_fm, ..Osc::with_level(1.0) };
    patch.osc2 = Osc { levels: [0.0, 0.0, 1.0, 0.0], ..Osc::with_level(0.0) };
    patch
}

fn two_octaves_up() -> Params {
    let mut patch = Params::DEFAULT;
    patch.osc1.tune = TWO_OCTAVES;
    patch
}

fn two_voices() -> Params {
    let mut patch = Params::DEFAULT;
    patch.voices = TWO_VOICES;
    patch
}

fn level_mod() -> Params {
    let mut patch = Params::DEFAULT;
    patch.modulation = Modulation { level1: 1.0, ..Modulation::DEFAULT };
    patch
}

fn tied_at_one_mod() -> Arp {
    let mut pattern = Pattern::DEFAULT;
    pattern.set_length(2);
    for step in 0..2 {
        pattern.set(Lane::Mod, step, FULL);
        pattern.set(Lane::Gate, step, FULL);
    }
    pattern.set_join(1, Join::Tie);
    Arp { on: true, stepped: true, sync: false, time_ms: STEP_MS, pattern, ..Arp::DEFAULT }
}

pub(crate) fn probes() -> [(&'static str, Arp, Params); 8] {
    [
        ("a chaos LFO on pitch", Arp::DEFAULT, chaos_on_pitch()),
        ("a comb on one copy", Arp::DEFAULT, comb(1, 0.0)),
        ("a comb on two copies spread from the start", Arp::DEFAULT, comb(2, 1.0)),
        ("hard sync without FM", Arp::DEFAULT, synced(0.0, false)),
        ("hard sync under linear FM", Arp::DEFAULT, synced(LINEAR_FM, true)),
        ("tuned two octaves up", Arp::DEFAULT, two_octaves_up()),
        ("a chord on two voices at one velocity", Arp::DEFAULT, two_voices()),
        ("a same-pitch tie that keeps its mod", tied_at_one_mod(), level_mod()),
    ]
}

fn side_by_side<T: Sync, R: Send>(jobs: &[T], work: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let workers = thread::available_parallelism().map_or(1, NonZeroUsize::get).min(jobs.len());
    let mut done: Vec<(usize, R)> = thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                scope.spawn(|| {
                    iter::from_fn(|| {
                        let at = next.fetch_add(1, Ordering::Relaxed);
                        jobs.get(at).map(|job| (at, work(job)))
                    })
                    .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|handle| handle.join().unwrap_or_else(|panic| panic::resume_unwind(panic))).collect()
    });
    done.sort_unstable_by_key(|&(at, _)| at);
    done.into_iter().map(|(_, result)| result).collect()
}

fn hashed(project: &Project) -> u64 {
    let (left, right) = heard(project);
    fingerprint(&left, &right)
}

fn fixture_line(name: &str, got: u64) -> String {
    format!("    (\"{name}\", {got:#018x}),")
}

#[test]
fn every_core_preset_in_the_bank_renders_as_it_did_before_the_patches() {
    let listed: Vec<(&str, u64, Project)> = BANK_BEFORE_THE_CORE_PATCHES
        .into_iter()
        .filter_map(|(name, wanted)| {
            let (_, text) = PRESETS.iter().find(|(listed, _)| *listed == name)?;
            Some((name, wanted, placed(name, text).unwrap_or_else(|| panic!("wanted {name} to carry a Core row"))))
        })
        .collect();
    let mut changed = Vec::new();
    for ((name, wanted, _), got) in listed.iter().zip(side_by_side(&listed, |(_, _, project)| hashed(project))) {
        if got != *wanted {
            changed.push(fixture_line(name, got));
        }
    }
    assert!(changed.is_empty(), "wanted every listed preset's held chord and tail to hash as it did before the Core patches, got these renders changed, as fixture lines:\n{}", changed.join("\n"));
}

#[test]
fn the_paths_the_patches_touched_render_as_they_did_before() {
    let probes = probes();
    let mut changed = Vec::new();
    for ((name, _, _), got) in probes.iter().zip(side_by_side(&probes, |(_, arp, patch)| hashed(&solo(*arp, *patch)))) {
        match PROBES_BEFORE_THE_CORE_PATCHES.iter().find(|(listed, _)| listed == name) {
            Some((_, wanted)) if *wanted == got => {}
            _ => changed.push(fixture_line(name, got)),
        }
    }
    assert!(changed.is_empty(), "wanted every probe patch to hash as it did before the Core patches, got these renders changed or unlisted, as fixture lines:\n{}", changed.join("\n"));
}
