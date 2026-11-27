use bp::app::{App, Opened};
use bp::json::{self, Json};
use bp::mcp::Call;
use bp::project::{Clip, DEFAULT_BPM, Project, TICKS_PER_BEAT, Track};
use bp::wav::Wave;
use std::f64::consts::TAU;
use std::time::Duration;
use support::{HEIGHT, SAMPLE_RATE, WIDTH};

const SONG: &str = "/struck/songs/song.json";
const NAME: &str = "strikes";
const BEATS: i32 = 4;
const FRAMES_PER_BEAT: f64 = SAMPLE_RATE * 60.0 / DEFAULT_BPM;
const SOUNDING: usize = 4_800;
const FULL_SCALE: f64 = 32_768.0;
const LOW: Strike = Strike { frame: 24_137, hz: 200.0, peak: 16_384.0, attack: 0 };
const HIGH: Strike = Strike { frame: 48_301, hz: 3_000.0, peak: 8_192.0, attack: 0 };
const SWELL: Strike = Strike { frame: 72_019, hz: 400.0, peak: 16_384.0, attack: 1_920 };
const ONSET_SLACK_BEATS: f64 = 0.002;
const PEAK_SLACK_DB: f64 = 0.15;
const HZ_SLACK: f64 = 0.03;
const PATIENCE: Duration = Duration::from_secs(120);
const TICK: f32 = 0.01;
const PAUSE: Duration = Duration::from_millis(2);
const WHOLE: &str = "{}";
const HALVES: &str = r#"{"spans":[[0,2],[2,4]]}"#;

#[derive(Clone, Copy)]
struct Strike {
    frame: usize,
    hz: f64,
    peak: f64,
    attack: usize,
}

impl Strike {
    fn beat(self) -> f64 {
        self.frame as f64 / FRAMES_PER_BEAT
    }

    fn peak_db(self) -> f64 {
        20.0 * (self.peak / FULL_SCALE).log10()
    }
}

fn strikes() -> Vec<i16> {
    let mut pcm = vec![0i16; (f64::from(BEATS) * FRAMES_PER_BEAT) as usize];
    for strike in [LOW, HIGH, SWELL] {
        for at in 0..SOUNDING {
            let risen = if at < strike.attack { at as f64 / strike.attack as f64 } else { 1.0 };
            pcm[strike.frame + at] = (strike.peak * risen * (TAU * strike.hz * at as f64 / SAMPLE_RATE).sin()).round() as i16;
        }
    }
    pcm
}

fn song() -> String {
    let mut project = Project::new();
    project.tracks.truncate(1);
    project.tracks[0] = Track::audio();
    project.tracks[0].clips.push(Clip::audio(0, BEATS * TICKS_PER_BEAT, String::from(NAME), String::from(NAME)));
    let mut document = project.to_json();
    if let Json::Object(fields) = &mut document {
        fields.push((String::from("waves"), Json::Array(vec![Wave::sound(String::from(NAME), SAMPLE_RATE, 1, strikes()).to_json()])));
    }
    document.write()
}

fn struck() -> App {
    let mut app = App::new();
    app.draw(WIDTH, HEIGHT);
    let opened = app.opened(String::from(SONG), Some(&song()), None);
    assert_eq!(opened, Opened::Project, "wanted the song of three strikes to open, got {opened:?}");
    app.take_requests();
    app
}

fn listened(app: &mut App, args: &str) -> Json {
    let (listen, waiting) = Call::new("listen", json::parse(args).expect("wanted the arguments to parse"));
    app.deliver(listen);
    let answer = std::thread::spawn(move || waiting.reply(PATIENCE));
    while !answer.is_finished() {
        app.tick(TICK);
        std::thread::sleep(PAUSE);
    }
    let reply = answer.join().expect("wanted the listen answered");
    assert!(!reply.failed, "wanted listen {args} answered, got {}", reply.text);
    json::parse(&reply.text).unwrap_or_else(|| panic!("wanted listen to answer JSON, got {}", reply.text))
}

fn hits(heard: &Json) -> Vec<Vec<f64>> {
    heard.get("hits").and_then(Json::as_array).unwrap_or(&[]).iter().map(|hit| hit.as_array().unwrap_or(&[]).iter().filter_map(Json::as_f64).collect()).collect()
}

#[test]
fn a_hit_tells_where_its_attack_starts_its_peak_and_how_bright_it_is() {
    let heard = listened(&mut struck(), WHOLE);
    let hits = hits(&heard);
    assert_eq!(hits.len(), 3, "wanted the three strikes told as three hits, got {hits:?}");
    for (name, strike, hit) in [("low", LOW, &hits[0]), ("high", HIGH, &hits[1]), ("swell", SWELL, &hits[2])] {
        let [beat, peak_db, hz] = hit.as_slice() else {
            panic!("wanted the {name} strike told as beat, peakDb and hz, got {hit:?}");
        };
        assert!((beat - strike.beat()).abs() <= ONSET_SLACK_BEATS, "wanted the {name} strike told where it starts, beat {:.4}, within {ONSET_SLACK_BEATS}, got {beat}", strike.beat());
        assert!((peak_db - strike.peak_db()).abs() <= PEAK_SLACK_DB, "wanted the {name} strike's peak at {:.1} dB within {PEAK_SLACK_DB}, got {peak_db}", strike.peak_db());
        assert!((hz / strike.hz - 1.0).abs() <= HZ_SLACK, "wanted the {name} strike, a {} Hz tone, told as bright as one within {HZ_SLACK}, got {hz} Hz", strike.hz);
    }
}

#[test]
fn a_swell_is_told_at_its_start_though_its_loudest_sample_comes_forty_milliseconds_on() {
    let heard = listened(&mut struck(), HALVES);
    let later = heard.get("spans").and_then(Json::as_array).and_then(|spans| spans.get(1)).unwrap_or_else(|| panic!("wanted the second half measured as a span, got {}", heard.write()));
    let loudest = later.get("peakBeat").and_then(Json::as_f64).unwrap_or_else(|| panic!("wanted the second half's loudest instant told, got {}", later.write()));
    let risen = SWELL.beat() + SWELL.attack as f64 / FRAMES_PER_BEAT;
    assert!(loudest >= risen - ONSET_SLACK_BEATS, "wanted the second half loudest once the swell has risen, from beat {risen:.4}, got {loudest}");
    let swell = hits(later).pop().unwrap_or_else(|| panic!("wanted the swell told as a hit in the second half, got {}", later.write()));
    assert!((swell[0] - SWELL.beat()).abs() <= ONSET_SLACK_BEATS, "wanted the swell's hit told where it starts, beat {:.4}, not at its loudest, beat {loudest}; got {}", SWELL.beat(), swell[0]);
}

#[test]
fn the_hits_inside_spans_are_the_whole_renders_hits_figure_for_figure() {
    let mut app = struck();
    let whole = hits(&listened(&mut app, WHOLE));
    let halves = listened(&mut app, HALVES);
    let spans = halves.get("spans").and_then(Json::as_array).unwrap_or_else(|| panic!("wanted both halves measured as spans, got {}", halves.write()));
    let (first, second) = (hits(&spans[0]), hits(&spans[1]));
    assert_eq!(first, whole[..1], "wanted the first half to hold the low strike as the whole render tells it");
    assert_eq!(second, whole[1..], "wanted the second half to hold the high strike and the swell as the whole render tells them");
    assert_eq!(hits(&halves), Vec::<Vec<f64>>::new(), "wanted a listen with spans to list its hits under them alone");
}
