#![cfg(target_os = "macos")]
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

const SHELL: &str = "../Baking Powder/mac/src";
const CRASHED: &str = "beginDraggingSession:event:source:";
const FIXED: &str = "beginDraggingSessionWithItems:event:source:";
const FOLLOW: usize = 4;

const QUERY: &str = r#"
#include <objc/runtime.h>
#include <stdio.h>
#include <stdlib.h>

#define MOST 4096

static int count;
static SEL wanted[MOST];
static const char *answer[MOST];

static void mark(SEL found, const char *by) {
    for (int i = 0; i < count; i++) {
        if (!answer[i] && wanted[i] == found) {
            answer[i] = by;
        }
    }
}

static void methods_of(Class cls, const char *by) {
    unsigned n = 0;
    Method *list = class_copyMethodList(cls, &n);
    for (unsigned i = 0; i < n; i++) {
        mark(method_getName(list[i]), by);
    }
    free(list);
}

static void methods_declared_by(Protocol *protocol, BOOL required, BOOL instance) {
    unsigned n = 0;
    struct objc_method_description *list = protocol_copyMethodDescriptionList(protocol, required, instance, &n);
    for (unsigned i = 0; i < n; i++) {
        mark(list[i].name, protocol_getName(protocol));
    }
    free(list);
}

int main(int argc, char **argv) {
    count = argc - 1 < MOST ? argc - 1 : MOST;
    for (int i = 0; i < count; i++) {
        wanted[i] = sel_registerName(argv[i + 1]);
    }
    int room = objc_getClassList(NULL, 0);
    Class *classes = calloc((size_t)room, sizeof(Class));
    int total = objc_getClassList(classes, room);
    if (total > room) {
        total = room;
    }
    for (int i = 0; i < total; i++) {
        methods_of(classes[i], class_getName(classes[i]));
        methods_of(object_getClass(classes[i]), class_getName(classes[i]));
    }
    unsigned declared = 0;
    Protocol **protocols = objc_copyProtocolList(&declared);
    for (unsigned i = 0; i < declared; i++) {
        methods_declared_by(protocols[i], YES, YES);
        methods_declared_by(protocols[i], YES, NO);
        methods_declared_by(protocols[i], NO, YES);
        methods_declared_by(protocols[i], NO, NO);
    }
    free(protocols);
    free(classes);
    for (int i = 0; i < count; i++) {
        printf("%s\t%s\n", sel_getName(wanted[i]), answer[i] ? answer[i] : "");
    }
    return 0;
}
"#;

type Site = String;

struct Call {
    file: String,
    name: String,
    args: Vec<String>,
    within: String,
    parents: [String; 2],
    site: Site,
}

#[derive(Default)]
struct Shell {
    calls: Vec<Call>,
    params: BTreeMap<(String, String), Vec<String>>,
    consts: BTreeMap<String, String>,
}

#[derive(Default)]
struct Scan {
    sent: Vec<(String, Site)>,
    registered: Vec<(String, Site)>,
    unfollowed: Vec<(String, Site)>,
}

struct Frame<'a> {
    opener: u8,
    name: &'a str,
    definition: bool,
    open_at: usize,
    arg_start: usize,
    args: Vec<&'a str>,
}

fn shell_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(SHELL)
}

fn sources() -> Vec<(String, String)> {
    let dir = shell_dir();
    let entries = fs::read_dir(&dir).unwrap_or_else(|error| panic!("wanted to list {}: {error}", dir.display()));
    let mut files: Vec<PathBuf> = entries.map(|entry| entry.expect("wanted an entry of the shell folder").path()).filter(|path| path.extension().is_some_and(|extension| extension == "rs")).collect();
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
            let text = fs::read_to_string(&path).unwrap_or_else(|error| panic!("wanted to read {}: {error}", path.display()));
            (name, text)
        })
        .collect()
}

fn frameworks(sources: &[(String, String)]) -> BTreeSet<String> {
    sources
        .iter()
        .flat_map(|(_, text)| text.lines())
        .filter_map(|line| {
            let (name, tail) = line.trim().strip_prefix("#[link(name = \"")?.split_once('"')?;
            tail.contains("kind = \"framework\"").then(|| name.to_string())
        })
        .collect()
}

fn literal(expr: &str) -> Option<String> {
    expr.trim().strip_prefix("c\"")?.strip_suffix('"').map(String::from)
}

fn consts(text: &str, found: &mut BTreeMap<String, String>) {
    for line in text.lines() {
        let line = line.trim();
        let line = line.strip_prefix("pub(crate) ").or_else(|| line.strip_prefix("pub ")).unwrap_or(line);
        let Some((head, value)) = line.strip_prefix("const ").and_then(|rest| rest.split_once('=')) else {
            continue;
        };
        if let (Some(name), Some(text)) = (head.split(':').next(), literal(value.trim().trim_end_matches(';'))) {
            found.insert(name.trim().to_string(), text);
        }
    }
}

