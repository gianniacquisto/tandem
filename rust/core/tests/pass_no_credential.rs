//! Sprint runner part 2 (TAN-281): the pass-execution credential never crosses
//! into the core.
//!
//! Main owns spawn, lifetime, url and password (ADR 0038); the core owns the
//! transitions (TAN-275) and stays the only decider. The executor lives in
//! `app/src/main/passExecutor.ts` and speaks HTTP with Basic auth, so a
//! captured rpc frame is safe to log: no frame and no wire type carries the
//! secret.
//!
//! Scope: the usage read (`domain/harness.rs::read_api`, TAN-276) takes a
//! password as a function argument on its own path; this test pins the wire,
//! the frames the app and the core exchange. The core's rpc layer never speaks
//! the pass endpoint: dispatch adds no method, so the parity walkers
//! (`every_wire_method_names_a_cli_verb`) keep their existing seams.

use serde_json::json;
use tandem_core::wire::rpc::{METHODS, Request};

fn root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("the repository root resolves")
}

fn file(path: &str) -> String {
    std::fs::read_to_string(root().join(path)).unwrap_or_else(|_| panic!("missing {path}"))
}

#[test]
fn no_credential_crosses_into_the_core() {
    // Captured frames carry no secret: every method's empty-params frame parses
    // on the existing seam and names no credential.
    for (tag, _) in METHODS {
        let frame = json!({"id": 1, "method": tag, "params": {}}).to_string();
        let _: Request = serde_json::from_str(&frame)
            .unwrap_or_else(|error| panic!("{tag} accepts an empty params object: {error}"));
        let lowered = frame.to_lowercase();
        for needle in ["password", "authorization", "basic ", "opencode:"] {
            assert!(
                !lowered.contains(needle),
                "frame for `{tag}` carries `{needle}`: {frame}"
            );
        }
    }

    // The wire types carry no secret: no frame could name one.
    let wire = file("rust/core/src/wire/rpc.rs");
    for needle in [
        "password",
        "Authorization",
        "Basic",
        "OPENCODE_SERVER_PASSWORD",
        "TANDEM_HARNESS_PASSWORD",
    ] {
        assert!(
            !wire.contains(needle),
            "the wire carries `{needle}`, so a frame could name the secret"
        );
    }

    // The core's rpc layer never speaks the pass endpoint: dispatch adds no
    // method and no pass path beside the wire.
    let dispatch = file("rust/core/src/rpc/mod.rs");
    for needle in ["api/sprint/pass", "passExecutor", "sprint/pass"] {
        assert!(
            !dispatch.contains(needle),
            "the core's rpc layer names `{needle}`: pass dispatch belongs to main"
        );
    }

    // The secret lives in main only: the executor sends it on every request.
    let executor = file("app/src/main/passExecutor.ts");
    assert!(
        executor.contains("Authorization"),
        "the executor sends the Authorization header main owns (ADR 0038)"
    );
    assert!(
        executor.to_lowercase().contains("password"),
        "the executor holds the password main owns (ADR 0038)"
    );
}
