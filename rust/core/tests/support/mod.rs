//! The suite's one fixture: a throwaway home no other test can reach.
//!
//! Every case is a `Sandbox` with its own directory under the system temp dir,
//! removed when it drops, so no run touches the human's `~/.tandem` and two tests
//! cannot share a store by accident.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use tandem_core::domain::{Board, Filter, Ticket};
use tandem_core::store::{Actor, Home, Store};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// A throwaway root: `root/home` is the Tandem home, `root/user` a sandbox user
/// home, so "the user's `~/.tandem`" is a path no run resolves unless it was told
/// to.
pub struct Sandbox {
    root: PathBuf,
}

impl Sandbox {
    pub fn new(name: &str) -> Self {
        let serial = NEXT.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "tandem-domain-{name}-{}-{serial}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("the sandbox is creatable");
        Self { root }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn home(&self) -> Home {
        Home::resolve(Some(&self.root.join("home")), None, &self.root.join("user"))
    }

    pub fn store(&self) -> Store {
        Store::new(self.home())
    }

    /// A board writing as the human, the actor most cases need.
    pub fn board(&self) -> Board {
        Board::with_actor(self.store(), Actor::Human)
    }

    /// A board writing as one named pass, for the cases that pin authorship.
    pub fn board_as(&self, actor: &str) -> Board {
        Board::with_actor(self.store(), Actor::Agent(actor.to_string()))
    }

    pub fn database(&self) -> PathBuf {
        self.root.join("home").join("tandem.db")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// The tickets in stored order under `filter`.
pub fn titles(board: &Board, filter: Filter) -> Vec<String> {
    board
        .list_filter(filter)
        .expect("the read answers")
        .into_iter()
        .map(|ticket| ticket.title)
        .collect()
}

/// Every ticket in stored order, the archive included.
pub fn all(board: &Board) -> Vec<Ticket> {
    board.list_filter(Filter::All).expect("the read answers")
}
