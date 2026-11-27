use bp::app::{App, Event, Opened, Request};
use bp::json::{self, Json};
use bp::keyboard::Key;
use bp::project::{Clip, Project, TICKS_PER_BEAT, Track};
use bp::wav::Wave;
use support::{HEIGHT, RATE, WIDTH};

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
const NAME: &str = "hand";
const GOOD: &str = "good";
const BAD: &str = "bad";
const PATH: &str = "/tmp/wave-lines-under-test.json";
const LINE: usize = 1_024;
const STRAYS: [&str; 5] = ["!", "-", "_", ".", "é"];
const DEEP: usize = 603;
const SPLIT_GROUP: usize = 1_021;

fn bytes(count: usize, salt: usize) -> Vec<u8> {
    (0..count).map(|at| (at.wrapping_mul(97).wrapping_add(salt.wrapping_mul(131)) % 256) as u8).collect()
}

fn base64(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for group in bytes.chunks(3) {
        let bits = group.iter().enumerate().fold(0u32, |bits, (at, &byte)| bits | u32::from(byte) << (16 - 8 * at));
        for at in 0..4 {
            text.push(if at <= group.len() { char::from(ALPHABET[(bits >> (18 - 6 * at)) as usize & 63]) } else { '=' });
        }
    }
    text
}

fn unpadded(bytes: &[u8]) -> String {
    String::from(base64(bytes).trim_end_matches('='))
}

fn samples(bytes: &[u8]) -> Vec<i16> {
    bytes.as_chunks::<2>().0.iter().map(|pair| i16::from_le_bytes(*pair)).collect()
}

fn late(bytes: &[u8], bits: u32) -> Vec<u8> {
    let mut before = 0u8;
    bytes.iter().map(|&byte| {
        let read = before << (8 - bits) | byte >> bits;
        before = byte;
        read
    }).collect()
}

fn cut(text: &str, lengths: &[usize]) -> Vec<String> {
    let mut lines = Vec::new();
    let mut rest = text;
    for &length in lengths.iter().cycle() {
        if rest.is_empty() {
            break;
        }
        let (line, after) = rest.split_at(length.min(rest.len()));
        lines.push(String::from(line));
        rest = after;
    }
    lines
}

fn spaced(line: &str, every: usize, gap: &str) -> String {
    let mut out = String::new();
    for (at, character) in line.chars().enumerate() {
        if at > 0 && at % every == 0 {
            out.push_str(gap);
        }
        out.push(character);
    }
    out
}

