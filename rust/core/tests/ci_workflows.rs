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
        text.contains("if: runner.os == 'macOS'"),
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

/// The release pipeline CUT-16 delivers: one archive per platform, a smoke test
/// of the extracted binary, and one aggregator that keeps the required check
/// name `release`.
#[test]
fn the_release_workflow_builds_three_platform_archives() {
    let text = std::fs::read_to_string(workflows().join("release.yml")).expect("release.yml reads");
    for job in ["build-macos:", "build-linux:", "build-windows:"] {
        assert!(text.contains(job), "release.yml still names `{job}`");
    }
    assert!(
        text.contains("needs: [build-macos, build-linux, build-windows]"),
        "the `release` aggregator waits for all three builds"
    );
    for archive in [
        "tandem-$version-darwin-arm64.tar.gz",
        "tandem-$version-linux-x86_64.tar.gz",
        "tandem-$version-windows-x86_64.zip",
    ] {
        assert!(text.contains(archive), "release.yml packages `{archive}`");
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
