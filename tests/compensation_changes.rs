use bp::audio::vision::Seer;
use bp::audio::{Audio, Renderer};
use bp::fx::params::{Freak, FreakSlope};
use bp::fx::{Kind, Params, Slot};
use bp::project::{Clip, Project, Row, TICKS_PER_BEAT, Track};
use bp::wav::Wave;
use std::f64::consts::TAU;
use std::sync::Arc;
use support::{BLOCK, SAMPLE_RATE, SILENT};

const NAME: &str = "sine";
const HZ: f64 = 1_000.0;
const AMPLITUDE: i16 = 8_192;
const SCALE: f32 = 32_768.0;
const DOUBLED: f32 = 0.5;
const WAVE_SECONDS: usize = 3;
const CLIP_BEATS: i32 = 16;
const SETTLED_IN: usize = 4_800;
const BEFORE: usize = 48_000;
const PHASES: [usize; 6] = [0, 8, 16, 24, 32, 40];
const SPLICE: usize = 64;
const WATCH: usize = 24_000;
const SETTLED: usize = 24_000;
const AROUND: usize = 1_024;
const STEPS: f32 = 4.0;
const ALIGNED: f32 = 1e-3;
const UTILITY: usize = 71;
const LIN: usize = 431;

fn utility() -> Row {
    let mut slot = Slot::new(Kind::Utility);
    if let Params::Utility(p) = &mut slot.params {
        p.limit = true;
        p.hard = false;
        p.ceiling_db = 0.0;
        p.lookahead_ms = 1.5;
    }
    Row::Effect(slot)
}

fn bypassed() -> Row {
    let mut row = utility();
    if let Row::Effect(slot) = &mut row {
        slot.bypass = true;
    }
    row
}

fn linear_crossover() -> Row {
    Row::Effect(Slot { params: Params::Freak(Freak { slope: FreakSlope::Linear, oversample: false, ..Freak::DEFAULT }), ..Slot::new(Kind::Freak) })
}

fn sine() -> Arc<Wave> {
    let pcm = (0..WAVE_SECONDS * SAMPLE_RATE as usize).map(|i| ((TAU * HZ * i as f64 / SAMPLE_RATE).sin() * f64::from(AMPLITUDE)).round() as i16).collect();
    Arc::new(Wave::sound(String::from(NAME), SAMPLE_RATE, 1, pcm))
}

fn project(racks: &[Vec<Row>]) -> Project {
    let mut project = Project::new();
    project.tracks = racks
        .iter()
        .map(|rack| {
            let mut track = Track::audio();
            track.clips.push(Clip::audio(0, CLIP_BEATS * TICKS_PER_BEAT, String::from(NAME), String::from(NAME)));
            track.rack = rack.clone();
            track
        })
        .collect();
    project
}

fn render(renderer: &mut Renderer, frames: usize, out: &mut Vec<f32>) {
    let (mut l, mut r) = (vec![0.0; BLOCK], vec![0.0; BLOCK]);
    let mut done = 0;
    while done < frames {
        let n = BLOCK.min(frames - done);
        renderer.render(&mut l[..n], &mut r[..n]);
        out.extend_from_slice(&l[..n]);
        done += n;
    }
}

struct Played {
    change: usize,
    lo_before: usize,
    lo_after: usize,
    out: Vec<f32>,
}

fn play_across(first: &Project, second: &Project, waves: &[Arc<Wave>], change: usize) -> Played {
    let (mut audio, mut renderer) = Audio::open(SAMPLE_RATE, BLOCK, Seer::Inline);
    audio.sync(first, None, &[], waves);
    let lo_before = audio.timing().output;
    audio.play(0.0);
    let mut out = Vec::with_capacity(change + WATCH + SETTLED);
    render(&mut renderer, change, &mut out);
    audio.sync(second, None, &[], waves);
    let lo_after = audio.timing().output;
    render(&mut renderer, WATCH + SETTLED, &mut out);
    Played { change, lo_before, lo_after, out }
}

fn worst_misalignment(out: &[f32], pcm: &[i16], lo: usize, from: usize, to: usize) -> (usize, f32) {
    (from..to).map(|t| (t, (out[t] - 2.0 * f32::from(pcm[t - lo]) / SCALE).abs())).fold((from, 0.0), |worst, candidate| if candidate.1 > worst.1 { candidate } else { worst })
}

fn peak(out: &[f32]) -> f32 {
    out.iter().fold(0.0f32, |peak, sample| peak.max(sample.abs()))
}

fn longest_quiet_run(out: &[f32], from: usize, to: usize) -> (usize, usize) {
    let mut longest = (from, 0);
    let mut run: Option<usize> = None;
    for (t, sample) in out[..to].iter().enumerate().skip(from) {
        if sample.abs() < SILENT {
            run.get_or_insert(t);
        } else if let Some(start) = run.take()
            && t - start > longest.1
        {
            longest = (start, t - start);
        }
    }
    if let Some(start) = run
        && to - start > longest.1
    {
        longest = (start, to - start);
    }
    longest
}