fn quoted(line: &str) -> String {
    let mut out = String::from("\"");
    for character in line.chars() {
        match character {
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\n' => out.push_str("\\n"),
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn entry(name: &str, lines: &[String]) -> String {
    let lines: Vec<String> = lines.iter().map(|line| quoted(line)).collect();
    format!("{{\"name\": \"{name}\", \"sampleRateHz\": {RATE}, \"channels\": 1, \"pcm16Base64\": [{}]}}", lines.join(", "))
}

fn decoded(lines: &[String]) -> Option<Vec<i16>> {
    let text = entry(NAME, lines);
    let json = json::parse(&text).unwrap_or_else(|| panic!("wanted the hand-written entry to parse as JSON: {text}"));
    Wave::from_json(&json).map(|wave| wave.pcm)
}

fn expect_samples(what: &str, got: Option<Vec<i16>>, wanted: &[i16]) {
    let Some(got) = got else {
        panic!("{what}: wanted {} samples, the entry was refused", wanted.len());
    };
    if let Some(at) = got.iter().zip(wanted).position(|(a, b)| a != b) {
        panic!("{what}: sample {at} wanted {} got {}", wanted[at], got[at]);
    }
    assert_eq!(got.len(), wanted.len(), "{what}: wanted {} samples, got {}", wanted.len(), got.len());
}

#[test]
fn groups_cut_across_lines_decode_as_one_stream() {
    let source = bytes(3_000, 1);
    let text = base64(&source);
    for lengths in [vec![1], vec![2, 3], vec![5], vec![7, 9, 11], vec![SPLIT_GROUP, 3], vec![LINE - 1], vec![LINE + 1, 6]] {
        expect_samples(&format!("lines of {lengths:?} characters"), decoded(&cut(&text, &lengths)), &samples(&source));
    }
}

#[test]
fn a_last_group_of_two_or_three_characters_needs_no_padding() {
    for (count, last) in [(4, 2), (8, 3), (1_000, 2), (1_534, 2), (1_001, 3), (5, 3), (7, 2)] {
        let source = bytes(count, 2);
        let text = unpadded(&source);
        let group = text.len() % 4;
        assert_eq!(group, last, "{count} bytes: wanted a last group of {last} characters, the text ends in {group}");
        for lengths in [vec![LINE], vec![7]] {
            expect_samples(&format!("{count} bytes ending in a group of {last} characters, lines of {lengths:?}"), decoded(&cut(&text, &lengths)), &samples(&source));
        }
    }
}

#[test]
fn padding_at_the_end_of_the_last_line_changes_no_sample() {
    for count in [4, 8, 1_000, 1_001] {
        let source = bytes(count, 3);
        let text = base64(&source);
        assert!(text.ends_with('='), "{count} bytes: wanted the text padded, got {}", &text[text.len() - 4..]);
        for lengths in [vec![LINE], vec![5], vec![text.len() - 1]] {
            expect_samples(&format!("{count} bytes padded, lines of {lengths:?}"), decoded(&cut(&text, &lengths)), &samples(&source));
        }
    }
}

#[test]
fn a_pad_ends_its_line_and_later_lines_carry_on_from_the_bits_before_it() {
    let (first, second) = (bytes(600, 4), bytes(900, 5));
    let mut lines = vec![format!("{}=AAAA! -_é", base64(&first))];
    lines.extend(cut(&base64(&second), &[LINE]));
    expect_samples("a pad after whole groups, with stray characters after it", decoded(&lines), &samples(&[first, second].concat()));
    for (count, carried, after) in [(601, 4, 897), (602, 2, 900)] {
        let first = bytes(count, 6);
        let second = bytes(after, 7);
        let mut lines = vec![format!("{}QUJD", base64(&first))];
        lines.extend(cut(&base64(&second), &[13]));
        let wanted = samples(&[first, late(&second, carried)].concat());
        expect_samples(&format!("a pad after {count} bytes, {carried} bits carried into the next line"), decoded(&lines), &wanted);
    }
}

#[test]
fn spaces_tabs_and_line_breaks_inside_and_around_lines_are_skipped() {
    let source = bytes(1_502, 8);
    let text = base64(&source);
    let mut lines: Vec<String> = cut(&text, &[13, 7, LINE, 5, 30]).iter().enumerate().map(|(at, line)| match at % 4 {
        0 => format!("  {line}\r\n"),
        1 => format!("\t{}\n", spaced(line, 3, " ")),
        2 => format!(" {} \t", spaced(line, 9, "\r\n")),
        _ => spaced(line, 1, "\t"),
    }).collect();
    lines.insert(1, String::new());
    lines.insert(3, String::from(" \t\r\n "));
    if let Some(last) = lines.last_mut() {
        let padding = last.rfind(|character: char| character != '=' && !character.is_ascii_whitespace()).map_or(0, |at| at + 1);
        last.insert_str(padding, " \r\n\t");
    }
    expect_samples("whitespace inside, around and between lines", decoded(&lines), &samples(&source));
}

#[test]
fn one_stray_character_refuses_the_whole_wave() {
    let source = bytes(1_200, 9);
    let text = base64(&source);
    assert!(!text.ends_with('='), "wanted the text unpadded so a stray at its end is read");
    expect_samples("the clean text", decoded(&cut(&text, &[LINE])), &samples(&source));
    let (first, second) = (bytes(601, 10), bytes(900, 11));
    for stray in STRAYS {
        let mut deep = cut(&text, &[LINE]);
        deep[0].insert_str(DEEP, stray);
        let mut split = cut(&text, &[SPLIT_GROUP]);
        split[1].insert_str(0, stray);
        let mut last = cut(&text, &[LINE]);
        if let Some(line) = last.last_mut() {
            line.push_str(stray);
        }
        let after_pad = vec![base64(&first), format!("{stray}{}", base64(&second))];
        for (place, lines) in [("deep in a long line", deep), ("inside a group split across lines", split), ("at the very end", last), ("on the line after a pad", after_pad)] {
            assert_eq!(decoded(&lines).map(|pcm| pcm.len()), None, "wanted a stray {stray:?} {place} to refuse the wave");
        }
    }
}

fn fresh() -> App {
    let mut app = App::new();
    app.draw(WIDTH, HEIGHT);
    app.take_requests();
    app
}

fn playing(name: &str) -> Track {
    Track { clips: vec![Clip::audio(0, TICKS_PER_BEAT, String::new(), String::from(name))], ..Track::audio() }
}

fn document(entries: &[String]) -> String {
    let mut project = Project::new();
    project.tracks = vec![playing(GOOD), playing(BAD)];
    let text = project.to_json().write();
    let body = text.trim_end().strip_suffix('}').expect("wanted the project written as one object").trim_end();
    format!("{body},\n  \"waves\": [\n    {}\n  ]\n}}\n", entries.join(",\n    "))
}

fn saved(app: &mut App) -> Json {
    app.event(Event::Key { key: Key::Char('s'), shift: false, command: true, repeat: false });
    let text = app.take_requests().into_iter().find_map(|request| match request {
        Request::Write { json, .. } | Request::SaveAs { json, .. } => Some(json),
        _ => None,
    }).expect("wanted a save request after Cmd+S");
    json::parse(&text).expect("wanted the save to parse")
}

#[test]
fn the_app_opens_a_quirky_entry_whole_and_leaves_out_one_with_a_stray() {
    let (first, second) = (bytes(602, 12), bytes(1_500, 13));
    let mut quirky = vec![format!(" {}=junk!", base64(&first))];
    quirky.extend(cut(&unpadded(&second), &[7, LINE + 3]).iter().map(|line| format!("{line}\r\n")));
    let wanted = samples(&[first, late(&second, 2)].concat());
    let mut stray = cut(&base64(&bytes(1_500, 14)), &[LINE]);
    stray[0].insert(DEEP, '!');
    let mut app = fresh();
    assert_eq!(app.opened(String::from(PATH), Some(&document(&[entry(GOOD, &quirky), entry(BAD, &stray)])), None), Opened::Project, "wanted the hand-written project to open");
    let reads: Vec<String> = app.take_requests().into_iter().filter_map(|request| if let Request::Read { path } = request { Some(path) } else { None }).collect();
    assert!(reads.is_empty(), "wanted nothing read from disk for waves the file holds, got {reads:?}");
    let file = saved(&mut app);
    let clips = file.get("tracks").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(|track| track.get("clips").and_then(Json::as_array)).flatten().filter(|clip| clip.get("wave").is_some()).count();
    assert_eq!(clips, 2, "wanted both audio clips kept, got {clips}");
    let pool: Vec<Wave> = file.get("waves").and_then(Json::as_array).unwrap_or(&[]).iter().filter_map(Wave::from_json).collect();
    let names: Vec<&str> = pool.iter().map(|wave| wave.name.as_str()).collect();
    assert_eq!(names, vec![GOOD], "wanted only the readable wave saved back, got {names:?}");
    expect_samples("the quirky wave saved back by the app", pool.into_iter().next().map(|wave| wave.pcm), &wanted);
}
