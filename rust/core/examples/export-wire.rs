//! Regenerate the TypeScript declarations from the Rust wire types.
//!
//! `docs/port.md` (Stack): the message types are generated, so the app cannot
//! ship a shape the core does not answer. This example is the only writer of
//! `app/renderer/src/wire/`; `tests/wire_types.rs` fails when the committed
//! files and this output disagree.
//!
//! ```text
//! cargo run -p tandem-core --example export-wire
//! ```

use std::fs;
use std::path::PathBuf;

fn main() {
    let directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("app")
        .join("renderer")
        .join("src")
        .join("wire");
    fs::create_dir_all(&directory).unwrap_or_else(|error| {
        panic!(
            "the wire directory {} is creatable: {error}",
            directory.display()
        )
    });
    for (name, body) in tandem_core::wire::typescript::files() {
        let path = directory.join(name);
        // LF on every platform: the generator's copy is the one the staleness
        // test compares, and a Windows checkout may hand the same file back
        // with CRLF. Written as bytes, never through a text translation.
        let body = body.replace("\r\n", "\n");
        fs::write(&path, body.as_bytes())
            .unwrap_or_else(|error| panic!("{} is writable: {error}", path.display()));
        println!("{}", path.display());
    }
}
