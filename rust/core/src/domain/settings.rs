//! The store's settings, the few keys a face reads.
//!
//! A setting is a `settings` row (ADR 0031): adding one is one more
//! `key`/`value` pair, never a migration, and the row's value is the JSON text
//! the Swift store wrote. This module is the domain over those rows, the same
//! way [`Board`](super::Board) is the domain over the tickets: the parsing and
//! the defaults are here, so no face invents them.

use crate::store::Store;

use super::error::DomainResult;
use crate::store::Actor;

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

    /// Store the roster whole: the stored list replaces the default entirely,
    /// and the next write replaces this one the same way.
    ///
    /// Cleaning lives here, so no face parses a row: names are trimmed,
    /// empties are dropped, and repeats keep their first order. An explicitly
    /// empty list stores as `[]` and reads back empty; it is the human's
    /// removal, not a value this build cannot read. The cleaned list is the
    /// receipt, the same names a read answers back.
    pub fn set_roster(&self, actor: &Actor, roster: Vec<String>) -> DomainResult<Vec<String>> {
        let cleaned = clean_roster(&roster);
        let value = serde_json::to_string(&cleaned).expect("a cleaned roster is writable as JSON");
        self.store.write(actor, |write| {
            write.upsert_setting(crate::store::NewSetting {
                key: ROSTER_KEY,
                value: &value,
            })
        })?;
        Ok(cleaned)
    }
}

/// Trim names, drop empties, dedupe preserving order: the one cleaning both the
/// write and its receipt share, so the face redraws what the store holds.
fn clean_roster(roster: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut cleaned = Vec::new();
    for name in roster {
        let trimmed = name.trim().to_string();
        if trimmed.is_empty() || !seen.insert(trimmed.clone()) {
            continue;
        }
        cleaned.push(trimmed);
    }
    cleaned
}

fn default_roster() -> Vec<String> {
    DEFAULT_ROSTER.iter().map(|name| name.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::store::{Actor, Home};

    static NEXT: AtomicUsize = AtomicUsize::new(0);

    struct Sandbox {
        root: std::path::PathBuf,
        settings: Settings,
    }

    impl Sandbox {
        fn new(name: &str) -> Self {
            let serial = NEXT.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "tandem-settings-{name}-{}-{serial}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&root);
            let home = Home::resolve(Some(&root.join("home")), None, &root.join("user"));
            let settings = Settings::new(&Store::new(home));
            Self { root, settings }
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| name.to_string()).collect()
    }

    /// A write replaces the stored list whole: the second write's names are
    /// the roster, not a merge of both writes.
    #[test]
    fn a_roster_write_replaces_the_stored_list_whole() {
        let sandbox = Sandbox::new("replace");
        let actor = Actor::Human;

        let stored = sandbox
            .settings
            .set_roster(&actor, names(&["alpha", "beta"]))
            .expect("the write lands");
        assert_eq!(stored, names(&["alpha", "beta"]));
        assert_eq!(
            sandbox.settings.roster().expect("the read lands"),
            names(&["alpha", "beta"])
        );

        let stored = sandbox
            .settings
            .set_roster(&actor, names(&["gamma"]))
            .expect("the write lands");
        assert_eq!(stored, names(&["gamma"]));
        assert_eq!(
            sandbox.settings.roster().expect("the read lands"),
            names(&["gamma"]),
            "the second write replaced the first, it did not merge"
        );
    }

    /// Cleaning lives in the domain, so no face parses a row: padding is
    /// trimmed, empties are dropped, and repeats keep their first order.
    #[test]
    fn a_roster_write_cleans_names() {
        let sandbox = Sandbox::new("clean");
        let stored = sandbox
            .settings
            .set_roster(
                &Actor::Human,
                names(&["  alpha  ", "", "   ", "beta", "alpha", "beta "]),
            )
            .expect("the write lands");
        assert_eq!(stored, names(&["alpha", "beta"]));
        assert_eq!(
            sandbox.settings.roster().expect("the read lands"),
            names(&["alpha", "beta"]),
            "the read answers the cleaned list the write stored"
        );
    }

    /// An explicitly empty list stores as empty and reads back empty: absent
    /// means "never edited", and the default is not a floor the human is stuck
    /// above.
    #[test]
    fn an_empty_roster_stores_as_empty() {
        let sandbox = Sandbox::new("empty");
        let stored = sandbox
            .settings
            .set_roster(&Actor::Human, Vec::new())
            .expect("the write lands");
        assert_eq!(stored, Vec::<String>::new());
        assert_eq!(
            sandbox.settings.roster().expect("the read lands"),
            Vec::<String>::new(),
            "an emptied roster stays empty, it does not fall back to the default"
        );
    }

    /// A value this build cannot read is the default: a broken row must not
    /// shrink the picker, and a later write still replaces it whole.
    #[test]
    fn a_malformed_stored_value_still_reads_as_the_default() {
        use crate::store::NewSetting;

        let sandbox = Sandbox::new("malformed");
        sandbox
            .settings
            .store
            .write(&Actor::Human, |write| {
                write.insert_setting(NewSetting {
                    key: ROSTER_KEY,
                    value: "not json",
                })
            })
            .expect("the broken row is storable");
        assert_eq!(
            sandbox.settings.roster().expect("the read lands"),
            default_roster(),
            "a malformed roster reads as the default"
        );

        let stored = sandbox
            .settings
            .set_roster(&Actor::Human, names(&["alpha"]))
            .expect("the write lands over the broken row");
        assert_eq!(stored, names(&["alpha"]));
    }
}
