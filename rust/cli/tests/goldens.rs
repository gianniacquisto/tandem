//! The port's parity gate, Rust half: the case list in
//! `Fixtures/goldens/manifest.json`, run through the Rust binary and compared,
//! message for message, to the goldens the Swift binary captured.
//!
//! `docs/port.md` (Parity, item 1). `Scripts/capture-goldens.sh` is the other
//! half: it runs the same cases through the Swift binary and writes one file per
//! case under `Fixtures/goldens/<group>/<case>.json`. Neither half knows which
//! binary captured a golden; both read the same manifest, both normalize the same
//! way, and the comparison is on the result, not on a rendering of it.
//!
//! The rules this test shares with the capture:
//!
//! - **A fresh copy of the fixture home per case.** Nothing carries between
//!   cases, and no run sees `~/.tandem`: the child gets an `env_clear`ed
//!   environment whose `HOME` and `TANDEM_HOME` are directories this test made.
//! - **Ids, timestamps and the two sandbox paths are names, nothing else.**
//!   `<id>`, `<timestamp>`, `<tandem-home>`, `<home>`. The manifest says so, in
//!   its own `normalize` list; a value that differs between machines and is not
//!   in that list is a difference this test reports.
//! - **A case's `mask` is the case admitting what the machine owns.** `doctor`'s
//!   `app` and `skills` rows read this machine, not the fixture home, so their
//!   names still compare and their detail is `<environment>`.
//!
//! **A group the binary does not answer yet is skipped, by name.** PRT-9 lands
//! the grammar groups; `setup` and `doctor` are PRT-10's, and a group whose help
//! the binary does not answer is unrun rather than failed. The skip names the
//! group and the case count, so the gap is a line in the output and not a silent
//! pass; the first PR that answers a group turns its cases on, with no change
//! here. A group that answers and then fails is a port that regressed, and the
//! case names the verb.
//!
//! **`TANDEM_GOLDENS_BINARY` checks the harness.** The goldens are the Swift
//! binary's own answers, so pointing this test at it must match every case; that
//! is how the normalization and the comparison got verified before PRT-9, instead
//! of only after it.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::{Map, Value, json};

const BINARY: &str = env!("CARGO_BIN_EXE_tandem");

/// The binary under test: this package's own, unless `TANDEM_GOLDENS_BINARY` names
/// another. Naming the Swift binary is how the harness gets checked rather than
/// trusted: it captured the goldens, so every case must match it exactly.
fn binary() -> PathBuf {
    std::env::var_os("TANDEM_GOLDENS_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(BINARY))
}

/// Sandbox names are unique per process and per case, so two runs never share a
/// directory and a leftover from a killed run cannot be read as this one's.
static NEXT_SANDBOX: AtomicUsize = AtomicUsize::new(0);

// MARK: - Where everything lives

fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR")); // <repo>/rust/cli
    manifest
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| panic!("{} sits two levels below the repo root", manifest.display()))
        .to_path_buf()
}

