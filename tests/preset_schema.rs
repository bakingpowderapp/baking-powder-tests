use bp::arp::{LANE_STEPS, Lane};
use bp::core::params::Modulation;
use bp::factory::PRESETS;
use bp::json::{self, Json, tidy};
use bp::project::{Aim, Document, Preset, Row, manual};

const WORDS: [&str; 3] = ["description", "author", "website"];
const MOD_BAND: &str = "MOD";
const APART: &str = ",;:\"()[]{}";

#[derive(Clone)]
enum Step {
    Key(String),
    Index(usize),
}

fn spell(path: &[Step]) -> String {
    let mut spelled = String::new();
    for step in path {
        match step {
            Step::Key(key) if spelled.is_empty() => spelled.push_str(key),
            Step::Key(key) => spelled.push_str(&format!(".{key}")),
            Step::Index(index) => spelled.push_str(&format!("[{index}]")),
        }
    }
    spelled
}

fn place<'a>(json: &'a mut Json, path: &[Step]) -> Option<&'a mut Json> {
    let Some((first, rest)) = path.split_first() else {
        return Some(json);
    };
    let inner = match (json, first) {
        (Json::Object(fields), Step::Key(key)) => fields.iter_mut().find(|(own, _)| own == key).map(|(_, value)| value)?,
        (Json::Array(items), Step::Index(index)) => items.get_mut(*index)?,
        _ => return None,
    };
    place(inner, rest)
}

struct Known {
    strings: Vec<String>,
    names: Vec<String>,
}

fn collect(json: &Json, into: &mut Vec<String>) {
    match json {
        Json::String(text) => into.push(text.clone()),
        Json::Array(items) => items.iter().for_each(|item| collect(item, into)),
        Json::Object(fields) => fields.iter().for_each(|(_, value)| collect(value, into)),
        _ => {}
    }
}

fn known() -> Known {
    let text = manual::document().write() + &manual::preset().write();
    let words = text.split(|c: char| c.is_whitespace() || APART.contains(c)).flat_map(|word| [word, word.trim_end_matches('.')]);
    let mut names: Vec<String> = words.filter(|word| !word.is_empty()).map(String::from).collect();
    names.sort();
    names.dedup();
    let mut strings = Vec::new();
    for (_, text) in PRESETS {
        if let Some(json) = json::parse(text) {
            collect(&json, &mut strings);
        }
    }
    strings.sort();
    strings.dedup();
    Known { strings, names }
}

struct Probe<'a> {
    original: &'a Json,
    preset: &'a Preset,
    known: &'a Known,
}

impl Probe<'_> {
    fn loads_differently(&self, path: &[Step], value: Json) -> bool {
        let mut changed = self.original.clone();
        if let Some(slot) = place(&mut changed, path) {
            *slot = value;
        }
        !matches!(Document::from_json(&changed), Some(Document::Preset(preset)) if preset == *self.preset)
    }

    fn others(&self, value: &Json) -> Vec<Json> {
        match value {
            Json::Number(n) => {
                let n = *n;
                let tried = [n + 1.0, n - 1.0, n + 0.1, n - 0.1, n + 0.01, n - 0.01, n * 2.0, n / 2.0, -n, 0.0, 1.0, -1.0, 0.5, 100.0, -100.0, 10_000.0, -10_000.0];
                tried.into_iter().filter(|other| *other != n).map(Json::Number).collect()
            }
            Json::Bool(on) => vec![Json::Bool(!on)],
            Json::String(text) => self.known.strings.iter().filter(|other| *other != text).map(|other| Json::String(other.clone())).collect(),
            _ => vec![Json::Number(0.0), Json::Bool(true), Json::String(String::from("x"))],
        }
    }

    fn compare(&self, path: &mut Vec<Step>, value: &Json, saved: Option<&Json>, found: &mut Vec<String>) {
        match (value, saved) {
            (_, None) => self.dropped(path, value, found),
            (Json::Object(fields), Some(kept @ Json::Object(_))) => {
                for (key, inner) in fields {
                    path.push(Step::Key(key.clone()));
                    self.compare(path, inner, kept.get(key), found);
                    path.pop();
                }
            }
            (Json::Array(items), Some(Json::Array(kept))) => {
                for (index, inner) in items.iter().enumerate() {
                    path.push(Step::Index(index));
                    self.compare(path, inner, kept.get(index), found);
                    path.pop();
                }
            }
            (Json::Number(was), Some(Json::Number(is))) if tidy(*was) != tidy(*is) => found.push(format!("{}: {} loads as {}", spell(path), json::number(*was), json::number(*is))),
            (Json::Number(_), Some(Json::Number(_))) => {}
            (was, Some(is)) if was != is => found.push(format!("{}: {} loads as {}", spell(path), was.write_at(0), is.write_at(0))),
            _ => {}
        }
    }

    fn dropped(&self, path: &mut Vec<Step>, value: &Json, found: &mut Vec<String>) {
        match value {
            Json::Object(fields) if !fields.is_empty() => {
                for (key, inner) in fields {
                    path.push(Step::Key(key.clone()));
                    self.dropped(path, inner, found);
                    path.pop();
                }
            }
            Json::Array(items) if !items.is_empty() => {
                for (index, inner) in items.iter().enumerate() {
                    path.push(Step::Index(index));
                    self.dropped(path, inner, found);
                    path.pop();
                }
            }
            Json::Object(_) | Json::Array(_) => {
                if path.last().is_some_and(|step| matches!(step, Step::Key(_))) && !self.loads_differently(path, Json::Number(0.0)) && !self.loads_differently(path, Json::Null) {
                    found.push(format!("{}: empty, carries nothing", spell(path)));
                }
            }
            leaf => {
                let unnamed = matches!(leaf, Json::String(text) if self.known.names.binary_search(text).is_err());
                if unnamed || !self.others(leaf).into_iter().any(|other| self.loads_differently(path, other)) {
                    found.push(format!("{}: {} is ignored, nothing reads it", spell(path), leaf.write_at(0)));
                } else if self.loads_differently(path, Json::Null) {
                    found.push(format!("{}: {} is read, but a save drops it", spell(path), leaf.write_at(0)));
                }
            }
        }
    }
}

