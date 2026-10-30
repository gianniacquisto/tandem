//! Ported from `BikeshedPathTests`: the path-hygiene rule (ADR 0008), the one
//! place that decides whether a bikeshed-root-relative path is legal or an
//! escape.

use std::path::{Path, PathBuf};

use crate::bikeshed::BikeshedPath;

fn root() -> PathBuf {
    PathBuf::from("/tmp/tandem-bikeshed")
}

#[test]
fn legal_paths_resolve_under_the_root() {
    assert_eq!(
        BikeshedPath::url(&root(), "Note.md"),
        Some(PathBuf::from("/tmp/tandem-bikeshed/Note.md"))
    );
    assert_eq!(
        BikeshedPath::url(&root(), "a/b/C.md"),
        Some(PathBuf::from("/tmp/tandem-bikeshed/a/b/C.md"))
    );
    // Spaces are legal: the create/rename rule allows them.
    assert_eq!(
        BikeshedPath::url(&root(), "My Note.md"),
        Some(PathBuf::from("/tmp/tandem-bikeshed/My Note.md"))
    );
    // `#` and `|` are tolerated here (pre-existing on-disk violators).
    assert!(BikeshedPath::url(&root(), "a#b.md").is_some());
}

#[test]
fn an_empty_path_empty_components_and_dot_escapes_are_refused() {
    for bad in [
        "",
        "/Note.md",
        "a//b.md",
        "a/",
        ".",
        "..",
        "../outside.md",
        "a/../../outside.md",
        "a/./b.md",
    ] {
        assert_eq!(BikeshedPath::url(&root(), bad), None, "refused: {bad}");
    }
}

#[test]
fn the_resolved_path_is_standardized_regardless_of_the_roots_shape() {
    let messy = Path::new("/tmp/tandem-bikeshed/./");
    assert_eq!(
        BikeshedPath::url(messy, "a/b.md"),
        Some(PathBuf::from("/tmp/tandem-bikeshed/a/b.md"))
    );
}

#[test]
fn a_component_containing_a_slash_cannot_smuggle_a_path() {
    assert_eq!(BikeshedPath::url(&root(), "a/../b.md"), None);
}
