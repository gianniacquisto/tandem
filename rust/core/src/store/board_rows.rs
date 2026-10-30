//! The board's rows: what the ticket domain reads, and the difference it writes
//! back.
//!
//! The domain above the store holds the board as one value and hands back the
//! value it changed (the read-modify-write the Swift store has always done). This
//! module is the adapter's half of that contract: [`Store::board_rows`] reads the
//! three tables the board owns, and [`Write::save_board`] writes the difference
//! between the rows it read and the rows it was handed. Unchanged rows are left
//! alone, so an untouched ticket keeps the actor and the bytes of the write that
//! made it, and a no-op mutation writes no row at all.
//!
//! All SQL in the port lives under the store, and this module is no exception:
//! nothing here is public outside the crate, and no table shape reaches the wire.

use rusqlite::params;

use super::rows::{NewLane, NewTicket};
use super::{Result, Store, Write, translate};

/// One lane row: its name (the key the board groups by), its ref prefix, the last
/// number it issued and its render position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LaneRow {
    pub name: String,
    pub key: String,
    pub last_number: i64,
    pub position: i64,
}

/// One ticket row. `position` is the stored order (ADR 0006) and `number` the
/// per-lane number; the ref is derived from the lane's key and that number by the
/// domain, never stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TicketRow {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub archived: bool,
    pub lane: Option<String>,
    pub number: Option<i64>,
    pub origin: String,
    pub assignee: Option<String>,
    pub link: Option<String>,
    pub position: i64,
    pub created_at: String,
    pub completed_at: Option<String>,
}

/// One dependency edge, both ends a ticket's UUID. Its row order is the order the
/// edges were set, which is the order the board reads them back in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EdgeRow {
    pub ticket_id: String,
    pub depends_on_id: String,
}

/// The board as the store holds it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct BoardRows {
    pub lanes: Vec<LaneRow>,
    pub tickets: Vec<TicketRow>,
    pub edges: Vec<EdgeRow>,
}

/// One comment row, in the order it was appended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommentRow {
    pub author: String,
    pub at: String,
    pub body: String,
}

impl Store {
    /// The board, in stored order, or `None` when there is no database yet (a
    /// fresh home reads as an empty board, and reading creates nothing).
    pub(crate) fn board_rows(&self) -> Result<Option<BoardRows>> {
        self.read(read_board)
    }

    /// One ticket's comments in append order, or `None` when there is no
    /// database yet.
    pub(crate) fn comment_rows(&self, ticket_id: &str) -> Result<Option<Vec<CommentRow>>> {
        self.read(|connection| {
            let mut statement = connection
                .prepare(
                    "SELECT author, at, body FROM comments WHERE ticket_id = ?1 ORDER BY rowid",
                )
                .map_err(translate)?;
            let rows = statement
                .query_map([ticket_id], |row| {
                    Ok(CommentRow {
                        author: row.get(0)?,
                        at: row.get(1)?,
                        body: row.get(2)?,
                    })
                })
                .map_err(translate)?;
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(translate)
        })
    }
}

/// The board as one read, on any connection: the store's read path and a write's
/// own transaction both use it, so a check inside a transaction sees the state the
/// transaction will commit over.
pub(crate) fn read_board(connection: &rusqlite::Connection) -> Result<BoardRows> {
    Ok(BoardRows {
        lanes: lanes(connection)?,
        tickets: tickets(connection)?,
        edges: edges(connection)?,
    })
}

fn lanes(connection: &rusqlite::Connection) -> Result<Vec<LaneRow>> {
    let mut statement = connection
        .prepare("SELECT name, key, last_number, position FROM lanes ORDER BY position, rowid")
        .map_err(translate)?;
    let rows = statement
        .query_map([], |row| {
            Ok(LaneRow {
                name: row.get(0)?,
                key: row.get(1)?,
                last_number: row.get(2)?,
                position: row.get(3)?,
            })
        })
        .map_err(translate)?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(translate)
}

fn tickets(connection: &rusqlite::Connection) -> Result<Vec<TicketRow>> {
    let mut statement = connection
        .prepare(
            "SELECT id, title, description, status, archived, lane, number, origin,
                    assignee, link, position, created_at, completed_at
             FROM tickets ORDER BY position, rowid",
        )
        .map_err(translate)?;
    let rows = statement
        .query_map([], |row| {
            Ok(TicketRow {
                id: row.get(0)?,
                title: row.get(1)?,
                description: row.get(2)?,
                status: row.get(3)?,
                archived: row.get::<_, i64>(4)? != 0,
                lane: row.get(5)?,
                number: row.get(6)?,
                origin: row.get(7)?,
                assignee: row.get(8)?,
                link: row.get(9)?,
                position: row.get(10)?,
                created_at: row.get(11)?,
                completed_at: row.get(12)?,
            })
        })
        .map_err(translate)?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(translate)
}

