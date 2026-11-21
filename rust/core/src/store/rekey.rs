//! The one-prefix-per-project re-key (schema v16, ADR 0053).
//!
//! Before v16 a **label** owned the ref prefix and a counter, so one project
//! issued several prefixes. From v16 the **project** owns one key and one
//! counter, and a ticket stores its project. This module is the arithmetic of
//! that move, in one place:
//!
//! - [`rekey`] is a pure function from the pre-v16 board ([`LegacyBoard`]) to the
//!   v16 rows. The migration writes its answer; a read of a file the migration
//!   has not reached yet runs the same function in memory, so a read before the
//!   first write shows the refs the write will leave behind.
//! - [`read_legacy`] reads the pre-v16 shapes (`lanes` or `labels`, the ticket
//!   column `lane` or `label`, a project table that may be absent).
//! - [`migrate`] is the v16 migration body, inside the one transaction the seam
//!   gives it. Every SQL value is bound.
//!
//! **The re-key, per project.** The project's key is the key of the label named
//! like the project, when that label has one; otherwise the key of the label with
//! the highest counter (ties go to label position); a project with no label gets
//! a key generated from its name over every taken key. The project's counter
//! starts at the chosen label's counter and the chosen label's tickets keep their
//! numbers. Every other numbered ticket takes the project's next number in
//! `created_at` order, then id, and its old ref is recorded as an alias. Tickets
//! that never had a number (the default area, a hand edit) are numbered after
//! them, in the same order, with no alias, because they never had a ref.

use rusqlite::{Connection, params};

use super::board_rows::{AliasRow, BoardRows, EdgeRow, LabelRow, ProjectRow, TicketRow};
use crate::domain::ticket_label::ProjectKey;

/// The project a home without any project files its project-less tickets under.
pub(crate) const INBOX: &str = "Inbox";

/// One pre-v16 label row: it owned a key and a counter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LegacyLabel {
    pub name: String,
    pub key: String,
    pub last_number: i64,
    pub position: i64,
    pub project: Option<String>,
}

/// The board as a pre-v16 file holds it. The tickets carry no project.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LegacyBoard {
    pub labels: Vec<LegacyLabel>,
    /// Name and position, in render order.
    pub projects: Vec<(String, i64)>,
    pub tickets: Vec<TicketRow>,
    pub edges: Vec<EdgeRow>,
}

struct Plan {
    name: String,
    position: i64,
    key: String,
    last_number: i64,
    chosen: Option<String>,
}

