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
//! The domain itself lands at this level, one module per aggregate: the records
//! ([`session`], [`sprint`], [`usage`]) are this ticket's, and the board and the
//! bikeshed are PRT-5's and PRT-7's. Every failure the domain raises is
//! [`error::DomainError`], one shape for the CLI to print.

pub mod error;
pub mod operation;
pub mod session;
pub mod sprint;
pub mod store;
pub mod usage;
pub mod wire;

#[cfg(test)]
mod tests;
