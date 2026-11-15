//! `tandem mcp` is retired (ADR 0047): the agents' door is `POST /mcp` on
//! `tandem serve`, and the verb reads as any other unknown one. The protocol
//! cases this file carried over stdio live in `serve.rs`, over a real socket.

use std::process::{Command, Stdio};

#[test]
fn tandem_mcp_is_an_unknown_verb() {
    let output = Command::new(env!("CARGO_BIN_EXE_tandem"))
        .arg("mcp")
        .stdin(Stdio::null())
        .output()
        .expect("the binary runs");
    assert_eq!(
        output.status.code(),
        Some(2),
        "an unknown verb is a usage error"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Usage:"),
        "the refusal prints the grammar: {stderr}"
    );
}
