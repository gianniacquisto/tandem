//! The browser sessions a token-mode `tandem serve` keeps (ADR 0049).
//!
//! A session is one row of `browser_sessions`: the keyed digest of the id the
//! browser holds, when it was minted and when it was last used. The id itself
//! is never stored, and the digest is keyed with the token's in-memory digest
//! by the caller, so a changed token matches no row.
//!
//! **A session row is not board data.** Its writes go through
//! [`Store::session_write`], not [`Store::write`]: they never move the store's
//! revision (so signing in wakes no `/events` stream) and never arm the rolling
//! backup. They still migrate the store lazily, as every write does.
//!
//! All SQL lives under the store module (ADR 0009); serve holds none.

use rusqlite::{Connection, OptionalExtension, params};

use super::schema::{self, has_table};
use super::{
    Actor, Result, Store, backup_database, begin_immediate, commit, enter_gate, open,
    sweep_orphans, switch_to_wal, translate, write_gate,
};

/// A session unused this long is gone: 30 days since last use.
pub const SESSION_IDLE_SECONDS: i64 = 30 * 24 * 60 * 60;

/// Last use is rewritten at most this often per session.
pub const SESSION_TOUCH_SECONDS: i64 = 24 * 60 * 60;

/// What a lookup found.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionCheck {
    /// No such row, an expired one, or a store with no table yet.
    Missing,
    /// A live session; its last use was not rewritten.
    Valid,
    /// A live session whose last use was rewritten now, so the cookie's
    /// lifetime is refreshed with it.
    Refreshed,
}

/// Version 13: the browser sessions. Only a keyed digest, no member or owner
/// column (a later migration adds one).
pub(crate) const VERSION_THIRTEEN: &str = "
CREATE TABLE IF NOT EXISTS browser_sessions (
    digest TEXT PRIMARY KEY,
    created_at INTEGER NOT NULL,
    last_used_at INTEGER NOT NULL
);
";

impl Store {
    /// A write that is not board data: the lazy migration and the one
    /// transaction of [`Store::write`], and neither the revision bump nor the
    /// rolling backup.
    fn session_write<T>(&self, body: impl FnOnce(&Connection) -> Result<T>) -> Result<T> {
        self.home().ensure().map_err(|error| {
            super::StoreError::Io(format!(
                "could not create the home {}: {error}",
                self.home().directory().display()
            ))
        })?;
        let gate = write_gate(self.home().directory());
        let _gate = enter_gate(&gate, self.home().directory(), self.gate_timeout)?;
        sweep_orphans(self.home().directory());
        let database = self.database();
        let existed = database.exists();
        let connection = open(&database)?;
        schema::refuse_legacy(&connection)?;
        switch_to_wal(&connection)?;
        if existed && schema::behind(&connection)? {
            backup_database(&database, &self.backup())?;
        }
        schema::migrate(&connection, &Actor::Human)?;
        begin_immediate(&connection)?;
        match body(&connection) {
            Ok(value) => {
                commit(&connection)?;
                Ok(value)
            }
            Err(error) => {
                let _ = connection.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    /// Mint a session: insert its digest, drop the one the browser presented
    /// (a login replaces it) and sweep every expired row.
    pub fn browser_session_open(
        &self,
        digest: &str,
        replacing: Option<&str>,
        now: i64,
    ) -> Result<()> {
        self.session_write(|connection| {
            sweep(connection, now)?;
            if let Some(old) = replacing {
                connection
                    .execute("DELETE FROM browser_sessions WHERE digest = ?1", [old])
                    .map_err(|error| translate(error, connection))?;
            }
            connection
                .execute(
                    "INSERT OR REPLACE INTO browser_sessions (digest, created_at, last_used_at)
                     VALUES (?1, ?2, ?2)",
                    params![digest, now],
                )
                .map_err(|error| translate(error, connection))?;
            Ok(())
        })
    }

    /// Look a session up. A read never migrates, so a store with no table, or
    /// no database, answers [`SessionCheck::Missing`], never an error. With
    /// `touch`, a live session last used a day or more ago is rewritten.
    pub fn browser_session_check(
        &self,
        digest: &str,
        now: i64,
        touch: bool,
    ) -> Result<SessionCheck> {
        let last_used = self
            .read(|connection| {
                if !has_table(connection, "browser_sessions")? {
                    return Ok(None);
                }
                connection
                    .query_row(
                        "SELECT last_used_at FROM browser_sessions WHERE digest = ?1",
                        [digest],
                        |row| row.get::<_, i64>(0),
                    )
                    .optional()
                    .map_err(|error| translate(error, connection))
            })?
            .flatten();
        let Some(last_used) = last_used else {
            return Ok(SessionCheck::Missing);
        };
        if now - last_used > SESSION_IDLE_SECONDS {
            return Ok(SessionCheck::Missing);
        }
        if !touch || now - last_used < SESSION_TOUCH_SECONDS {
            return Ok(SessionCheck::Valid);
        }
        let touched = self.session_write(|connection| {
            connection
                .execute(
                    "UPDATE browser_sessions SET last_used_at = ?2
                     WHERE digest = ?1 AND last_used_at < ?2",
                    params![digest, now],
                )
                .map_err(|error| translate(error, connection))
        });
        // A touch that loses the lock is not a sign-out: the session is live.
        Ok(match touched {
            Ok(_) => SessionCheck::Refreshed,
            Err(_) => SessionCheck::Valid,
        })
    }

    /// End a session: delete its row.
    pub fn browser_session_close(&self, digest: &str) -> Result<()> {
        self.session_write(|connection| {
            connection
                .execute("DELETE FROM browser_sessions WHERE digest = ?1", [digest])
                .map_err(|error| translate(error, connection))?;
            Ok(())
        })
    }

    /// Delete every expired row, when the table exists. A store with no table
    /// is left alone: a sweep never migrates a home on its own.
    pub fn browser_sessions_sweep(&self, now: i64) -> Result<usize> {
        let has = self
            .read(|connection| has_table(connection, "browser_sessions"))?
            .unwrap_or(false);
        if !has {
            return Ok(0);
        }
        self.session_write(|connection| sweep(connection, now))
    }
}

fn sweep(connection: &Connection, now: i64) -> Result<usize> {
    connection
        .execute(
            "DELETE FROM browser_sessions WHERE last_used_at < ?1",
            [now - SESSION_IDLE_SECONDS],
        )
        .map_err(|error| translate(error, connection))
}
