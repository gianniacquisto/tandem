//! Tandem's core: the domain, the store adapter and the wire.
//!
//! The port is laid out at its seam, not at the Swift module's folder shape
//! (`docs/port.md`, Where the old code goes):
//!
//! - [`store`]: the home, and from PRT-4 the SQLite adapter behind it. All SQL
//!   lives there; nothing outside it writes a row.
//! - [`wire`]: the JSON the agent face prints and the app reads.
//! - [`domain`]: the aggregates, pure over the adapter: the board's tickets,
//!   lanes, comments, order and the dependency edge (PRT-5). Every failure the
//!   domain raises is [`domain::error::DomainError`],
//!   one shape for the CLI to print.
//!
//! - [`bikeshed`]: the notes vault (PRT-7), a plain folder of markdown files
//!   rather than rows: the tree, the note operations, the wikilink scanner, the
//!   derived readers (search, backlinks, health, front page, stats) and the
//!   capture layer. It carries its own locked-write mechanics and no SQL.
//!
//! - [`env`]: the run's environment as one injectable value, so a test names a
//!   throwaway home and a fixture instead of the process's own.
//!
//! - [`diagnostics`]: ADR 0014's report, cut to `home`, `store` and `bikeshed`
//!   (ADR 0045), a pure assembly over injected facts with one impure gatherer.

pub mod bikeshed;
pub mod diagnostics;
pub mod domain;
pub mod env;
pub mod mcp;
pub mod rpc;
pub mod serve;
pub mod store;
pub mod wire;

/// The release version the build carries.
///
/// The release workflow sets `TANDEM_VERSION` before `cargo build --release`,
/// so the tag run's binary answers its tag and the archive's smoke test reads it
/// back from `tandem --version`. A dev build carries no stamp and falls back to
/// the crate version (the workspace sits at 0.1.0 and never moves with a
/// release). One reader for both the CLI's version flag and the MCP handshake.
pub const fn release_version() -> &'static str {
    match option_env!("TANDEM_VERSION") {
        Some(version) => version,
        None => env!("CARGO_PKG_VERSION"),
    }
}

#[cfg(test)]
mod tests;
