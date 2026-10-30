//! The store seam: where a run keeps its state, and (from PRT-4) the SQLite
//! adapter that lives there.
//!
//! One home everywhere, `~/.tandem` on macOS, Linux and Windows, with
//! `TANDEM_HOME` the only override and `--home` the run's own (docs/port.md,
//! Store). This module owns the home; the adapter lands with PRT-4 and no SQL
//! leaves it.

mod home;

pub use home::{ENVIRONMENT, Home, HomeSource, user_home};

#[cfg(test)]
mod tests {
    //! The pinned engine has to link and answer on every runner, not only on
    //! the Mac the port was written on: `bundled` compiles SQLite from the
    //! crate, and a static library that fails to link is phase 1's floor
    //! missing. PRT-4 builds the adapter on this.

    #[test]
    fn bundled_sqlite_answers() {
        let connection = rusqlite::Connection::open_in_memory().expect("an in-memory engine opens");
        let version: String = connection
            .query_row("select sqlite_version()", [], |row| row.get(0))
            .expect("the engine answers");
        assert!(!version.is_empty(), "SQLite reports its version");
    }
}