/// The v16 rows a legacy board becomes.
pub(crate) fn rekey(legacy: LegacyBoard) -> BoardRows {
    let LegacyBoard {
        mut labels,
        projects: stored_projects,
        tickets,
        edges,
    } = legacy;

    // A label a ticket names but the label table does not carry (a hand edit) is
    // declared, so no ticket is left without a home.
    let mut next_position = labels
        .iter()
        .map(|label| label.position)
        .max()
        .unwrap_or(-1)
        + 1;
    for ticket in &tickets {
        let Some(name) = &ticket.label else { continue };
        if !labels.iter().any(|label| &label.name == name) {
            labels.push(LegacyLabel {
                name: name.clone(),
                key: String::new(),
                last_number: 0,
                position: next_position,
                project: None,
            });
            next_position += 1;
        }
    }

    // Every label has exactly one parent: an absent or empty one reads as the
    // project named for the label.
    for label in &mut labels {
        let parent = label.project.as_deref().map(str::trim).unwrap_or("");
        label.project = Some(if parent.is_empty() {
            label.name.clone()
        } else {
            parent.to_string()
        });
    }

    // The keys taken: every label key, retired or not, because a retired key
    // becomes an alias prefix and may never name a second project.
    let mut taken: Vec<String> = labels
        .iter()
        .map(|label| label.key.to_uppercase())
        .filter(|key| !key.is_empty())
        .collect();
    for label in &mut labels {
        if label.key.is_empty() {
            let key = ProjectKey::generate(&label.name, &taken);
            taken.push(key.clone());
            label.key = key;
        }
    }

    // The projects in render order: the stored ones, then any a label names that
    // the table does not carry.
    let mut projects: Vec<(String, i64)> = stored_projects;
    let mut position = projects
        .iter()
        .map(|(_, position)| *position)
        .max()
        .unwrap_or(-1)
        + 1;
    for label in &labels {
        let parent = label.project.clone().unwrap_or_default();
        if !projects.iter().any(|(name, _)| *name == parent) {
            projects.push((parent, position));
            position += 1;
        }
    }
    // A home with project-less tickets and no project at all gets an Inbox.
    if projects.is_empty() && tickets.iter().any(|ticket| ticket.label.is_none()) {
        projects.push((INBOX.to_string(), 0));
    }

    // Each project's chosen label, key and counter.
    let mut plans: Vec<Plan> = Vec::new();
    for (name, position) in &projects {
        let own: Vec<&LegacyLabel> = labels
            .iter()
            .filter(|label| label.project.as_deref() == Some(name.as_str()))
            .collect();
        let mut chosen: Option<&LegacyLabel> = own
            .iter()
            .copied()
            .find(|label| &label.name == name && !label.key.is_empty());
        if chosen.is_none() {
            for candidate in &own {
                if chosen.is_none_or(|best| candidate.last_number > best.last_number) {
                    chosen = Some(candidate);
                }
            }
        }
        let (key, last_number, chosen_name) = match chosen {
            Some(label) => {
                let held = tickets
                    .iter()
                    .filter(|ticket| ticket.label.as_deref() == Some(label.name.as_str()))
                    .filter_map(|ticket| ticket.number)
                    .max()
                    .unwrap_or(0);
                (
                    label.key.to_uppercase(),
                    label.last_number.max(held),
                    Some(label.name.clone()),
                )
            }
            None => (String::new(), 0, None),
        };
        plans.push(Plan {
            name: name.clone(),
            position: *position,
            key,
            last_number,
            chosen: chosen_name,
        });
    }
    // A label-less project takes a generated key after every label key is taken.
    for plan in &mut plans {
        if plan.key.is_empty() {
            let key = ProjectKey::generate(&plan.name, &taken);
            taken.push(key.clone());
            plan.key = key;
        }
    }

    // The tickets: each takes its label's project, or the first project when it
    // has no label.
    let first = projects.first().map(|(name, _)| name.clone());
    let parent_of = |ticket: &TicketRow| -> Option<String> {
        match &ticket.label {
            Some(label) => labels
                .iter()
                .find(|candidate| &candidate.name == label)
                .and_then(|candidate| candidate.project.clone()),
            None => first.clone(),
        }
    };
    let mut out_tickets: Vec<TicketRow> = tickets.clone();
    let mut aliases: Vec<AliasRow> = Vec::new();
    for plan in &mut plans {
        let mut retired: Vec<(usize, String)> = Vec::new();
        let mut fresh: Vec<usize> = Vec::new();
        for (index, ticket) in tickets.iter().enumerate() {
            if parent_of(ticket).as_deref() != Some(plan.name.as_str()) {
                continue;
            }
            let in_chosen = ticket.label.is_some() && ticket.label == plan.chosen;
            match (ticket.number, in_chosen) {
                (Some(_), true) => {}
                (Some(number), false) if ticket.label.is_some() => {
                    let old_key = labels
                        .iter()
                        .find(|candidate| Some(&candidate.name) == ticket.label.as_ref())
                        .map(|candidate| candidate.key.to_uppercase())
                        .unwrap_or_default();
                    retired.push((index, format!("{old_key}-{number}")));
                }
                _ => fresh.push(index),
            }
        }
        let by_age = |left: usize, right: usize| {
            tickets[left]
                .created_at
                .cmp(&tickets[right].created_at)
                .then_with(|| tickets[left].id.cmp(&tickets[right].id))
        };
        retired.sort_by(|left, right| by_age(left.0, right.0));
        fresh.sort_by(|left, right| by_age(*left, *right));
        for (index, reference) in retired {
            plan.last_number += 1;
            out_tickets[index].number = Some(plan.last_number);
            aliases.push(AliasRow {
                reference,
                ticket_id: out_tickets[index].id.clone(),
            });
        }
        for index in fresh {
            plan.last_number += 1;
            out_tickets[index].number = Some(plan.last_number);
        }
        for (index, ticket) in tickets.iter().enumerate() {
            if parent_of(ticket).as_deref() == Some(plan.name.as_str()) {
                out_tickets[index].project = Some(plan.name.clone());
            }
        }
    }

    BoardRows {
        labels: labels
            .iter()
            .map(|label| LabelRow {
                name: label.name.clone(),
                position: label.position,
                project: label.project.clone(),
            })
            .collect(),
        projects: plans
            .iter()
            .map(|plan| ProjectRow {
                name: plan.name.clone(),
                key: plan.key.clone(),
                last_number: plan.last_number,
                position: plan.position,
            })
            .collect(),
        tickets: out_tickets,
        edges,
        aliases,
    }
}

// MARK: - Reading the pre-v16 shapes

fn column_exists(connection: &Connection, table: &str, column: &str) -> rusqlite::Result<bool> {
    connection
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2",
            params![table, column],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
}

fn table_exists(connection: &Connection, table: &str) -> rusqlite::Result<bool> {
    connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |row| row.get::<_, i64>(0),
        )
        .map(|count| count > 0)
}

/// Whether a file predates v16: its projects table has no key column (or there is
/// no projects table at all, v5 and below).
pub(crate) fn is_legacy(connection: &Connection) -> rusqlite::Result<bool> {
    Ok(!column_exists(connection, "projects", "key")?)
}