fn moves(modulation: &Modulation) -> bool {
    let rest = Modulation::DEFAULT;
    Modulation { sample: rest.sample, sampled: rest.sampled, quantize: rest.quantize, lag_ms: rest.lag_ms, ..*modulation } != rest
}

fn unheard_mod(preset: &Preset) -> Option<String> {
    let Preset::Rack(sound) = preset else {
        return None;
    };
    let mut cores = sound.rack.iter().filter_map(|row| match row {
        Row::Core(layer) => Some(layer.patch.modulation),
        _ => None,
    });
    let first = cores.next()?;
    let aimed = sound.lfos.iter().any(|lfo| matches!(&lfo.aim, Aim::Knob { row, band, .. } if band == MOD_BAND && sound.rack.get(*row).is_some_and(Row::is_core)));
    if aimed || moves(&first) || cores.any(|modulation| moves(&modulation)) {
        return None;
    }
    let steps: Vec<String> = sound
        .rack
        .iter()
        .enumerate()
        .filter_map(|(at, row)| match row {
            Row::Midi(midi) => Some((at, midi.arp.pattern)),
            _ => None,
        })
        .flat_map(|(at, pattern)| (0..LANE_STEPS).filter(move |step| pattern.get(Lane::Mod, *step) != Lane::Mod.initial()).map(move |step| format!("rack[{at}].arp.pattern.steps[{step}]")))
        .collect();
    (!steps.is_empty()).then(|| format!("mod on {} steps ({} to {}) while every Core's MOD depth is zero", steps.len(), steps[0], steps[steps.len() - 1]))
}

fn findings(text: &str, known: &Known) -> Vec<String> {
    let Some(original) = json::parse(text) else {
        return vec![String::from("does not parse")];
    };
    let Some(Document::Preset(preset)) = Document::from_json(&original) else {
        return vec![String::from("does not load as a preset")];
    };
    let saved = preset.to_json();
    let probe = Probe { original: &original, preset: &preset, known };
    let mut found = Vec::new();
    if let Json::Object(fields) = &original {
        for (key, value) in fields.iter().filter(|(key, _)| !WORDS.contains(&key.as_str())) {
            probe.compare(&mut vec![Step::Key(key.clone())], value, saved.get(key), &mut found);
        }
    }
    found.extend(unheard_mod(&preset));
    found
}

#[test]
fn every_factory_preset_saves_back_key_for_key_and_holds_no_step_mod_that_no_core_hears() {
    let known = known();
    let found: Vec<String> = PRESETS.iter().flat_map(|(path, text)| findings(text, &known).into_iter().map(move |finding| format!("{path}: {finding}"))).collect();
    assert!(found.is_empty(), "wanted every one of the {} factory presets to load and save back every key as written, its own words aside, and no step MOD where no Core's MOD depth can hear it, got {} findings:\n{}", PRESETS.len(), found.len(), found.join("\n"));
}