fn edges(connection: &rusqlite::Connection) -> Result<Vec<EdgeRow>> {
    let mut statement = connection
        .prepare("SELECT ticket_id, depends_on_id FROM ticket_dependencies ORDER BY rowid")
        .map_err(translate)?;
    let rows = statement
        .query_map([], |row| {
            Ok(EdgeRow {
                ticket_id: row.get(0)?,
                depends_on_id: row.get(1)?,
            })
        })
        .map_err(translate)?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(translate)
}

impl Write<'_> {
    /// The board as the transaction sees it, for an operation that has to check a
    /// ticket still exists before it writes a row against it.
    pub(crate) fn board_rows(&self) -> Result<BoardRows> {
        read_board(self.connection)
    }

    /// Write the board's difference: rows gone are deleted, rows new are
    /// inserted, rows changed are updated. Everything a caller did not touch is
    /// left exactly as it was, so its author and its place in the table survive a
    /// write that did not concern it.
    pub(crate) fn save_board(&self, before: &BoardRows, after: &BoardRows) -> Result<()> {
        self.before_mutation()?;
        for old in &before.lanes {
            if !after.lanes.iter().any(|new| new.name == old.name) {
                self.execute("DELETE FROM lanes WHERE name = ?1", [&old.name])?;
            }
        }
        for new in &after.lanes {
            match before.lanes.iter().find(|old| old.name == new.name) {
                Some(old) if old == new => {}
                Some(_) => self.execute(
                    "UPDATE lanes SET key = ?2, last_number = ?3, position = ?4, author = ?5
                     WHERE name = ?1",
                    params![
                        new.name,
                        new.key,
                        new.last_number,
                        new.position,
                        self.actor().as_str()
                    ],
                )?,
                None => self.insert_lane(NewLane {
                    name: &new.name,
                    key: &new.key,
                    last_number: new.last_number,
                    position: new.position,
                })?,
            }
        }
        for old in &before.tickets {
            if !after.tickets.iter().any(|new| new.id == old.id) {
                self.execute("DELETE FROM tickets WHERE id = ?1", [&old.id])?;
            }
        }
        for new in &after.tickets {
            match before.tickets.iter().find(|old| old.id == new.id) {
                Some(old) if old == new => {}
                Some(_) => self.execute(
                    "UPDATE tickets SET title = ?2, description = ?3, status = ?4,
                            archived = ?5, lane = ?6, number = ?7, origin = ?8,
                            assignee = ?9, link = ?10, position = ?11,
                            created_at = ?12, completed_at = ?13, author = ?14
                     WHERE id = ?1",
                    params![
                        new.id,
                        new.title,
                        new.description,
                        new.status,
                        i64::from(new.archived),
                        new.lane,
                        new.number,
                        new.origin,
                        new.assignee,
                        new.link,
                        new.position,
                        new.created_at,
                        new.completed_at,
                        self.actor().as_str()
                    ],
                )?,
                None => self.insert_ticket(NewTicket {
                    id: &new.id,
                    title: &new.title,
                    description: new.description.as_deref(),
                    status: &new.status,
                    archived: new.archived,
                    lane: new.lane.as_deref(),
                    number: new.number,
                    origin: &new.origin,
                    assignee: new.assignee.as_deref(),
                    link: new.link.as_deref(),
                    position: new.position,
                    created_at: &new.created_at,
                    completed_at: new.completed_at.as_deref(),
                })?,
            }
        }
        // An edge's key is the pair of ids; its place in the table is the order
        // the edges were set, so a surviving edge is not touched.
        for old in &before.edges {
            if !after.edges.contains(old) {
                self.execute(
                    "DELETE FROM ticket_dependencies WHERE ticket_id = ?1 AND depends_on_id = ?2",
                    params![old.ticket_id, old.depends_on_id],
                )?;
            }
        }
        for new in &after.edges {
            if !before.edges.contains(new) {
                self.insert_dependency(super::rows::NewDependency {
                    ticket_id: &new.ticket_id,
                    depends_on_id: &new.depends_on_id,
                })?;
            }
        }
        Ok(())
    }
}
