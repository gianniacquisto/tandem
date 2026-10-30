//! A throwaway home for the record suites: a directory per sandbox, a store
//! over it, and the board rows the sprints' batch check asks for.
//!
//! Nothing here touches `~/.tandem`: the home is a temp directory, and the
//! sandbox removes it on drop. The suites seed the board through the store's own
//! write, so a ticket the batch takes is a ticket the store really carries.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::store::{Actor, Home, NewTicket, Store};

static NEXT_SANDBOX: AtomicUsize = AtomicUsize::new(0);

pub struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    pub fn new(name: &str) -> Self {
        let serial = NEXT_SANDBOX.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "tandem-records-{name}-{}-{serial}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("the sandbox is creatable");
        Self { root }
    }

    pub fn path(&self) -> &Path {
        &self.root
    }

    pub fn home(&self) -> Home {
        Home::resolve(Some(&self.root), None, &self.root)
    }

    pub fn store(&self) -> Store {
        Store::new(self.home())
    }

    /// The pass an agent-run write records.
    pub fn actor(&self) -> Actor {
        Actor::Agent("builder".to_string())
    }

    /// Seed one board ticket, so `sprint ticket` has an id the store holds.
    pub fn ticket(&self, id: &str) {
        let store = self.store();
        store
            .write(&Actor::Human, |write| {
                write.insert_ticket(NewTicket {
                    id,
                    title: "a fixture ticket",
                    description: None,
                    status: "ready",
                    archived: false,
                    lane: None,
                    number: None,
                    origin: "human",
                    assignee: None,
                    link: None,
                    position: 0,
                    created_at: "2026-10-30T09:00:00Z",
                    completed_at: None,
                })
            })
            .expect("the fixture ticket is written");
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// A whole-second stamp the suites share, so two records can be compared without
/// the clock moving under them.
pub fn at(seconds: i64) -> String {
    crate::store::timestamp::encode(1_793_000_000 + seconds)
}

pub const TICKET_ONE: &str = "00000000-0000-0000-0000-000000000001";
pub const TICKET_TWO: &str = "00000000-0000-0000-0000-000000000002";
pub const TICKET_THREE: &str = "00000000-0000-0000-0000-000000000003";
