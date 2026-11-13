//! The Settings face's contract with the wire (ADR 0014, `docs/port.md`):
//! every method the face calls is a method the wire speaks, so a renderer
//! cannot invent a UI-only writer.
//!
//! The face's setup half retired with ADR 0045; what stays is the store's
//! roster, read and written through `settings.get` and `settings.set`.

use std::fs;
use std::path::Path;

use tandem_core::wire::rpc::METHODS;

#[test]
fn every_method_the_settings_face_calls_is_the_wires() {
    let app = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../app/renderer/src");
    let name = "faces/SettingsFace.tsx";
    let source = fs::read_to_string(app.join(name))
        .unwrap_or_else(|error| panic!("the renderer source {name} is readable: {error}"));

    let mut called: Vec<String> = Vec::new();
    for capture in source.split("method: '").skip(1) {
        let tag = capture
            .split('\'')
            .next()
            .unwrap_or_else(|| panic!("a method literal in {name} is one tag"));
        called.push(tag.to_string());
    }

    assert!(!called.is_empty(), "the Settings face calls the wire");
    for tag in &called {
        assert!(
            METHODS.iter().any(|(known, _)| known == tag),
            "the Settings face calls `{tag}`, which the wire does not speak"
        );
    }
}
