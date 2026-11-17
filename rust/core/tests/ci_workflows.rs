//! The workflows' `runner.os` conditions name an OS value.
//!
//! The OS context is `Linux`, `macOS` or `Windows`; `ubuntu-latest` is a runner
//! label, so `runner.os == 'ubuntu-latest'` is false on every job and the step
//! it guards is skipped everywhere. RET-91's retired-path scan lost its caller
//! when `app.yml` was deleted, and the guard restored in `rust.yml` was spelled
//! with the label: the check sat in the file and never ran (gate finding on
//! #431). A conditional is one line of config, so this reads the files.

use std::path::{Path, PathBuf};

fn workflows() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/workflows")
}

/// Every quoted value compared with `runner.os` in one workflow file, in file
/// order.
fn os_values(text: &str) -> Vec<String> {
    let mut values = Vec::new();
    for line in text.lines() {
        let mut rest = line;
        while let Some(at) = rest.find("runner.os") {
            rest = &rest[at + "runner.os".len()..];
            let Some(open) = rest.find('\'') else { break };
            let after = &rest[open + 1..];
            let Some(close) = after.find('\'') else { break };
            values.push(after[..close].to_string());
            rest = &after[close + 1..];
        }
    }
    values
}

#[test]
fn every_runner_os_condition_names_an_os_value() {
    let mut checked = 0;
    let entries = std::fs::read_dir(workflows()).expect("the workflow folder reads");
    for entry in entries {
        let path = entry.expect("the folder entry reads").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("yml") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("the workflow reads");
        for value in os_values(&text) {
            assert!(
                matches!(value.as_str(), "Linux" | "macOS" | "Windows"),
                "{}: `runner.os` is compared with {value:?}, a runner label, not an OS value, \
                 so the step is skipped on every job",
                path.display()
            );
            checked += 1;
        }
    }
    assert!(
        checked >= 3,
        "the workflows still guard steps by the OS context ({checked} conditions read)"
    );
}

#[test]
fn the_retired_path_scan_is_guarded_by_the_macos_os() {
    let text = std::fs::read_to_string(workflows().join("rust.yml")).expect("rust.yml reads");
    assert!(
        text.contains("run: bash Scripts/check-retired-paths.sh"),
        "the retained scan still has its step"
    );
    assert!(
        text.contains("&& runner.os == 'macOS'"),
        "the scan's step runs on the macOS job: `runner.os` is `macOS`, not a runner label"
    );
}

/// CUT-16 removed the renderer tree and the Node toolchain. A workflow that sets Node up,
/// installs from a lockfile or runs an npm script is a second toolchain the
/// repository no longer has, and it fails before it reaches the cargo gates.
#[test]
fn no_workflow_invokes_node_or_npm() {
    let banned = [
        "actions/setup-node",
        "npm ci",
        "npm run",
        "npm test",
        "npm version",
        "npx ",
    ];
    let mut checked = 0;
    for entry in std::fs::read_dir(workflows()).expect("the workflow folder reads") {
        let path = entry.expect("the folder entry reads").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("yml") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("the workflow reads");
        for command in banned {
            assert!(
                !text.contains(command),
                "{} still runs `{command}`: the Node toolchain was removed with the renderer tree (CUT-15, CUT-16)",
                path.display()
            );
        }
        checked += 1;
    }
    assert!(checked >= 2, "the workflows are read ({checked} files)");
}