/// The board of a pre-v16 file. The table is `lanes` until v12 and `labels`
/// after, the ticket column `lane` and `label` with it, and the label's
/// `project` arrived at v6.
pub(crate) fn read_legacy(connection: &Connection) -> rusqlite::Result<LegacyBoard> {
    let (table, column) = if table_exists(connection, "labels")? {
        ("labels", "label")
    } else {
        ("lanes", "lane")
    };
    let parent = if column_exists(connection, table, "project")? {
        "project"
    } else {
        "NULL"
    };
    let mut statement = connection.prepare(&format!(
        "SELECT name, key, last_number, position, {parent} FROM {table}
         ORDER BY position, rowid"
    ))?;
    let labels = statement
        .query_map([], |row| {
            Ok(LegacyLabel {
                name: row.get(0)?,
                key: row.get(1)?,
                last_number: row.get(2)?,
                position: row.get(3)?,
                project: row.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);

    let projects = if table_exists(connection, "projects")? {
        let mut statement =
            connection.prepare("SELECT name, position FROM projects ORDER BY position, rowid")?;
        statement
            .query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        Vec::new()
    };

    let mut statement = connection.prepare(&format!(
        "SELECT id, title, description, status, archived, {column}, number, origin,
                assignee, link, position, created_at, completed_at
         FROM tickets ORDER BY position, rowid"
    ))?;
    let tickets = statement
        .query_map([], |row| {
            Ok(TicketRow {
                id: row.get(0)?,
                title: row.get(1)?,
                description: row.get(2)?,
                status: row.get(3)?,
                archived: row.get::<_, i64>(4)? != 0,
                project: None,
                label: row.get(5)?,
                number: row.get(6)?,
                origin: row.get(7)?,
                assignee: row.get(8)?,
                link: row.get(9)?,
                position: row.get(10)?,
                created_at: row.get(11)?,
                completed_at: row.get(12)?,
                description_length: None,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);

    let mut statement = connection
        .prepare("SELECT ticket_id, depends_on_id FROM ticket_dependencies ORDER BY rowid")?;
    let edges = statement
        .query_map([], |row| {
            Ok(EdgeRow {
                ticket_id: row.get(0)?,
                depends_on_id: row.get(1)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(statement);

    Ok(LegacyBoard {
        labels,
        projects,
        tickets,
        edges,
    })
}

// MARK: - The migration

/// Schema v16: re-key every project, store each ticket's project, record the
/// retired refs, and drop the label's key and counter so no second copy of the
/// counter survives.
///
/// Runs inside the migration seam's transaction: a failure anywhere rolls the
/// whole of it back and the file stays at v15. No ticket, comment, edge or label
/// row is deleted; ids, comments, edges and positions are untouched.
pub(crate) fn migrate(connection: &Connection) -> rusqlite::Result<()> {
    let plan = rekey(read_legacy(connection)?);

    connection.execute_batch(
        "ALTER TABLE projects ADD COLUMN key TEXT;
         ALTER TABLE projects ADD COLUMN last_number INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE tickets ADD COLUMN project TEXT;
         CREATE TABLE ticket_aliases (
             reference TEXT PRIMARY KEY,
             ticket_id TEXT NOT NULL,
             author    TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS tickets_project_number ON tickets (project, number);
         CREATE INDEX IF NOT EXISTS ticket_aliases_ticket ON ticket_aliases (ticket_id);",
    )?;

    for project in &plan.projects {
        connection.execute(
            "INSERT INTO projects (name, key, last_number, position, author)
             VALUES (?1, ?2, ?3, ?4, 'human')
             ON CONFLICT(name) DO UPDATE SET key = excluded.key,
                                             last_number = excluded.last_number",
            params![
                project.name,
                project.key,
                project.last_number,
                project.position
            ],
        )?;
    }
    for ticket in &plan.tickets {
        connection.execute(
            "UPDATE tickets SET project = ?2, number = ?3 WHERE id = ?1",
            params![ticket.id, ticket.project, ticket.number],
        )?;
    }
    for alias in &plan.aliases {
        connection.execute(
            "INSERT OR IGNORE INTO ticket_aliases (reference, ticket_id, author)
             SELECT ?1, id, author FROM tickets WHERE id = ?2",
            params![alias.reference.to_uppercase(), alias.ticket_id],
        )?;
    }

    let table = if table_exists(connection, "labels")? {
        "labels"
    } else {
        "lanes"
    };
    connection.execute_batch(&format!(
        "ALTER TABLE {table} DROP COLUMN key;
         ALTER TABLE {table} DROP COLUMN last_number;"
    ))?;
    for label in &plan.labels {
        connection.execute(
            &format!(
                "INSERT OR IGNORE INTO {table} (name, position, project, author)
                 VALUES (?1, ?2, ?3, 'human')"
            ),
            params![label.name, label.position, label.project],
        )?;
    }
    Ok(())
}