fn largest_step(out: &[f32], from: usize, to: usize) -> (usize, f32) {
    (from..to - 1).map(|t| (t, (out[t + 1] - out[t]).abs())).fold((from, 0.0), |worst, candidate| if candidate.1 > worst.1 { candidate } else { worst })
}

fn own_step(pcm: &[i16]) -> f32 {
    pcm.windows(2).map(|pair| (f32::from(pair[1]) - f32::from(pair[0])).abs() / SCALE).fold(0.0f32, f32::max)
}

fn judge(name: &str, before: &[Vec<Row>], after: &[Vec<Row>], expected: (usize, usize), cold: usize) {
    let wave = sine();
    let waves = [Arc::clone(&wave)];
    let (first, second) = (project(before), project(after));
    let own = own_step(&wave.pcm);
    let plays: Vec<Played> = PHASES.iter().map(|&phase| play_across(&first, &second, &waves, BEFORE + phase)).collect();
    for played in &plays {
        let Played { change, lo_before, lo_after, out } = played;
        let (change, lo_before, lo_after) = (*change, *lo_before, *lo_after);
        assert_eq!((lo_before, lo_after), expected, "{name}: wanted the output latency to go from {} to {}, got {lo_before} to {lo_after}", expected.0, expected.1);
        let (frame, delta) = worst_misalignment(out, &wave.pcm, lo_before, SETTLED_IN, change);
        assert!(delta <= ALIGNED, "{name}: wanted both tracks aligned before the change at frame {change}, twice the sine {lo_before} late, got a difference of {delta} at frame {frame}");
        let before_peak = peak(&out[SETTLED_IN..change]);
        assert!((before_peak - DOUBLED).abs() <= ALIGNED, "{name}: wanted the two sines summing to {DOUBLED} before the change, got a peak of {before_peak}");
        let (hole_at, hole) = longest_quiet_run(out, change, change + WATCH);
        assert!(hole <= SPLICE + cold, "{name}: wanted no silence beyond the {SPLICE} frame splice and the cold row's {cold} after the change at frame {change}, got {hole} quiet frames from frame {hole_at}");
        let settled = change + WATCH;
        let (quiet_at, quiet) = longest_quiet_run(out, settled, settled + SETTLED);
        assert!(quiet <= SPLICE, "{name}: wanted no run over {SPLICE} quiet frames once settled, got {quiet} from frame {quiet_at}");
        let (frame, delta) = worst_misalignment(out, &wave.pcm, lo_after, settled, settled + SETTLED);
        assert!(delta <= ALIGNED, "{name}: wanted both tracks aligned {SETTLED} frames after the change, twice the sine {lo_after} late, got a difference of {delta} at frame {frame}");
        let after_peak = peak(&out[settled..settled + SETTLED]);
        assert!((after_peak - DOUBLED).abs() <= ALIGNED, "{name}: wanted the two sines summing to {DOUBLED} once settled, got a peak of {after_peak}");
    }
    let (worst, (step_at, step)) = plays.iter().map(|played| (played.change, largest_step(&played.out, played.change - AROUND, played.change + WATCH))).fold((0, (0, 0.0f32)), |worst, candidate| if candidate.1.1 > worst.1.1 { candidate } else { worst });
    assert!(step <= STEPS * own, "{name}: wanted the largest step around the change within {STEPS} times the sine's own {own}, {}, got {step} at frame {step_at} for the change at frame {worst}, {} frames after it", STEPS * own, step_at + 1 - worst);
}

#[test]
fn switching_a_71_sample_utility_to_a_431_sample_lin_freak_leaves_one_splice() {
    judge("utility to freak", &[Vec::new(), vec![utility()]], &[Vec::new(), vec![linear_crossover()]], (UTILITY, LIN), LIN);
}

#[test]
fn inserting_a_latent_row_onto_a_dry_track_leaves_one_splice() {
    judge("insert", &[Vec::new(), Vec::new()], &[Vec::new(), vec![utility()]], (0, UTILITY), UTILITY);
}

#[test]
fn removing_the_latent_row_leaves_one_splice() {
    judge("remove", &[Vec::new(), vec![utility()]], &[Vec::new(), Vec::new()], (UTILITY, 0), 0);
}

#[test]
fn bypassing_the_latent_row_leaves_one_splice() {
    judge("bypass", &[Vec::new(), vec![utility()]], &[Vec::new(), vec![bypassed()]], (UTILITY, 0), 0);
}

#[test]
fn unbypassing_the_latent_row_leaves_one_splice() {
    judge("unbypass", &[Vec::new(), vec![bypassed()]], &[Vec::new(), vec![utility()]], (0, UTILITY), UTILITY);
}