/// One workflow step: its `name:` and every line that follows until the next
/// step, so a test can read what a named step actually runs.
fn steps(text: &str) -> Vec<(&str, String)> {
    let mut steps: Vec<(&str, String)> = Vec::new();
    let mut current: Option<(&str, String)> = None;
    for line in text.lines() {
        if let Some(name) = line.trim_start().strip_prefix("- name:") {
            if let Some(finished) = current.take() {
                steps.push(finished);
            }
            current = Some((name.trim().trim_matches('"'), String::new()));
        } else if let Some((_, body)) = current.as_mut() {
            body.push_str(line);
            body.push('\n');
        }
    }
    if let Some(finished) = current.take() {
        steps.push(finished);
    }
    steps
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The release pipeline CUT-16 delivers, as one matrix job (CUT-32): one
/// archive per platform, a smoke test that extracts that archive and runs the
/// binary inside it, and one aggregator that keeps the required check name
/// `release`. Adding a target is one matrix row, so the job is defined once.
#[test]
fn the_release_workflow_builds_three_platform_archives() {
    // A Windows checkout may carry CRLF line endings.
    let text = std::fs::read_to_string(workflows().join("release.yml"))
        .expect("release.yml reads")
        .replace("\r\n", "\n");
    assert_eq!(
        text.matches("\n  build:\n").count(),
        1,
        "release.yml defines one `build` job"
    );
    for retired in ["build-macos:", "build-linux:", "build-windows:"] {
        assert!(
            !text.contains(retired),
            "release.yml still copies `{retired}`"
        );
    }
    assert!(
        text.contains("needs: [changes, build]"),
        "the `release` aggregator waits for the build matrix"
    );
    for row in [
        "target: darwin-arm64",
        "target: linux-x86_64",
        "target: windows-x86_64",
    ] {
        assert!(text.contains(row), "the build matrix has the row `{row}`");
    }
    assert_eq!(
        text.matches("target: ").count(),
        3,
        "the build matrix has exactly three rows"
    );
    for archive in [
        "tandem-$version-darwin-arm64.tar.gz",
        "tandem-$version-linux-x86_64.tar.gz",
        "tandem-$version-windows-x86_64.zip",
    ] {
        assert!(text.contains(archive), "release.yml verifies `{archive}`");
    }
    assert!(
        text.contains("Scripts/stage-release-assets.sh"),
        "release.yml stages install.sh and SHA256SUMS with the stager"
    );
    assert!(
        text.contains("cargo build --release -p tandem"),
        "release.yml builds the one binary with cargo"
    );
}

/// The gate's finding on #433: the smoke steps ran the build tree, so a broken
/// tar or zip was caught only after a user downloaded it. The build job must
/// extract the archive it just wrote and smoke the binary that archive carries:
/// the unix step in place, the Windows step through `Scripts/smoke-binary.ps1`.
#[test]
fn every_smoke_step_extracts_its_archive_and_runs_the_extracted_binary() {
    let text = std::fs::read_to_string(workflows().join("release.yml")).expect("release.yml reads");
    let mut smoke_steps = 0;
    for (name, body) in steps(&text) {
        if !name.contains("Smoke:") {
            continue;
        }
        smoke_steps += 1;
        assert!(
            !body.contains("target/release") && !body.contains("target\\release"),
            "`{name}` smokes the build tree, not the archive the tag publishes"
        );
        if body.contains("smoke-binary.ps1") {
            assert!(
                body.contains("dist"),
                "`{name}` does not read the archive the job packaged into dist"
            );
            continue;
        }
        assert!(
            body.contains("tar -xzf"),
            "`{name}` does not extract the archive before it smokes it"
        );
        assert!(
            body.contains("dist/tandem-$version-${{ matrix.target }}."),
            "`{name}` does not read the archive the job packaged into dist"
        );
        assert!(
            body.contains("$smoke/tandem"),
            "`{name}` does not run the binary the archive carried"
        );
    }
    assert_eq!(
        smoke_steps, 2,
        "release.yml smokes each archive kind ({smoke_steps} smoke steps read)"
    );
    let script = std::fs::read_to_string(repo_root().join("Scripts/smoke-binary.ps1"))
        .expect("Scripts/smoke-binary.ps1 reads");
    assert!(
        script.contains("Expand-Archive"),
        "the Windows smoke extracts the zip"
    );
    assert!(
        !script.contains("target\\release"),
        "the Windows smoke runs the extracted exe"
    );
}

/// A filter must never let a change to a workflow, an action, `install.sh` or
/// the Rust tree skip its own gates, and a docs-only change must skip them.
/// The script runs on the Linux `changes` job; the Windows leg has no bash
/// contract to pin.
#[cfg(not(windows))]
#[test]
fn the_changed_paths_filter_runs_the_gates_for_code_and_skips_docs() {
    let dir = std::env::temp_dir().join(format!("tandem-changed-paths-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(&dir)
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    git(&["init", "-q"]);
    std::fs::write(dir.join("seed"), "x").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-qm", "seed"]);
    let script = repo_root().join("Scripts/changed-paths.sh");
    let prefixes = ["Resources/", "LICENSE"];
    let answer = |event: &str| {
        let out = std::process::Command::new("bash")
            .arg(&script)
            .args(prefixes)
            .env("GITHUB_EVENT_NAME", event)
            .current_dir(&dir)
            .output()
            .expect("the script runs");
        assert!(out.status.success());
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    };
    let touch = |path: &str| {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, path).unwrap();
        git(&["add", "-A"]);
        git(&["commit", "-qm", path]);
    };
    touch("Resources/pic.jpg");
    assert_eq!(
        answer("pull_request"),
        "false",
        "an allowlisted change skips"
    );
    touch("LICENSE");
    assert_eq!(
        answer("pull_request"),
        "false",
        "an exact allowlisted file skips"
    );
    for path in [
        "rust/core/src/lib.rs",
        "Scripts/install.sh",
        "Fixtures/f.json",
        ".github/workflows/rust.yml",
        ".github/actions/rust-setup/action.yml",
        "docs/a.md",
        "examples/skills/x.md",
        "README.md",
        "GLOSSARY.md",
        "NOTICE.txt",
        "AGENTS.md",
        "brand-new/dir/file",
    ] {
        touch(path);
        assert_eq!(answer("pull_request"), "true", "{path} runs the gates");
    }
    touch("docs/b.md");
    assert_eq!(
        answer("push"),
        "true",
        "a push is already filtered by `on.push.paths-ignore`"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The gates fail open (CUT-32): a `changes` output that is empty, because
/// `Scripts/changed-paths.sh` is missing or crashed, must still run them. So
/// no condition may demand `== 'true'`, and the setup action runs unless the
/// answer is exactly `false`.
#[test]
fn an_empty_changes_output_still_runs_the_gates() {
    for name in ["rust.yml", "release.yml"] {
        let text = std::fs::read_to_string(workflows().join(name)).expect("workflow reads");
        assert!(
            !text.contains("outputs.code == 'true'"),
            "{name}: a gate guarded on `== 'true'` is skipped when the script prints nothing"
        );
        assert!(
            text.contains("outputs.code != 'false'"),
            "{name}: the gates are guarded on `!= 'false'`"
        );
    }
    let action = std::fs::read_to_string(repo_root().join(".github/actions/rust-setup/action.yml"))
        .expect("the setup action reads");
    assert!(
        !action.contains("inputs.enabled == 'true'")
            && action.contains("inputs.enabled != 'false'"),
        "the setup runs unless it is told `false`"
    );
}

/// The script half of the pin: a missing script never answers `false`, and a
/// PR that deletes the script answers `true`. The script runs on the Linux
/// `changes` job; the Windows leg has no bash contract to pin.
#[cfg(not(windows))]
#[test]
fn a_missing_or_deleted_filter_script_never_answers_false() {
    // A missing script never prints `false` (it prints nothing, or on some
    // hosts an error), so the step's output is not `false` and the gates run.
    let out = std::process::Command::new("bash")
        .arg(repo_root().join("Scripts/no-such-changed-paths.sh"))
        .output()
        .expect("bash runs");
    assert_ne!(String::from_utf8_lossy(&out.stdout).trim(), "false");

    // A pull request that deletes the script is not on the allowlist, so the
    // script itself answers `true` for it.
    let dir = std::env::temp_dir().join(format!("tandem-deleted-script-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("Scripts")).unwrap();
    let git = |args: &[&str]| {
        let out = std::process::Command::new("git")
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .current_dir(&dir)
            .output()
            .expect("git runs");
        assert!(out.status.success(), "git {args:?}");
    };
    git(&["init", "-q"]);
    std::fs::write(dir.join("Scripts/changed-paths.sh"), "x").unwrap();
    git(&["add", "-A"]);
    git(&["commit", "-qm", "seed"]);
    git(&["rm", "-q", "Scripts/changed-paths.sh"]);
    git(&["commit", "-qm", "delete the script"]);
    let out = std::process::Command::new("bash")
        .arg(repo_root().join("Scripts/changed-paths.sh"))
        .args(["Resources/", "LICENSE"])
        .env("GITHUB_EVENT_NAME", "pull_request")
        .current_dir(&dir)
        .output()
        .expect("the script runs");
    assert_eq!(String::from_utf8(out.stdout).unwrap().trim(), "true");
}
