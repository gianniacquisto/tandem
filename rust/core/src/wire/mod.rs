//! The wire: the JSON the agent face prints and the app reads.
//!
//! One owner. The CLI parses argv and prints these types; it shapes no message
//! of its own, so `--help` drift and wire drift can never be the same bug
//! twice. `ts-rs` derives the TypeScript declarations from the same structs,
//! so the app cannot ship a shape the core does not answer (PRT-12).

use serde::Serialize;
use ts_rs::TS;

/// What `doctor` answers: ADR 0014's shape, `{"ok": bool, "checks": [...]}`.
///
/// The eight checks land with PRT-10; this stub answers the one row it owns,
/// `home`, which is the half every verb already depends on. `home` is
/// informational in ADR 0014 and cannot fail, so `ok` is true here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct DoctorReport {
    /// The AND of the rows that can fail. This stub's one row cannot.
    pub ok: bool,
    /// The checks that ran, in the order ADR 0014 pins for them.
    pub checks: Vec<DoctorCheck>,
}

/// One row of the report: exactly `name`, `ok` and `detail`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
pub struct DoctorCheck {
    /// The check's stable name; renaming one is a breaking change to the wire.
    pub name: String,
    /// Whether this check passed.
    pub ok: bool,
    /// What the check saw, in the words its reader needs.
    pub detail: String,
}
