//! The TypeScript declarations are generated from the Rust structs, and this
//! test is what makes a stale one fail the build (`docs/port.md`, Stack).
//!
//! `tandem_core::wire::typescript::files()` renders every exported type the
//! same way the exporter does; the committed files under
//! `app/renderer/src/wire/` must be byte-identical. A field added to a Rust
//! struct without regenerating is a red test here, not a shape the app ships.
//!
//! Regenerate with:
//!
//! ```text
//! cargo run -p tandem-core --example export-wire
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn wire_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("app")
        .join("renderer")
        .join("src")
        .join("wire")
}

#[test]
fn the_typescript_types_are_current() {
    let directory = wire_directory();
    let expected: BTreeMap<String, String> = tandem_core::wire::typescript::files()
        .into_iter()
        .map(|(name, body)| (name.to_string(), body))
        .collect();
    assert!(!expected.is_empty(), "the wire exports at least one type");

    for (name, body) in &expected {
        let path = directory.join(name);
        let actual = fs::read_to_string(&path).unwrap_or_else(|error| {
            panic!(
                "the generated type {} is missing ({error}); run \
                 `cargo run -p tandem-core --example export-wire`",
                path.display()
            )
        });
        assert_eq!(
            actual,
            *body,
            "{} is stale; run `cargo run -p tandem-core --example export-wire`",
            path.display()
        );
    }

    let committed: Vec<String> = fs::read_dir(&directory)
        .unwrap_or_else(|error| {
            panic!(
                "the generated types live at {} ({error}); run \
                 `cargo run -p tandem-core --example export-wire`",
                directory.display()
            )
        })
        .flatten()
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .filter(|name| name.ends_with(".ts"))
        .collect();
    for name in committed {
        assert!(
            expected.contains_key(&name),
            "{} is generated but no Rust type exports it any more; run \
             `cargo run -p tandem-core --example export-wire`",
            directory.join(&name).display()
        );
    }
}
