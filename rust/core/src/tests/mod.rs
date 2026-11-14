//! The records domain's ported suites.
//!
//! Ported, not rewritten (`docs/port.md`, Parity): the behaviours the Swift
//! suites encode are the behaviours these assert, in Rust, against the store the
//! port writes. The bikeshed's Swift suites, one Rust module each:
//!   `bikeshed_path`, `bikeshed_wikilink`, `bikeshed_backlink`, `bikeshed_health`,
//!   `bikeshed_stats`, `bikeshed_search`, `bikeshed_move`, `bikeshed_rename`,
//!   `bikeshed_note_ops`, `bikeshed_conflict`, `bikeshed_crash_recovery`,
//!   `bikeshed_locking`, `bikeshed_scale`; `fixture_bikeshed` reads the
//!   committed fixture vault. The memory workflow's suites (`bikeshed_capture`,
//!   `bikeshed_front`, `bikeshed_hidden_layer`, `bikeshed_prune`, `daily_note`)
//!   left with the layer they tested (CUT-7).
//!
//! Each module is one suite, so `cargo test <suite>` runs it alone.

mod bikeshed_backlink;
mod bikeshed_conflict;
mod bikeshed_crash_recovery;
mod bikeshed_health;
mod bikeshed_locked;
mod bikeshed_locking;
mod bikeshed_move;
mod bikeshed_note_ops;
mod bikeshed_path;
mod bikeshed_rename;
mod bikeshed_scale;
mod bikeshed_search;
mod bikeshed_stats;
mod bikeshed_support;
mod bikeshed_wikilink;
mod examples_tools;
mod fixture_bikeshed;

#[cfg(test)]
mod sha {
    //! The pinned set has to resolve on every runner, not only on the machine
    //! that first fetched it. The bikeshed hashes note bytes with `sha2`; this
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
