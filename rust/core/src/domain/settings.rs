//! The store's settings, the few keys a face reads.
//!
//! A setting is a `settings` row (ADR 0031): adding one is one more
//! `key`/`value` pair, never a migration, and the row's value is the JSON text
//! the Swift store wrote. This module is the domain over those rows, the same
//! way [`Board`](super::Board) is the domain over the tickets: the parsing and
//! the defaults are here, so no face invents them.

use crate::store::Store;
use crate::store::{Actor, NewSetting};

use super::error::DomainResult;
use super::error::DomainError;

/// The roster's key: the names the assignee picker offers (`roster` in the
/// Swift store, `TandemSettings.swift`).
pub const ROSTER_KEY: &str = "roster";

/// The active project's key: the lane the faces draw when the human works on
/// one project at a time. The row holds the lane name as plain text (not
/// JSON: it is one name, not a list), or is absent for no focus.
pub const ACTIVE_PROJECT_KEY: &str = "active_project";

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

    /// The active project: the lane the faces draw when the human works on one
    /// project at a time.
    ///
    /// The row holds a lane name or is absent (ADR 0031: a new key is a new
    /// pair, never a migration). Unset, cleared, blank, or naming a lane the
    /// board no longer declares all read as no focus, never as an empty board:
    /// a stale focus must not hide tickets.
    pub fn active_project(&self, declared: &[String]) -> DomainResult<Option<String>> {
        let Some(stored) = self.store.setting(ACTIVE_PROJECT_KEY)? else {
            return Ok(None);
        };
        let name = stored.trim();
        if name.is_empty() || !declared.iter().any(|lane| lane == name) {
            return Ok(None);
        }
        Ok(Some(name.to_string()))
    }

    /// Focus the faces on one declared lane, or clear the focus back to every
    /// lane. An absent or blank project clears; a name no declared lane
    /// carries is refused with the lanes the human may pick, and nothing is
    /// written. The answer is the resolved focus, the same value a read gives.
    pub fn set_active_project(
        &self,
        actor: &Actor,
        project: Option<&str>,
        declared: &[String],
    ) -> DomainResult<Option<String>> {
        let name = project.map(str::trim).filter(|name| !name.is_empty());
        // A write that would store what the read already answers is no write:
        // the revision stays put, so the faces re-read nothing (`ticket.set`'s
        // same-value rule).
        let current = self.active_project(declared)?;
        if current.as_deref() == name {
            return Ok(current);
        }
        let Some(name) = name else {
            self.store
                .write(actor, |write| write.delete_setting(ACTIVE_PROJECT_KEY))?;
            return Ok(None);
        };
        if !declared.iter().any(|lane| lane == name) {
            return Err(DomainError::UnknownProject {
                name: name.to_string(),
                lanes: declared.to_vec(),
            });
        }
        let focused = name.to_string();
        self.store.write(actor, |write| {
            write.upsert_setting(NewSetting {
                key: ACTIVE_PROJECT_KEY,
                value: &focused,
            })
        })?;
        Ok(Some(focused))
    }
}

