use bp::audio::Audio;
use bp::audio::vision::Seer;
use bp::fx::params::{Freak, FreakSlope};
use bp::fx::{self, Kind, Slot};
use bp::project::{Clip, Project, Row, TICKS_PER_BEAT, Track};
use bp::wav::Wave;
use std::sync::Arc;
use support::{BLOCK, SAMPLE_RATE, beat_frame, wav};

const PATH: &str = "/x/click.wav";
const CLICK: i16 = 8_192;
const QUARTER: f32 = 0.25;
const CLICK_BEAT: usize = 1;
const FILE_BEATS: i32 = 2;
const FRAMES: usize = 36_000;
const LIMITER: usize = 71;
const LINEAR: usize = 431;
const EXACT: f32 = 1e-6;
const CLOSE: f32 = 1e-4;
const SHOWN: usize = 8;
const GROUP: usize = 0;
const SEND_A: usize = 0;
const UNITY: f32 = 1.0;

fn click_wave() -> Arc<Wave> {
    let mut samples = vec![0; FILE_BEATS as usize * beat_frame(1)];
    samples[beat_frame(CLICK_BEAT)] = CLICK;
    Arc::new(Wave::parse(String::from(PATH), String::from(PATH), &wav(&samples)).expect("wanted the click file to read"))
}

fn clicking(rack: Vec<Row>) -> Track {
    Track { rack, clips: vec![Clip::audio(0, FILE_BEATS * TICKS_PER_BEAT, String::from("click"), String::from(PATH))], ..Track::audio() }
}

fn limiter() -> Row {
    let mut slot = Slot::new(Kind::Utility);
    if let fx::Params::Utility(p) = &mut slot.params {
        p.limit = true;
        p.hard = false;
        p.ceiling_db = 0.0;
        p.lookahead_ms = 1.5;
    }
    Row::Effect(slot)
}

fn linear_crossover() -> Row {
    Row::Effect(Slot { params: fx::Params::Freak(Freak { slope: FreakSlope::Linear, oversample: false, ..Freak::DEFAULT }), ..Slot::new(Kind::Freak) })
}

fn grouped(group_rack: Vec<Row>, members: Vec<Track>, outside: Vec<Track>) -> Project {
    let mut project = Project::new();
    project.tracks.clear();
    project.add_track(GROUP, Track { rack: group_rack, ..Track::group() });
    let count = members.len();
    for (at, member) in members.into_iter().enumerate() {
        project.add_track(GROUP + 1 + at, member);
    }
    for (at, track) in outside.into_iter().enumerate() {
        project.add_track_outside(GROUP + 1 + count + at, track);
    }
    let inside = project.members_of(GROUP);
    assert_eq!(inside, (GROUP + 1..=GROUP + count).collect::<Vec<usize>>(), "wanted {count} members inside the group, got {inside:?}");
    project
}

fn render(project: &Project, waves: &[Arc<Wave>]) -> (Vec<f32>, usize) {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(project, None, &[], waves);
    let output = audio.timing().output;
    audio.play(0.0);
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut out = Vec::with_capacity(FRAMES + BLOCK);
    while out.len() < FRAMES {
        renderer.render(&mut l, &mut r);
        out.extend_from_slice(&l);
    }
    out.truncate(FRAMES);
    (out, output)
}

fn heard(out: &[f32], floor: f32) -> Vec<(usize, f32)> {
    out.iter().enumerate().filter(|(_, sample)| sample.abs() > floor).map(|(frame, sample)| (frame, *sample)).collect()
}

fn one_click(out: &[f32], frame: usize, height: f32, within: f32, what: &str) {
    let heard = heard(out, within);
    let alone = heard.len() == 1 && heard[0].0 == frame && (heard[0].1 - height).abs() <= within;
    assert!(alone, "{what}: wanted one click of {height} at frame {frame} and nothing else above {within}, got {} frames above it, the first {:?}", heard.len(), &heard[..heard.len().min(SHOWN)]);
}

#[test]
fn a_dry_member_and_a_limited_member_leave_their_group_as_one_click() {
    let n = beat_frame(CLICK_BEAT);
    let (out, output) = render(&grouped(Vec::new(), vec![clicking(Vec::new()), clicking(vec![limiter()])], Vec::new()), &[click_wave()]);
    assert_eq!(output, LIMITER, "wanted the output latency at {LIMITER}, got {output}");
    one_click(&out, n + LIMITER, 2.0 * QUARTER, EXACT, "a dry member beside a limited member");
}

#[test]
fn a_latent_group_rack_lands_with_a_dry_track_outside_it() {
    let n = beat_frame(CLICK_BEAT);
    let (out, output) = render(&grouped(vec![linear_crossover()], vec![clicking(Vec::new()), clicking(Vec::new())], vec![clicking(Vec::new())]), &[click_wave()]);
    assert_eq!(output, LINEAR, "wanted the output latency at {LINEAR}, got {output}");
    one_click(&out, n + LINEAR, 3.0 * QUARTER, CLOSE, "two members through a linear crossover on their group beside a dry track outside");
}

#[test]
fn a_members_send_lands_with_its_path_through_a_latent_group() {
    let n = beat_frame(CLICK_BEAT);
    let mut member = clicking(Vec::new());
    member.sends[SEND_A] = UNITY;
    let (out, output) = render(&grouped(vec![linear_crossover()], vec![member], Vec::new()), &[click_wave()]);
    assert_eq!(output, LINEAR, "wanted the output latency at {LINEAR}, got {output}");
    one_click(&out, n + LINEAR, 2.0 * QUARTER, CLOSE, "a member sent to return A beside its path through a linear crossover on the group");
}
