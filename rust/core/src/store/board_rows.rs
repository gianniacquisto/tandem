//! The board's rows: what the ticket domain reads, and the difference it writes
//! back.
//!
//! The domain above the store holds the board as one value and hands back the
//! value it changed (the read-modify-write the store has always done). This
//! module is the adapter's half of that contract: [`Store::board_rows`] reads the
//! three tables the board owns, and [`Write::save_board`] writes the difference
//! between the rows it read and the rows it was handed. Unchanged rows are left
//! alone, so an untouched ticket keeps the actor and the bytes of the write that
//! made it, and a no-op mutation writes no row at all.
//!
//! All SQL lives under the store, and this module is no exception:
//! nothing here is public outside the crate, and no table shape reaches the wire.

use rusqlite::params;

use super::schema::{has_column, has_table};
use super::write::{NewLabel, NewProject, NewTicket};
use super::{Result, Store, Write, translate};

/// One label row: its name (the key the board groups by), its ref prefix, the last
/// number it issued, its render position and its parent project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LabelRow {
    pub name: String,
    pub key: String,
    pub last_number: i64,
    pub position: i64,
    pub project: Option<String>,
}

/// One project row: its name and its render position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectRow {
    pub name: String,
    pub position: i64,
}

/// One ticket row. `position` is the stored order (ADR 0006) and `number` the
/// per-label number; the ref is derived from the label's key and that number by the
/// domain, never stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TicketRow {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub status: String,
    pub archived: bool,
    pub label: Option<String>,
    pub number: Option<i64>,
    pub origin: String,
    pub assignee: Option<String>,
    pub link: Option<String>,
    pub position: i64,
    pub created_at: String,
    pub completed_at: Option<String>,
    /// The body's length in characters, set only by a listing read that leaves
    /// `description` unread. `None` on every row a write reads or builds.
    pub description_length: Option<usize>,
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
    pub labels: Vec<LabelRow>,
    pub tickets: Vec<TicketRow>,
    pub edges: Vec<EdgeRow>,
    pub projects: Vec<ProjectRow>,
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

    /// The board and the revision it is a snapshot of, read in one transaction:
    /// a listing's rows and its number cannot be split by a concurrent writer.
    pub(crate) fn board_snapshot(&self) -> Result<Option<(BoardRows, i64)>> {
        self.read_snapshot(read_board)
    }

    /// The rows a listing needs and the revision they are a snapshot of, in one
    /// read transaction: the tickets the scope selects (their bodies unread
    /// unless `scope.with_description`), the far end of every edge they hold, and
    /// the small label, project and edge tables. See [`read_listing`].
    pub(crate) fn listing_snapshot(&self, scope: &ListScope) -> Result<Option<(BoardRows, i64)>> {
        self.read_snapshot(|connection| read_listing(connection, scope))
    }

    /// The rows one ticket's detail needs and the revision they are a snapshot
    /// of: the ticket whole, the far end of its edges, and the small tables. A key
    /// that names nothing reads the whole board, so the domain raises the failure
    /// it always raised. See [`read_one`].
    pub(crate) fn one_snapshot(&self, key: &TicketKey) -> Result<Option<(BoardRows, i64)>> {
        self.read_snapshot(|connection| read_one(connection, key))
    }

    /// One ticket's comments in append order, or `None` when there is no
    /// database yet.
    pub(crate) fn comment_rows(&self, ticket_id: &str) -> Result<Option<Vec<CommentRow>>> {
        self.read(|connection| read_comments(connection, ticket_id))
    }

    /// One ticket's comments and the revision they are a snapshot of, in one
    /// read transaction: the payload and its number cannot be split.
    pub(crate) fn comment_snapshot(
        &self,
        ticket_id: &str,
    ) -> Result<Option<(Vec<CommentRow>, i64)>> {
        self.read_snapshot(|connection| read_comments(connection, ticket_id))
    }
}

/// One ticket's comments in append order, on any connection: the store's read
/// path and a snapshot's own transaction both use it.
fn read_comments(connection: &rusqlite::Connection, ticket_id: &str) -> Result<Vec<CommentRow>> {
    let mut statement = connection
        .prepare("SELECT author, at, body FROM comments WHERE ticket_id = ?1 ORDER BY rowid")
        .map_err(|error| translate(error, connection))?;
    let rows = statement
        .query_map([ticket_id], |row| {
            Ok(CommentRow {
                author: row.get(0)?,
                at: row.get(1)?,
                body: row.get(2)?,
            })
        })
        .map_err(|error| translate(error, connection))?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|error| translate(error, connection))
}