fn default_roster() -> Vec<String> {
    DEFAULT_ROSTER.iter().map(|name| name.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::store::{Actor, Home, NewSetting};

    static NEXT_SANDBOX: AtomicUsize = AtomicUsize::new(0);

    struct Sandbox {
        root: PathBuf,
    }

    impl Sandbox {
        fn new(name: &str) -> Self {
            let serial = NEXT_SANDBOX.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "tandem-settings-{name}-{}-{serial}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).expect("the sandbox is creatable");
            Self { root }
        }

        fn store(&self) -> Store {
            Store::new(Home::resolve(
                Some(&self.root.join("home")),
                None,
                &self.root.join("user"),
            ))
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn lanes(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    fn stored_focus(store: &Store, value: &str) {
        store
            .write(&Actor::Human, |write| {
                write.insert_setting(NewSetting {
                    key: ACTIVE_PROJECT_KEY,
                    value,
                })
            })
            .expect("the row is stored");
    }

    #[test]
    fn an_unset_focus_reads_as_no_focus() {
        let sandbox = Sandbox::new("unset");
        let store = sandbox.store();

        assert_eq!(
            Settings::new(&store).active_project(&lanes(&["Tandem"])),
            Ok(None),
            "a home that never stored a focus has none"
        );
    }

    #[test]
    fn a_cleared_focus_reads_as_no_focus() {
        let sandbox = Sandbox::new("cleared");
        let store = sandbox.store();
        stored_focus(&store, "");
        assert_eq!(
            Settings::new(&store).active_project(&lanes(&["Tandem"])),
            Ok(None),
            "an explicitly cleared focus is no focus"
        );

        let sandbox = Sandbox::new("blank");
        let store = sandbox.store();
        stored_focus(&store, "   ");
        assert_eq!(
            Settings::new(&store).active_project(&lanes(&["Tandem"])),
            Ok(None),
            "a whitespace-only row is no focus"
        );
    }

    #[test]
    fn a_stale_focus_reads_as_no_focus() {
        let sandbox = Sandbox::new("stale");
        let store = sandbox.store();
        stored_focus(&store, "Retro");

        assert_eq!(
            Settings::new(&store).active_project(&lanes(&["Tandem", "Port"])),
            Ok(None),
            "a focus naming a lane that no longer exists hides nothing"
        );
    }

    #[test]
    fn a_stored_declared_lane_round_trips() {
        let sandbox = Sandbox::new("round-trip");
        let store = sandbox.store();
        stored_focus(&store, "Port");

        assert_eq!(
            Settings::new(&store).active_project(&lanes(&["Tandem", "Port"])),
            Ok(Some("Port".to_string()))
        );
    }

    #[test]
    fn setting_a_declared_lane_persists_and_clearing_removes_it() {
        let sandbox = Sandbox::new("set-clear");
        let store = sandbox.store();
        let settings = Settings::new(&store);
        let declared = lanes(&["Tandem", "Port"]);

        assert_eq!(
            settings.set_active_project(&Actor::Human, Some("Port"), &declared),
            Ok(Some("Port".to_string()))
        );
        assert_eq!(settings.active_project(&declared), Ok(Some("Port".to_string())));

        assert_eq!(
            settings.set_active_project(&Actor::Human, None, &declared),
            Ok(None)
        );
        assert_eq!(settings.active_project(&declared), Ok(None));
        assert_eq!(
            store.setting(ACTIVE_PROJECT_KEY).expect("the row reads"),
            None,
            "clearing removes the row, it does not store an empty one"
        );
    }

    #[test]
    fn resetting_the_same_focus_moves_no_revision() {
        let sandbox = Sandbox::new("no-op");
        let store = sandbox.store();
        let settings = Settings::new(&store);
        let declared = lanes(&["Tandem", "Port"]);

        settings
            .set_active_project(&Actor::Human, Some("Port"), &declared)
            .expect("the focus is set");
        let written = store.revision().expect("the revision reads");
        assert!(written > 0, "the focus write moved the revision");

        assert_eq!(
            settings.set_active_project(&Actor::Human, Some("Port"), &declared),
            Ok(Some("Port".to_string()))
        );
        assert_eq!(
            store.revision().expect("the revision reads"),
            written,
            "resetting the same focus writes nothing and moves no revision"
        );

        settings
            .set_active_project(&Actor::Human, None, &declared)
            .expect("the focus is cleared");
        let cleared = store.revision().expect("the revision reads");
        assert_eq!(
            settings.set_active_project(&Actor::Human, None, &declared),
            Ok(None)
        );
        assert_eq!(
            store.revision().expect("the revision reads"),
            cleared,
            "clearing an absent focus writes nothing either"
        );
    }

    #[test]
    fn setting_an_undeclared_lane_is_refused_naming_the_declared_lanes() {
        let sandbox = Sandbox::new("unknown");
        let store = sandbox.store();
        let settings = Settings::new(&store);
        let declared = lanes(&["Tandem", "Port"]);

        let refused = settings
            .set_active_project(&Actor::Human, Some("Nope"), &declared)
            .expect_err("an undeclared lane is refused");
        let message = refused.message();
        assert!(
            message.contains("\"Nope\""),
            "the refusal names the lane: {message}"
        );
        assert!(
            message.contains("Tandem") && message.contains("Port"),
            "the refusal names the declared lanes: {message}"
        );
        assert_eq!(
            store.setting(ACTIVE_PROJECT_KEY).expect("the row reads"),
            None,
            "the refused write left the store alone"
        );
    }
}
