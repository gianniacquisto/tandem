//! `--version` answers and exits 0, and it answers without resolving a home:
//! the flag is read by the parser, before any verb runs.

use std::process::Command;

#[test]
fn version_answers_and_exits_zero() {
    let output = Command::new(env!("CARGO_BIN_EXE_tandem"))
        .arg("--version")
        .output()
        .expect("the binary runs");

    assert!(output.status.success(), "--version exits 0");
    let stdout = String::from_utf8(output.stdout).expect("--version prints UTF-8");
    assert!(
        stdout.starts_with("tandem "),
        "--version names the binary: {stdout:?}"
    );
    assert!(
        stdout
            .trim()
            .split(' ')
            .nth(1)
            .is_some_and(|version| !version.is_empty()),
        "--version carries a version: {stdout:?}"
    );
}
