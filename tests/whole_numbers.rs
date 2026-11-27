use bp::json::{self, Json};
use bp::project::Project;

const SONG: &str = r#"{
  "kind": "project",
  "tracks": [
    {
      "name": "Keys",
      "kind": "midi",
      "rack": [
        {"kind": "midi", "velocity": 90.6, "arp": {"copies": 2.7}},
        {"kind": "core", "voices": 7.7, "stack": 2.7, "osc1": {"unison": 2.7}},
        {"kind": "freak", "bands": [{"times": 2.7, "foldRippleCount": 2.7}]},
        {"kind": "repeat", "offsetSixteenths": 2.7},
        {"kind": "reverb", "plateauCopies": 20.7}
      ]
    },
    {
      "name": "Take",
      "kind": "audio",
      "clips": [{"startBeat": 0, "lengthBeats": 4, "wave": {"name": "take", "semitones": 2.6}}]
    }
  ],
  "palette": [
    {"degree": 1, "inversion": 1.6, "octaveShift": -1.6, "liftLowest": 2.6, "liftHighest": 3.6, "liftOctaves": 1.6},
    {"degree": 2, "inversion": 1e12, "octaveShift": -1e12, "liftLowest": 1e12, "liftHighest": 1e12, "liftOctaves": 1e12}
  ]
}"#;

const FRACTIONS: [(&str, f64, f64); 15] = [
    ("tracks/0/rack/0/velocity", 90.6, 91.0),
    ("tracks/0/rack/0/arp/copies", 2.7, 3.0),
    ("tracks/0/rack/1/voices", 7.7, 8.0),
    ("tracks/0/rack/1/stack", 2.7, 3.0),
    ("tracks/0/rack/1/osc1/unison", 2.7, 3.0),
    ("tracks/0/rack/2/bands/0/times", 2.7, 3.0),
    ("tracks/0/rack/2/bands/0/foldRippleCount", 2.7, 3.0),
    ("tracks/0/rack/3/offsetSixteenths", 2.7, 3.0),
    ("tracks/0/rack/4/plateauCopies", 20.7, 21.0),
    ("tracks/1/clips/0/wave/semitones", 2.6, 3.0),
    ("palette/0/inversion", 1.6, 2.0),
    ("palette/0/octaveShift", -1.6, -2.0),
    ("palette/0/liftLowest", 2.6, 3.0),
    ("palette/0/liftHighest", 3.6, 4.0),
    ("palette/0/liftOctaves", 1.6, 2.0),
];

const OVERSIZED: [(&str, f64, f64); 5] = [
    ("palette/1/inversion", 1e12, 2.0),
    ("palette/1/octaveShift", -1e12, -2.0),
    ("palette/1/liftLowest", 1e12, i32::MAX as f64),
    ("palette/1/liftHighest", 1e12, i32::MAX as f64),
    ("palette/1/liftOctaves", 1e12, 2.0),
];

fn number(document: &Json, path: &str) -> Option<f64> {
    let found = path.split('/').try_fold(document, |here, step| match here {
        Json::Array(items) => items.get(step.parse::<usize>().ok()?),
        _ => here.get(step),
    });
    found.and_then(Json::as_f64)
}

fn opened_apart(fields: &[(&str, f64, f64)]) -> Vec<String> {
    let song = json::parse(SONG).expect("wanted the song to parse");
    let opened = Project::from_json(&song).expect("wanted the song to open").to_json();
    let apart = fields.iter().filter_map(|(path, written, wanted)| {
        assert_eq!(number(&song, path), Some(*written), "wanted the song to hold {written} at {path}");
        let got = number(&opened, path);
        (got != Some(*wanted)).then(|| format!("{path}: {written} opened as {got:?}, wanted {wanted}"))
    });
    apart.collect()
}

#[test]
fn a_fraction_in_a_whole_number_field_opens_as_the_nearest_whole_number() {
    let apart = opened_apart(&FRACTIONS);
    assert!(apart.is_empty(), "wanted each fraction opened as the nearest whole number, got {apart:#?}");
}

#[test]
fn a_palette_number_far_past_its_range_opens_at_the_ranges_end() {
    let apart = opened_apart(&OVERSIZED);
    assert!(apart.is_empty(), "wanted each oversized palette number opened at the end of its range, got {apart:#?}");
}
