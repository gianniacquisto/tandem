//! Tandem's core: the domain, the store adapter and the wire.
//!
//! The port is laid out at its seam, not at the Swift module's folder shape
//! (`docs/port.md`, Where the old code goes):
//!
//! - [`store`]: the home, and from PRT-4 the SQLite adapter behind it. All SQL
//!   lives there; nothing outside it writes a row.
//! - [`wire`]: the JSON the agent face prints and the app reads. The
//!   TypeScript declarations are generated from the same types (`ts-rs`).
//!
//! - [`domain`]: the board, pure over the adapter: tickets, lanes, comments,
//!   order and the dependency edge (PRT-5). The bikeshed and the records follow at
//!   this level with PRT-6 and PRT-7.

pub mod domain;
pub mod store;
pub mod wire;

#[cfg(test)]
mod tests {
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