/// The board as one read, on any connection: the store's read path and a write's
/// own transaction both use it, so a check inside a transaction sees the state the
/// transaction will commit over.
pub(crate) fn read_board(connection: &rusqlite::Connection) -> Result<BoardRows> {
    Ok(BoardRows {
        labels: labels(connection)?,
        tickets: tickets(connection)?,
        edges: edges(connection)?,
        projects: projects(connection)?,
    })
}

/// The table that holds the labels on this file: `labels` from schema v12, and
/// `lanes` on a file a read reaches before its first write migrates it (a read
/// never migrates, ADR 0031), so a v11 home still reads until then.
fn label_table(connection: &rusqlite::Connection) -> Result<&'static str> {
    Ok(if has_table(connection, "labels")? {
        "labels"
    } else {
        "lanes"
    })
}

fn labels(connection: &rusqlite::Connection) -> Result<Vec<LabelRow>> {
    let table = label_table(connection)?;
    if has_column(connection, table, "project")? {
        let mut statement = connection
            .prepare(&format!(
                "SELECT name, key, last_number, position, project FROM {table}
                 ORDER BY position, rowid"
            ))
            .map_err(|error| translate(error, connection))?;
        let rows = statement
            .query_map([], |row| {
                Ok(LabelRow {
                    name: row.get(0)?,
                    key: row.get(1)?,
                    last_number: row.get(2)?,
                    position: row.get(3)?,
                    project: row.get(4)?,
                })
            })
            .map_err(|error| translate(error, connection))?;
        return rows
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|error| translate(error, connection));
    }
    let mut statement = connection
        .prepare(&format!(
            "SELECT name, key, last_number, position FROM {table} ORDER BY position, rowid"
        ))
        .map_err(|error| translate(error, connection))?;
    let rows = statement
        .query_map([], |row| {
            Ok(LabelRow {
                name: row.get(0)?,
                key: row.get(1)?,
                last_number: row.get(2)?,
                position: row.get(3)?,
                project: None,
            })
        })
        .map_err(|error| translate(error, connection))?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|error| translate(error, connection))
}

fn projects(connection: &rusqlite::Connection) -> Result<Vec<ProjectRow>> {
    if !has_table(connection, "projects")? {
        return Ok(Vec::new());
    }
    let mut statement = connection
        .prepare("SELECT name, position FROM projects ORDER BY position, rowid")
        .map_err(|error| translate(error, connection))?;
    let rows = statement
        .query_map([], |row| {
            Ok(ProjectRow {
                name: row.get(0)?,
                position: row.get(1)?,
            })
        })
        .map_err(|error| translate(error, connection))?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|error| translate(error, connection))
}

fn tickets(connection: &rusqlite::Connection) -> Result<Vec<TicketRow>> {
    Ok(select_tickets(connection, "", &[], Body::Whole)?
        .into_iter()
        .map(|(_, row)| row)
        .collect())
}

/// How much of a ticket's body a read takes.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Body {
    /// The text itself.
    Whole,
    /// Its length in characters, not the text: `length()` on TEXT counts
    /// characters, as the domain's `chars().count()` does.
    Length,
    /// Neither: the row is wanted for its title and status alone.
    Skipped,
}

/// Ticket rows with their `rowid`, in stored order, for the `WHERE` tail given.
/// The one place the ticket columns are named, so the whole-board read and the
/// narrowed ones cannot disagree on a column.
fn select_tickets(
    connection: &rusqlite::Connection,
    tail: &str,
    values: &[&dyn rusqlite::ToSql],
    body: Body,
) -> Result<Vec<(i64, TicketRow)>> {
    // The column is `lane` until the file migrates to v12 (see `label_table`).
    let column = if label_table(connection)? == "labels" {
        "label"
    } else {
        "lane"
    };
    let (text, length) = match body {
        Body::Whole => ("description", "NULL"),
        Body::Length => ("NULL", "length(description)"),
        Body::Skipped => ("NULL", "NULL"),
    };
    let mut statement = connection
        .prepare(&format!(
            "SELECT id, title, {text}, status, archived, {column}, number, origin,
                    assignee, link, position, created_at, completed_at, {length}, rowid
             FROM tickets {tail} ORDER BY position, rowid"
        ))
        .map_err(|error| translate(error, connection))?;
    let rows = statement
        .query_map(values, |row| {
            Ok((
                row.get::<_, i64>(14)?,
                TicketRow {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    description: row.get(2)?,
                    status: row.get(3)?,
                    archived: row.get::<_, i64>(4)? != 0,
                    label: row.get(5)?,
                    number: row.get(6)?,
                    origin: row.get(7)?,
                    assignee: row.get(8)?,
                    link: row.get(9)?,
                    position: row.get(10)?,
                    created_at: row.get(11)?,
                    completed_at: row.get(12)?,
                    description_length: match body {
                        Body::Length => Some(
                            row.get::<_, Option<i64>>(13)?
                                .map_or(0, |length| length.max(0) as usize),
                        ),
                        _ => None,
                    },
                },
            ))
        })
        .map_err(|error| translate(error, connection))?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|error| translate(error, connection))
}

