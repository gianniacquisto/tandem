//! A stdin body is text. The verbs' multi-line arguments arrive on a pipe, and
//! a pipe that is not UTF-8 is its own failure, not the bikeshed's I/O error:
//! the body never named a vault path (PRT-24).

use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};

const BINARY: &str = env!("CARGO_BIN_EXE_tandem");

#[test]
fn a_non_utf8_stdin_body_answers_its_own_error_and_exit_one() {
    let root = std::env::temp_dir().join(format!("tandem-stdin-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("the sandbox is creatable");

    let mut child = Command::new(BINARY)
        .arg("--home")
        .arg(root.join("Home"))
        .args([
            "session",
            "start",
            "--session",
            "ses_stdin",
            "--agent",
            "builder",
            "--brief",
            "-",
        ])
        .env("HOME", &root)
        .env("USERPROFILE", &root)
        .env_remove("TANDEM_HOME")
        .env_remove("OPENCODE_SESSION_ID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary runs");
    child
        .stdin
        .as_mut()
        .expect("a piped stdin")
        .write_all(&[0xff, 0xfe, 0xfd])
        .expect("the bytes are writable");
    let output = child.wait_with_output().expect("the run finishes");

    assert_eq!(output.status.code(), Some(1), "a domain failure is exit 1");
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        "stdin is not valid UTF-8. Try again.\n",
        "the body's own failure, not a bikeshed error"
    );
    assert!(
        output.stdout.is_empty(),
        "a failure prints nothing on stdout"
    );

    let _ = fs::remove_dir_all(&root);
}