fn string_end(bytes: &[u8], at: usize) -> usize {
    let mut i = at + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return i + 1,
            _ => i += 1,
        }
    }
    i
}

fn quote_end(text: &str, at: usize) -> usize {
    let rest = &text[at + 1..];
    if rest.starts_with('\\') {
        return rest.get(2..).and_then(|tail| tail.find('\'')).map_or(text.len(), |end| at + 4 + end);
    }
    match rest.chars().next() {
        Some(c) if rest[c.len_utf8()..].starts_with('\'') => at + 2 + c.len_utf8(),
        _ => at + 1,
    }
}

fn identifier_before(text: &str, open: usize) -> &str {
    let bytes = text.as_bytes();
    let mut start = open;
    if start > 0 && bytes[start - 1] == b'!' {
        start -= 1;
    }
    while start > 0 && (bytes[start - 1].is_ascii_alphanumeric() || bytes[start - 1] == b'_') {
        start -= 1;
    }
    &text[start..open]
}

fn preceded_by_fn(text: &str, start: usize) -> bool {
    let before = text[..start].trim_end();
    before.ends_with("fn") && !before[..before.len() - 2].ends_with(|c: char| c.is_alphanumeric() || c == '_')
}

fn param_name(param: &str) -> String {
    param.split(':').next().unwrap_or("").trim().trim_start_matches("mut ").to_string()
}

fn frame_name(frame: &Frame) -> String {
    match frame.opener {
        b'[' => String::from("["),
        b'{' => String::from("{"),
        _ if frame.definition => format!("fn {}", frame.name),
        _ => frame.name.to_string(),
    }
}

fn line_of(text: &str, at: usize) -> usize {
    text[..at].bytes().filter(|&byte| byte == b'\n').count() + 1
}

fn parse(file: &str, text: &str, shell: &mut Shell) {
    consts(text, &mut shell.consts);
    let bytes = text.as_bytes();
    let mut stack: Vec<Frame> = Vec::new();
    let mut within = "";
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                i = string_end(bytes, i);
                continue;
            }
            b'\'' => {
                i = quote_end(text, i);
                continue;
            }
            opener @ (b'(' | b'[' | b'{') => {
                let name = if opener == b'(' { identifier_before(text, i) } else { "" };
                let definition = !name.is_empty() && preceded_by_fn(text, i - name.len());
                stack.push(Frame { opener, name, definition, open_at: i, arg_start: i + 1, args: Vec::new() });
            }
            b',' => {
                if let Some(frame) = stack.last_mut() {
                    frame.args.push(&text[frame.arg_start..i]);
                    frame.arg_start = i + 1;
                }
            }
            b')' | b']' | b'}' => {
                if let Some(mut frame) = stack.pop() {
                    let last = &text[frame.arg_start..i];
                    if !last.trim().is_empty() {
                        frame.args.push(last);
                    }
                    if frame.opener == b'(' && frame.definition {
                        shell.params.insert((file.to_string(), frame.name.to_string()), frame.args.iter().map(|param| param_name(param)).collect());
                        within = frame.name;
                    } else if frame.opener == b'(' {
                        let parent = |depth: usize| stack.len().checked_sub(depth).map_or_else(String::new, |at| frame_name(&stack[at]));
                        shell.calls.push(Call {
                            file: file.to_string(),
                            name: frame.name.to_string(),
                            args: frame.args.iter().map(|arg| arg.trim().to_string()).collect(),
                            within: within.to_string(),
                            parents: [parent(1), parent(2)],
                            site: format!("{file}:{}", line_of(text, frame.open_at)),
                        });
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }
}

fn follow(expr: &str, call: &Call, shell: &Shell, depth: usize, scan: &mut Scan) {
    if let Some(name) = literal(expr) {
        scan.sent.push((name, call.site.clone()));
        return;
    }
    if expr.starts_with('$') {
        return;
    }
    if let Some(name) = shell.consts.get(expr) {
        scan.sent.push((name.clone(), call.site.clone()));
        return;
    }
    let params = shell.params.get(&(call.file.clone(), call.within.clone()));
    if let Some(at) = params.and_then(|params| params.iter().position(|param| param == expr))
        && depth > 0
    {
        for caller in shell.calls.iter().filter(|caller| caller.name == call.within && caller.args.len() > at) {
            follow(&caller.args[at], caller, shell, depth - 1, scan);
        }
        return;
    }
    scan.unfollowed.push((expr.to_string(), call.site.clone()));
}

fn scan(sources: &[(String, String)]) -> Scan {
    let mut shell = Shell::default();
    for (file, text) in sources {
        parse(file, text, &mut shell);
    }
    let mut scan = Scan::default();
    for call in &shell.calls {
        match (call.name.as_str(), call.args.len()) {
            ("msg!", 2..) => follow(&call.args[1], call, &shell, FOLLOW, &mut scan),
            ("sel", 1) if call.parents[0] != "class_addMethod" => follow(&call.args[0], call, &shell, FOLLOW, &mut scan),
            ("", 3) if call.parents[0] == "[" && call.parents[1] == "subclass" => match literal(&call.args[0]) {
                Some(name) => scan.registered.push((name, call.site.clone())),
                None => scan.unfollowed.push((call.args[0].clone(), call.site.clone())),
            },
            _ => {}
        }
    }
    scan
}

fn query() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY.get_or_init(|| {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"));
        fs::create_dir_all(dir).unwrap_or_else(|error| panic!("wanted {}: {error}", dir.display()));
        let source = dir.join("selectors.c");
        let binary = dir.join("selectors");
        fs::write(&source, QUERY).unwrap_or_else(|error| panic!("wanted to write {}: {error}", source.display()));
        let mut cc = Command::new("cc");
        cc.arg("-o").arg(&binary).arg(&source).arg("-lobjc");
        for framework in frameworks(&sources()) {
            cc.arg("-framework").arg(framework);
        }
        let output = cc.output().unwrap_or_else(|error| panic!("wanted cc to compile the runtime query: {error}"));
        assert!(output.status.success(), "wanted the runtime query compiled, found cc saying:\n{}", String::from_utf8_lossy(&output.stderr));
        binary
    })
}