/// What a listing asks of the store: the axes SQL expresses exactly. The rest of
/// the query (text, status, done, blocked) the domain applies to the rows read, so
/// its semantics have one home.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ListScope {
    /// `Some(false)` the live rows, `Some(true)` the archive, `None` both.
    pub archived: Option<bool>,
    /// `Some(None)` the default area, `Some(Some(name))` one label.
    pub label: Option<Option<String>>,
    /// `Some(None)` unassigned, `Some(Some(name))` one assignee.
    pub assignee: Option<Option<String>>,
    /// Read the bodies: only a text filter needs them.
    pub with_description: bool,
}

/// The ticket a detail read names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TicketKey {
    /// A UUID in its stored (hyphenated, lowercase) form.
    Id(String),
    /// A label key (upper case) and a number.
    Ref { key: String, number: i64 },
}

/// A listing's rows. Only the tickets the scope selects are read, bodies unread
/// (their length instead) unless the scope wants them, plus the far end of every
/// edge those tickets hold (title and status, no body) so the blocked flag and
/// the block lists are true. Archived rows are not read unless the scope asks for
/// the archive, or an edge reaches one.
///
/// A row the narrowed read cannot make true alone (a labelled row with no number,
/// or a label the label table does not carry: the shapes `Contents::from_rows`
/// repairs by looking at the whole board) sends the read back to the whole board.
fn read_listing(connection: &rusqlite::Connection, scope: &ListScope) -> Result<BoardRows> {
    let column = if label_table(connection)? == "labels" {
        "label"
    } else {
        "lane"
    };
    let mut clauses: Vec<String> = Vec::new();
    let mut values: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(archived) = scope.archived {
        clauses.push(format!("archived = ?{}", values.len() + 1));
        values.push(Box::new(i64::from(archived)));
    }
    match &scope.label {
        None => {}
        Some(None) => clauses.push(format!("{column} IS NULL")),
        Some(Some(name)) => {
            clauses.push(format!("{column} = ?{}", values.len() + 1));
            values.push(Box::new(name.clone()));
        }
    }
    match &scope.assignee {
        None => {}
        Some(None) => clauses.push("assignee IS NULL".to_string()),
        Some(Some(name)) => {
            clauses.push(format!("assignee = ?{}", values.len() + 1));
            values.push(Box::new(name.clone()));
        }
    }
    let tail = if clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", clauses.join(" AND "))
    };
    let refs: Vec<&dyn rusqlite::ToSql> = values.iter().map(|value| value.as_ref()).collect();
    let body = if scope.with_description {
        Body::Whole
    } else {
        Body::Length
    };
    let matched = select_tickets(connection, &tail, &refs, body)?;
    narrowed(connection, matched)
}

/// One ticket's rows (see [`TicketKey`]); the whole board when the key names
/// nothing, or names more than one row.
fn read_one(connection: &rusqlite::Connection, key: &TicketKey) -> Result<BoardRows> {
    let column = if label_table(connection)? == "labels" {
        "label"
    } else {
        "lane"
    };
    let found = match key {
        TicketKey::Id(id) => select_tickets(
            connection,
            "WHERE id = ?1",
            &[id as &dyn rusqlite::ToSql],
            Body::Whole,
        )?,
        TicketKey::Ref { key, number } => {
            let table = label_table(connection)?;
            select_tickets(
                connection,
                &format!(
                    "WHERE number = ?2 AND {column} IN
                        (SELECT name FROM {table} WHERE upper(key) = upper(?1))"
                ),
                &[key as &dyn rusqlite::ToSql, number as &dyn rusqlite::ToSql],
                Body::Whole,
            )?
        }
    };
    if found.len() != 1 {
        return read_board(connection);
    }
    narrowed(connection, found)
}