fn manifest() -> Value {
    let path = repo_root().join("Fixtures/goldens/manifest.json");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("the manifest at {} is readable: {error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("the manifest at {} is JSON: {error}", path.display()))
}

fn cases(manifest: &Value) -> &[Value] {
    manifest["cases"]
        .as_array()
        .expect("the manifest carries a `cases` array")
}

fn case_name(case: &Value) -> String {
    format!(
        "{}/{}",
        case["group"].as_str().unwrap_or("?"),
        case["case"].as_str().unwrap_or("?")
    )
}

// MARK: - The grammar gate

/// Whether the binary answers a case's group at all.
///
/// `cli` is the root itself and always answers. A group answers when its
/// `--help` does. `doctor` is the exception: PRT-3's stub answers `--help` with
/// one row, so the report's own length is what tells the stub from PRT-10's eight
/// checks, and the goldens stay unrun until it is the report.
fn answers(binary: &Path, group: &str) -> bool {
    let sandbox = std::env::temp_dir();
    let home = sandbox.join(format!("tandem-goldens-probe-{}", std::process::id()));
    let run = |args: &[&str]| {
        Command::new(binary)
            .args(args)
            .env_clear()
            .env("HOME", &sandbox)
            .env("TANDEM_HOME", &home)
            .stdin(Stdio::null())
            .output()
    };
    match group {
        "cli" => true,
        "doctor" => match run(&["doctor"]) {
            Ok(output) => serde_json::from_slice::<Value>(&output.stdout)
                .ok()
                .and_then(|report| report["checks"].as_array().map(Vec::len))
                .is_some_and(|rows| rows == 8),
            Err(_) => false,
        },
        _ => run(&[group, "--help"]).is_ok_and(|output| output.status.success()),
    }
}

// MARK: - Normalization

/// Replace a sandbox path with its name, in both spellings the child may report:
/// the one this test built, and its canonicalized form when the two differ.
fn normalize_prefixes(text: &str, prefixes: &[(String, String)]) -> String {
    let mut text = text.to_owned();
    for (path, name) in prefixes {
        text = text.replace(path.as_str(), name);
    }
    text
}

fn transform(text: &str, rule: &Value, prefixes: &[(String, String)]) -> String {
    match rule["kind"].as_str().unwrap_or("") {
        "prefix" => normalize_prefixes(text, prefixes),
        "uuid" => replace_uuids(text, rule["as"].as_str().unwrap_or("<id>")),
        "timestamp" => replace_timestamps(text, rule["as"].as_str().unwrap_or("<timestamp>")),
        _ => text.to_owned(),
    }
}

fn apply_rule(value: &mut Value, rule: &Value, prefixes: &[(String, String)]) {
    match value {
        Value::String(text) => *text = transform(text, rule, prefixes),
        Value::Array(items) => items
            .iter_mut()
            .for_each(|item| apply_rule(item, rule, prefixes)),
        Value::Object(map) => map
            .values_mut()
            .for_each(|item| apply_rule(item, rule, prefixes)),
        _ => {}
    }
}

/// Normalize a document the way the manifest says, one rule at a time and in its
/// order, so both halves agree on the order the names are applied in.
fn normalize(value: &mut Value, rules: &[Value], prefixes: &[(String, String)]) {
    for rule in rules {
        apply_rule(value, rule, prefixes);
    }
}

fn hex(byte: u8) -> bool {
    byte.is_ascii_hexdigit()
}

fn digit(byte: u8) -> bool {
    byte.is_ascii_digit()
}

/// The length of the UUID at the head of `bytes`, when one starts there: five hex
/// groups of 8-4-4-4-12, each after the other, dashes between.
fn uuid_len(bytes: &[u8]) -> Option<usize> {
    const GROUPS: [usize; 5] = [8, 4, 4, 4, 12];
    let mut at = 0;
    for (index, width) in GROUPS.iter().enumerate() {
        if index > 0 {
            if bytes.get(at) != Some(&b'-') {
                return None;
            }
            at += 1;
        }
        for _ in 0..*width {
            if !bytes.get(at).is_some_and(|byte| hex(*byte)) {
                return None;
            }
            at += 1;
        }
    }
    Some(at)
}

fn replace_uuids(text: &str, name: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::new();
    let mut at = 0;
    while at < bytes.len() {
        if let Some(length) = uuid_len(&bytes[at..]) {
            out.push_str(name);
            at += length;
        } else {
            let character = text[at..].chars().next().expect("a char boundary");
            out.push(character);
            at += character.len_utf8();
        }
    }
    out
}

/// The length of an ISO-8601 UTC stamp at the head of `bytes`:
/// `2026-10-30T12:31:32Z`, an optional fraction, or the compact
/// `20261030T123132Z` a capture path carries.
fn timestamp_len(bytes: &[u8]) -> Option<usize> {
    let digits = |at: usize, width: usize| -> bool {
        (at..at + width).all(|index| bytes.get(index).is_some_and(|byte| digit(*byte)))
    };
    let dash = |at: usize, byte: u8| bytes.get(at) == Some(&byte);

    if digits(0, 4)
        && dash(4, b'-')
        && digits(5, 2)
        && dash(7, b'-')
        && digits(8, 2)
        && dash(10, b'T')
        && digits(11, 2)
        && dash(13, b':')
        && digits(14, 2)
        && dash(16, b':')
        && digits(17, 2)
    {
        let mut at = 19;
        if dash(at, b'.') {
            at += 1;
            let start = at;
            while bytes.get(at).is_some_and(|byte| digit(*byte)) {
                at += 1;
            }
            if at == start {
                return None;
            }
        }
        return (bytes.get(at) == Some(&b'Z')).then_some(at + 1);
    }

    if digits(0, 8) && dash(8, b'T') && digits(9, 6) && dash(15, b'Z') {
        return Some(16);
    }
    None
}

fn replace_timestamps(text: &str, name: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::new();
    let mut at = 0;
    while at < bytes.len() {
        if let Some(length) = timestamp_len(&bytes[at..]) {
            out.push_str(name);
            at += length;
        } else {
            let character = text[at..].chars().next().expect("a char boundary");
            out.push(character);
            at += character.len_utf8();
        }
    }
    out
}

/// A case's `mask`: the rows of a report whose detail is the machine's, not the
/// fixture home's. Their names and `ok` still compare.
fn mask(value: &mut Value, mask: &Value) {
    if mask.is_null() {
        return;
    }
    let Some(at) = mask["at"].as_str() else {
        return;
    };
    let Some(names) = mask["names"].as_array() else {
        return;
    };
    let replacement = mask["as"].as_str().unwrap_or("<environment>").to_owned();
    let Some(Value::Array(rows)) = value.get_mut(at) else {
        return;
    };
    for row in rows {
        let named = row["name"]
            .as_str()
            .is_some_and(|name| names.iter().any(|candidate| candidate == name));
        if named && let Value::Object(row) = row {
            row.insert("detail".into(), Value::String(replacement.clone()));
        }
    }
}

// MARK: - The difference

/// The first place `actual` departs from `expected`, as a key path: the case name
/// is the caller's to add, the path is what makes the failure readable.
fn first_difference(expected: &Value, actual: &Value, path: &str) -> Option<String> {
    match (expected, actual) {
        (Value::Object(expected), Value::Object(actual)) => {
            for key in expected.keys().chain(actual.keys()) {
                let here = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                match (expected.get(key), actual.get(key)) {
                    (Some(expected), Some(actual)) => {
                        if let Some(difference) = first_difference(expected, actual, &here) {
                            return Some(difference);
                        }
                    }
                    (Some(expected), None) => {
                        return Some(format!("{here}: expected {expected}, found nothing"));
                    }
                    (None, Some(actual)) => {
                        return Some(format!("{here}: expected nothing, found {actual}"));
                    }
                    (None, None) => {}
                }
            }
            None
        }
        (Value::Array(expected), Value::Array(actual)) => {
            for (index, (expected, actual)) in expected.iter().zip(actual).enumerate() {
                let here = format!("{path}[{index}]");
                if let Some(difference) = first_difference(expected, actual, &here) {
                    return Some(difference);
                }
            }
            (expected.len() != actual.len()).then(|| {
                format!(
                    "{path}: expected {} item(s), found {}",
                    expected.len(),
                    actual.len()
                )
            })
        }
        (expected, actual) => {
            (expected != actual).then(|| format!("{path}: expected {expected}, found {actual}"))
        }
    }
}

// MARK: - Running a case

struct Sandbox {
    root: PathBuf,
    home: PathBuf,
    user: PathBuf,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        let serial = NEXT_SANDBOX.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "tandem-goldens-{}-{serial}-{}",
            std::process::id(),
            name.replace('/', "-")
        ));
        let _ = fs::remove_dir_all(&root);
        let home = root.join("Home");
        let user = root.join("user");
        fs::create_dir_all(&home).expect("the sandbox home is creatable");
        fs::create_dir_all(&user).expect("the sandbox user home is creatable");
        copy_dir(&repo_root().join("Fixtures/port/Home"), &home);
        Self { root, home, user }
    }

    fn prefixes(&self) -> Vec<(String, String)> {
        let mut prefixes = Vec::new();
        for (path, name) in [(&self.home, "<tandem-home>"), (&self.user, "<home>")] {
            let spelled = path.to_string_lossy().into_owned();
            prefixes.push((spelled.clone(), name.to_owned()));
            if let Ok(canonical) = fs::canonicalize(path) {
                let canonical = canonical.to_string_lossy().into_owned();
                if canonical != spelled {
                    prefixes.push((canonical, name.to_owned()));
                }
            }
        }
        prefixes
    }

    fn environment(&self, manifest: &Value, case: &Value) -> Vec<(String, String)> {
        let face = repo_root().join(
            manifest["face"]
                .as_str()
                .unwrap_or("Fixtures/port/agent-face"),
        );
        let mut environment = Map::new();
        environment.insert("PATH".into(), json!("/usr/bin:/bin:/usr/sbin:/sbin"));
        environment.insert("TZ".into(), json!("UTC"));
        environment.insert("HOME".into(), json!(self.user.to_string_lossy()));
        environment.insert("TANDEM_HOME".into(), json!(self.home.to_string_lossy()));
        environment.insert("TANDEM_AGENT_BUNDLE".into(), json!(face.to_string_lossy()));
        for (key, value) in manifest["env"].as_object().into_iter().flatten() {
            let value = match value.as_str() {
                Some("$face") => json!(face.to_string_lossy()),
                Some(other) => json!(other),
                None => continue,
            };
            environment.insert(key.clone(), value);
        }
        // A null value means the case runs with the variable absent, which is a
        // shape of its own (`session start` without a session id).
        for (key, value) in case["env"].as_object().into_iter().flatten() {
            match value {
                Value::Null => {
                    environment.remove(key);
                }
                other => {
                    environment.insert(key.clone(), other.clone());
                }
            }
        }
        environment
            .into_iter()
            .filter_map(|(key, value)| value.as_str().map(|value| (key, value.to_owned())))
            .collect()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn copy_dir(from: &Path, to: &Path) {
    let entries = fs::read_dir(from)
        .unwrap_or_else(|error| panic!("{} is readable: {error}", from.display()));
    for entry in entries {
        let entry = entry.expect("a directory entry");
        let target = to.join(entry.file_name());
        let kind = entry.file_type().expect("a file type");
        if kind.is_dir() {
            fs::create_dir_all(&target).expect("a copied directory");
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap_or_else(|error| {
                panic!(
                    "{} copies to {}: {error}",
                    entry.path().display(),
                    target.display()
                )
            });
        }
    }
}

struct Outcome {
    exit: i32,
    stdout: String,
    stderr: String,
}

fn run(argv: &[Value], stdin: &str, environment: &[(String, String)], cwd: &Path) -> Outcome {
    let arguments: Vec<&str> = argv.iter().filter_map(Value::as_str).collect();
    let binary = binary();
    let mut child = Command::new(&binary)
        .args(&arguments)
        .current_dir(cwd)
        .env_clear()
        .envs(environment.iter().map(|(key, value)| (key, value)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("{} runs `{arguments:?}`: {error}", binary.display()));
    child
        .stdin
        .as_mut()
        .expect("a piped stdin")
        .write_all(stdin.as_bytes())
        .expect("the case's stdin is writable");
    let output = child.wait_with_output().expect("the run finishes");
    Outcome {
        exit: output.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

/// A run's output as the golden records it: JSON when the CLI answered JSON, one
/// entry per line otherwise.
fn lines(text: &str, rules: &[Value], prefixes: &[(String, String)]) -> Vec<Value> {
    let mut lines: Vec<&str> = text.split('\n').collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    lines
        .into_iter()
        .map(|line| {
            let mut value = Value::String(line.to_owned());
            normalize(&mut value, rules, prefixes);
            value
        })
        .collect()
}

fn stdout_value(
    outcome: &Outcome,
    rules: &[Value],
    prefixes: &[(String, String)],
) -> (Value, &'static str) {
    match serde_json::from_str::<Value>(&outcome.stdout) {
        Ok(mut parsed) if parsed.is_object() || parsed.is_array() => {
            normalize(&mut parsed, rules, prefixes);
            (parsed, "json")
        }
        _ => (
            Value::Array(lines(&outcome.stdout, rules, prefixes)),
            "text",
        ),
    }
}

fn setup_steps(case: &Value, sandbox: &Sandbox, environment: &[(String, String)]) {
    for step in case["setup"].as_array().into_iter().flatten() {
        if let Some(argv) = step["argv"].as_array() {
            let stdin = step["stdin"].as_str().unwrap_or("");
            let outcome = run(argv, stdin, environment, &sandbox.home);
            assert_eq!(
                outcome.exit,
                0,
                "{}: a setup step failed: {}\n{}",
                case_name(case),
                step,
                outcome.stderr
            );
        } else if let Some(path) = step["rm"].as_str() {
            let target = sandbox.home.join(path);
            if target.is_dir() {
                fs::remove_dir_all(&target).expect("a setup removal");
            } else {
                fs::remove_file(&target).expect("a setup removal");
            }
        } else if let Some(write) = step["write"].as_object() {
            let target = sandbox
                .home
                .join(step["write"]["at"].as_str().unwrap_or(""));
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).expect("a setup directory");
            }
            fs::write(&target, write["text"].as_str().unwrap_or(""))
                .unwrap_or_else(|error| panic!("{} writes: {error}", target.display()));
        } else {
            panic!("{}: unknown setup step {step}", case_name(case));
        }
    }
}

// MARK: - The tests

/// The gate itself: while the Rust binary answers no grammar, the goldens are
/// unrun and the skip says why.
#[test]
fn goldens_match_the_binary() {
    let manifest = manifest();
    let rules = manifest["normalize"]
        .as_array()
        .expect("the manifest carries a `normalize` list")
        .clone();
    let goldens = repo_root().join("Fixtures/goldens");
    let mut failures = Vec::new();

    let mut skipped: Vec<String> = Vec::new();
    for case in cases(&manifest) {
        let group = case["group"].as_str().unwrap_or("?");
        if !answers(&binary(), group) {
            skipped.push(group.to_string());
            continue;
        }
        let name = case_name(case);
        let sandbox = Sandbox::new(&name);
        let environment = sandbox.environment(&manifest, case);
        setup_steps(case, &sandbox, &environment);

        let argv = case["argv"].as_array().expect("a case's argv is an array");
        let stdin = case["stdin"].as_str().unwrap_or("");
        let outcome = run(argv, stdin, &environment, &sandbox.home);

        let mut actual = json!({
            "argv": argv,
            "stdin": stdin,
            "exit": outcome.exit,
            "stderr": lines(&outcome.stderr, &rules, &sandbox.prefixes()),
        });
        let (mut stdout, kind) = stdout_value(&outcome, &rules, &sandbox.prefixes());
        // The case's `mask` names rows of the answer, so it goes on `stdout`, the
        // one place the capture applies it too.
        mask(&mut stdout, &case["mask"]);
        actual["stdoutKind"] = json!(kind);
        actual["stdout"] = stdout;

        let path = goldens.join(format!("{name}.json"));
        let Ok(text) = fs::read_to_string(&path) else {
            failures.push(format!("{name}: no golden at {}", path.display()));
            continue;
        };
        let expected: Value = serde_json::from_str(&text)
            .unwrap_or_else(|error| panic!("{} is JSON: {error}", path.display()));

        // The golden records the invocation it was captured from, so a manifest
        // that changed without a recapture is a failure and not a stale read.
        let invocation = json!({
            "argv": case["argv"],
            "stdin": stdin,
            "exit": case["exit"].as_i64().unwrap_or(0),
        });
        for key in ["argv", "stdin", "exit"] {
            if expected[key] != invocation[key] {
                failures.push(format!(
                    "{name}: the golden was captured from a different invocation, recapture it \
                     with Scripts/capture-goldens.sh ({}: expected {}, the manifest says {})",
                    key, expected[key], invocation[key]
                ));
            }
        }

        if let Some(difference) = first_difference(&expected, &actual, "") {
            failures.push(format!("{name}: {difference}"));
        }
    }

    skipped.sort();
    skipped.dedup();
    if !skipped.is_empty() {
        eprintln!(
            "skipped: the binary does not answer {} yet, so those cases stay unrun; a later PR \
             turns them on with no change here.",
            skipped
                .iter()
                .map(|group| format!("`{group}`"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    assert!(
        failures.is_empty(),
        "{} of {} case(s) differ from the goldens:\n{}",
        failures.len(),
        cases(&manifest).len(),
        failures.join("\n")
    );
}

#[test]
fn a_uuid_is_its_name() {
    let text =
        "ticket 52DF0625-B688-42EE-B94F-4F20F9B85E1C and 52DF0625-B688-42EE-B94F-4F20F9B85E1 too";
    assert_eq!(
        replace_uuids(text, "<id>"),
        "ticket <id> and 52DF0625-B688-42EE-B94F-4F20F9B85E1 too"
    );
    assert_eq!(replace_uuids("no ids here", "<id>"), "no ids here");
}

#[test]
fn a_timestamp_is_its_name() {
    let text =
        "opened 2026-10-30T12:31:32Z, filed 2026-10-30T12:31:32.250Z, path 20261030T123132Z-fact";
    assert_eq!(
        replace_timestamps(text, "<timestamp>"),
        "opened <timestamp>, filed <timestamp>, path <timestamp>-fact"
    );
    assert_eq!(
        replace_timestamps("dated 2026-10-30 only", "<timestamp>"),
        "dated 2026-10-30 only",
        "a date is not a stamp: the fixture home's daily note keeps it"
    );
}

#[test]
fn the_sandbox_is_its_name() {
    let prefixes = vec![("/tmp/case/Home".to_owned(), "<tandem-home>".to_owned())];
    assert_eq!(
        normalize_prefixes("/tmp/case/Home/bikeshed is a file", &prefixes),
        "<tandem-home>/bikeshed is a file"
    );
}

#[test]
fn the_first_difference_names_the_key_path() {
    let expected = json!({"tickets": [{"title": "Fixture ticket", "id": "<id>"}]});
    let actual = json!({"tickets": [{"title": "Other ticket", "id": "<id>"}]});
    assert_eq!(
        first_difference(&expected, &actual, "").as_deref(),
        Some(r#"tickets[0].title: expected "Fixture ticket", found "Other ticket""#)
    );

    let missing = json!({"tickets": []});
    assert_eq!(
        first_difference(&expected, &missing, "").as_deref(),
        Some("tickets: expected 1 item(s), found 0")
    );
    assert_eq!(first_difference(&expected, &expected, ""), None);
}

#[test]
fn a_mask_replaces_the_named_rows() {
    let mut report = json!({"checks": [
        {"name": "home", "ok": true, "detail": "<tandem-home> (TANDEM_HOME override)"},
        {"name": "app", "ok": true, "detail": "installed at /Applications/Tandem.app"},
    ]});
    mask(
        &mut report,
        &json!({"at": "checks", "names": ["app"], "as": "<environment>"}),
    );
    assert_eq!(report["checks"][1]["detail"], json!("<environment>"));
    assert_eq!(
        report["checks"][0]["detail"],
        json!("<tandem-home> (TANDEM_HOME override)"),
        "a row the case does not name keeps its message"
    );
    assert_eq!(
        report["checks"][1]["name"],
        json!("app"),
        "the name still compares"
    );
}
