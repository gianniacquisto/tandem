//! The records domain's ported suites.
//!
//! The behaviours these assert, against the store the adapter writes. One Rust
//! module per notes suite:
//!   `notes_path`, `notes_wikilink`, `notes_backlink`, `notes_health`,
//!   `notes_stats`, `notes_search`, `notes_move`, `notes_rename`,
//!   `notes_note_ops`, `notes_conflict`, `notes_crash_recovery`,
//!   `notes_locking`, `notes_scale`; `fixture_notes` reads the
//!   committed fixture vault. The memory workflow's suites (`notes_capture`,
//!   `notes_front`, `notes_hidden_layer`, `notes_prune`, `daily_note`)
//!   left with the layer they tested (CUT-7).
//!
//! Each module is one suite, so `cargo test <suite>` runs it alone.

mod examples_tools;
mod fixture_notes;
mod notes_backlink;
mod notes_conflict;
mod notes_crash_recovery;
mod notes_edit;
mod notes_graph;
mod notes_health;
mod notes_locked;
mod notes_locking;
mod notes_move;
mod notes_note_ops;
mod notes_path;
mod notes_rename;
mod notes_scale;
mod notes_search;
mod notes_stats;
mod notes_support;
mod notes_trash;
mod notes_wikilink;

#[cfg(test)]
mod sha {
    //! The pinned set has to resolve on every runner, not only on the machine
    //! that first fetched it. The notes hashes note bytes with `sha2`; this
    //! test pins the crate's own answer beside them.

    use sha2::{Digest, Sha256};

    #[test]
    fn the_pinned_sha2_answers() {
        let digest = Sha256::digest(b"tandem");
        let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
        assert_eq!(
            hex, "7186b6e1e13844499594bc935ee9f1091d59093bcb00eca31c3d563c5bb86f7d",
            "SHA-256 of the binary's name"
        );
    }
}
