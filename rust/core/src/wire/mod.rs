//! The wire: the JSON the agent face prints and the app reads.
//!
//! One owner. The CLI parses argv and prints these types; it shapes no message
//! of its own, so `--help` drift and wire drift can never be the same bug
//! twice. `ts-rs` derives the TypeScript declarations from the same structs,
//! so the app cannot ship a shape the core does not answer (PRT-12).

use serde::Serialize;
use ts_rs::TS;

use crate::store::HomeSource;

/// What `doctor` reports about the run's home.
///
/// The eight diagnostics land with PRT-10; this is the home half, which is the
/// half every verb already depends on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct HomeReport {
    /// The absolute path this run resolved.
    pub home: String,
    /// Which branch of the precedence rule resolved it.
    pub source: HomeSource,
}
