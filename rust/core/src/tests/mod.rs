//! The records domain's ported suites.
//!
//! Ported, not rewritten (`docs/port.md`, Parity): the behaviours the Swift
//! suites encode are the behaviours these assert, in Rust, against the store the
//! port writes. The Swift names and the suite each behaviour came from:
//!
//! - `SessionsTests`, `SessionVerbTests`, `UsageWrapperTests`: `sessions`
//! - `SprintsTests`, `SprintGroupingTests`, `SprintDocumentsTests`,
//!   `SprintMigrationTests`, `SprintVerbTests`: `sprints`
//! - `UsageReportTests`: `usage_report`
//! - the harness read (`Scripts/usage-report.sh` today): `harness`
//!
//! Each module is one suite, so `cargo test <suite>` runs it alone.

mod harness;
mod sessions;
mod sprints;
mod support;
mod usage_report;

#[cfg(test)]
mod sha {
    //! The pinned set has to resolve on every runner, not only on the machine
    //! that first fetched it. `sha2` has no consumer yet (note content hashes
    //! are PRT-7's), so this test is what keeps it in `Cargo.lock` and proves
    //! the crate answers.

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