fn answered(selectors: &BTreeSet<String>) -> BTreeMap<String, Option<String>> {
    let output = Command::new(query()).args(selectors).output().unwrap_or_else(|error| panic!("wanted the runtime query to run: {error}"));
    assert!(output.status.success(), "wanted the runtime query to finish, found it saying:\n{}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once('\t'))
        .map(|(name, by)| (name.to_string(), (!by.is_empty()).then(|| by.to_string())))
        .collect()
}

fn unknown(found: &[(String, Site)], known_to_the_shell: &BTreeSet<String>) -> Vec<String> {
    let names: BTreeSet<String> = found.iter().map(|(name, _)| name.clone()).collect();
    let answers = answered(&names);
    found
        .iter()
        .filter(|(name, _)| !known_to_the_shell.contains(name) && answers.get(name).is_none_or(Option::is_none))
        .map(|(name, site)| format!("{site}: {name}"))
        .collect()
}

fn names(found: &[(String, Site)]) -> BTreeSet<String> {
    found.iter().map(|(name, _)| name.clone()).collect()
}

fn listed(found: &[(String, Site)]) -> String {
    found.iter().map(|(name, site)| format!("{site}: {name}")).collect::<Vec<String>>().join("\n")
}

#[test]
fn every_selector_the_shell_sends_is_answered_by_its_frameworks_or_registered_by_the_shell() {
    let scan = scan(&sources());
    assert!(scan.unfollowed.is_empty(), "wanted every selector the Mac shell sends written as a c\"...\" literal or passed through a &CStr parameter, could not follow {}:\n{}", scan.unfollowed.len(), listed(&scan.unfollowed));
    assert!(!scan.sent.is_empty(), "wanted selectors sent from {}, found none", shell_dir().display());
    let unknown = unknown(&scan.sent, &names(&scan.registered));
    assert!(unknown.is_empty(), "wanted every selector the Mac shell sends answered by a class or protocol of the frameworks it links, or registered on its own classes, found {} of {} unknown:\n{}", unknown.len(), scan.sent.len(), unknown.join("\n"));
}

#[test]
fn every_selector_the_shell_registers_is_one_its_frameworks_call_or_it_sends_itself() {
    let scan = scan(&sources());
    assert!(!scan.registered.is_empty(), "wanted selectors registered through subclass in {}, found none", shell_dir().display());
    let unknown = unknown(&scan.registered, &names(&scan.sent));
    assert!(unknown.is_empty(), "wanted every selector the Mac shell registers on its classes to be one its frameworks call or one it sends itself, found {} of {} unknown:\n{}", unknown.len(), scan.registered.len(), unknown.join("\n"));
}

#[test]
fn the_drag_selector_the_shell_crashed_on_is_reported_unknown_at_its_line_where_the_fixed_one_is_answered() {
    let drag = format!("fn drag_out(view: id, items: id, event: id) {{\n    let _: id = msg!(view, c\"{CRASHED}\", items, event, view);\n    let _: id = msg!(view, c\"{FIXED}\", items, event, view);\n}}\n");
    let scan = scan(&[(String::from("drag.rs"), drag)]);
    assert_eq!(scan.sent.len(), 2, "wanted both drag selectors found, found:\n{}", listed(&scan.sent));
    let unknown = unknown(&scan.sent, &BTreeSet::new());
    assert_eq!(unknown, [format!("drag.rs:2: {CRASHED}")], "wanted only the selector the shell crashed on reported, at its line");
}