/// The rows a read of `matched` needs: the matched tickets, the far end of each
/// edge they hold, and the small tables; or the whole board, when a matched or
/// far-end row is one the whole board would repair.
fn narrowed(
    connection: &rusqlite::Connection,
    matched: Vec<(i64, TicketRow)>,
) -> Result<BoardRows> {
    let labels = labels(connection)?;
    let all_edges = edges(connection)?;
    let held: std::collections::HashSet<&str> =
        matched.iter().map(|(_, row)| row.id.as_str()).collect();
    let touching: Vec<EdgeRow> = all_edges
        .iter()
        .filter(|edge| {
            held.contains(edge.ticket_id.as_str()) || held.contains(edge.depends_on_id.as_str())
        })
        .cloned()
        .collect();
    let mut far: Vec<String> = Vec::new();
    for edge in &touching {
        for end in [&edge.ticket_id, &edge.depends_on_id] {
            if !held.contains(end.as_str()) && !far.contains(end) {
                far.push(end.clone());
            }
        }
    }
    let mut rows = matched;
    for id in &far {
        rows.extend(select_tickets(
            connection,
            "WHERE id = ?1",
            &[id as &dyn rusqlite::ToSql],
            Body::Skipped,
        )?);
    }
    let needs_whole_board = rows.iter().any(|(_, row)| match &row.label {
        None => false,
        Some(name) => row.number.is_none() || !labels.iter().any(|label| &label.name == name),
    });
    if needs_whole_board {
        return read_board(connection);
    }
    rows.sort_by_key(|(rowid, row)| (row.position, *rowid));
    Ok(BoardRows {
        labels,
        tickets: rows.into_iter().map(|(_, row)| row).collect(),
        edges: touching,
        projects: projects(connection)?,
    })
}

fn edges(connection: &rusqlite::Connection) -> Result<Vec<EdgeRow>> {
    let mut statement = connection
        .prepare("SELECT ticket_id, depends_on_id FROM ticket_dependencies ORDER BY rowid")
        .map_err(|error| translate(error, connection))?;
    let rows = statement
        .query_map([], |row| {
            Ok(EdgeRow {
                ticket_id: row.get(0)?,
                depends_on_id: row.get(1)?,
            })
        })
        .map_err(|error| translate(error, connection))?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|error| translate(error, connection))
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
        for old in &before.projects {
            if !after.projects.iter().any(|new| new.name == old.name) {
                self.execute("DELETE FROM projects WHERE name = ?1", [&old.name])?;
            }
        }
        for new in &after.projects {
            match before.projects.iter().find(|old| old.name == new.name) {
                Some(old) if old == new => {}
                Some(_) => self.execute(
                    "UPDATE projects SET position = ?2, author = ?3
                     WHERE name = ?1
                       AND (position IS NOT ?2 OR author IS NOT ?3)",
                    params![new.name, new.position, self.actor().as_str()],
                )?,
                None => self.insert_project(NewProject {
                    name: &new.name,
                    position: new.position,
                })?,
            }
        }
        for old in &before.labels {
            if !after.labels.iter().any(|new| new.name == old.name) {
                self.execute("DELETE FROM labels WHERE name = ?1", [&old.name])?;
            }
        }
        for new in &after.labels {
            match before.labels.iter().find(|old| old.name == new.name) {
                Some(old) if old == new => {}
                Some(_) => self.execute(
                    "UPDATE labels SET key = ?2, last_number = ?3, position = ?4, project = ?5,
                            author = ?6
                     WHERE name = ?1
                       AND (key IS NOT ?2 OR last_number IS NOT ?3
                            OR position IS NOT ?4 OR project IS NOT ?5 OR author IS NOT ?6)",
                    params![
                        new.name,
                        new.key,
                        new.last_number,
                        new.position,
                        new.project,
                        self.actor().as_str()
                    ],
                )?,
                None => self.insert_label(NewLabel {
                    name: &new.name,
                    key: &new.key,
                    last_number: new.last_number,
                    position: new.position,
                    project: new.project.as_deref().unwrap_or(&new.name),
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
                            archived = ?5, label = ?6, number = ?7, origin = ?8,
                            assignee = ?9, link = ?10, position = ?11,
                            created_at = ?12, completed_at = ?13, author = ?14
                     WHERE id = ?1
                       AND (title IS NOT ?2 OR description IS NOT ?3
                            OR status IS NOT ?4 OR archived IS NOT ?5
                            OR label IS NOT ?6 OR number IS NOT ?7
                            OR origin IS NOT ?8 OR assignee IS NOT ?9
                            OR link IS NOT ?10 OR position IS NOT ?11
                            OR created_at IS NOT ?12 OR completed_at IS NOT ?13
                            OR author IS NOT ?14)",
                    params![
                        new.id,
                        new.title,
                        new.description,
                        new.status,
                        i64::from(new.archived),
                        new.label,
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
                    label: new.label.as_deref(),
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
                self.insert_dependency(super::write::NewDependency {
                    ticket_id: &new.ticket_id,
                    depends_on_id: &new.depends_on_id,
                })?;
            }
        }
        Ok(())
    }
}
