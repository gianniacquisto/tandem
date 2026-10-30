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
//!   sessions, sprints and usage derivation (PRT-6). The bikeshed follows at this
//!   level with PRT-7. Every failure the domain raises is
//!   [`domain::error::DomainError`], one shape for the CLI to print.

pub mod domain;
pub mod store;
pub mod wire;

#[cfg(test)]
mod tests;
