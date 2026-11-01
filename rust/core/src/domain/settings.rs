//! The store's settings, the few keys a face reads.
//!
//! A setting is a `settings` row (ADR 0031): adding one is one more
//! `key`/`value` pair, never a migration, and the row's value is the JSON text
//! the Swift store wrote. This module is the domain over those rows, the same
//! way [`Board`](super::Board) is the domain over the tickets: the parsing and
//! the defaults are here, so no face invents them.

use crate::store::Store;

use super::error::DomainResult;

/// The roster's key: the names the assignee picker offers (`roster` in the
/// Swift store, `TandemSettings.swift`).
pub const ROSTER_KEY: &str = "roster";

/// The roster a home offers when the human never edited one: the roles of
/// `skills/tandem-team/SKILL.md` (ADR 0029) at the time the setting shipped,
/// the same list the Swift app defaulted to. A stored roster replaces it whole;
/// it is the human's list, not a set to merge into.
pub const DEFAULT_ROSTER: &[&str] = &[
    "human",
    "builder",
    "refiner",
    "reviewer",
    "consolidator",
    "sprint",
];

/// The settings the faces read, over the home's store.
#[derive(Debug, Clone)]
pub struct Settings {
    store: Store,
}

impl Settings {
    pub fn new(store: &Store) -> Self {
        Self {
            store: store.clone(),
        }
    }

    /// The roster: the stored names, or the default when none was ever stored.
    ///
    /// A value this build cannot read is the default too, because a broken row
    /// must not shrink the picker; an **explicitly empty** list is empty, because
    /// the human removed every name and the picker's own constants (`human`,
    /// Unassigned) keep it useful anyway.
    pub fn roster(&self) -> DomainResult<Vec<String>> {
        let Some(stored) = self.store.setting(ROSTER_KEY)? else {
            return Ok(default_roster());
        };
        match serde_json::from_str::<Vec<String>>(&stored) {
            Ok(names) => Ok(names),
            Err(_) => Ok(default_roster()),
        }
    }
}

fn default_roster() -> Vec<String> {
    DEFAULT_ROSTER.iter().map(|name| name.to_string()).collect()
}
