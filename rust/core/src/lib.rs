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
//! - [`domain`]: the aggregates, pure over the adapter: the board's tickets,
//!   lanes, comments, order and the dependency edge (PRT-5), and the records'
//!   sessions, sprints and usage derivation (PRT-6). Every failure the domain
//!   raises is [`domain::error::DomainError`], one shape for the CLI to print.
//!
//! - [`bikeshed`]: the notes vault (PRT-7), a plain folder of markdown files
//!   rather than rows: the tree, the note operations, the wikilink scanner, the
//!   derived readers (search, backlinks, health, front page, stats) and the
//!   capture layer. It shares the store's locked-write mechanics and no SQL.
//!
//! - [`setup`]: the agent face's one writer (PRT-10, ADR 0034): the installed
//!   copy, the host links, the manifest and the warning model `setup status` and
//!   `doctor` both read.
//!
//! - [`diagnostics`]: ADR 0014's eleven-row setup report, a pure assembly over
//!   injected facts with one impure gatherer.

pub mod bikeshed;
pub mod diagnostics;
pub mod domain;
pub mod rpc;
pub mod setup;
pub mod store;
pub mod wire;

#[cfg(test)]
mod tests;
