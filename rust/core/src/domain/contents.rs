//! The board in memory: the labels and the tickets, in stored order
//! //!
//! **Stored order is the order.** The ticket vector's position is authoritative
//! (new tickets land on top by default); it is never recomputed from `created_at`.
//! It is kept **grouped by label**: the default (unnamed) area first, then each
//! declared label in its declared order, so a reader can walk it once and a
//! hand-edited store stays legible.
//!
//! The dependency edges are the one thing a ticket cannot derive alone, so this
//! value keeps them as they were read (`edges`, dependent then blocker, in the
//! order they were set) and fills `blocked_by`/`blocks` from them.

use std::collections::HashMap;

use uuid::Uuid;

use crate::store::board_rows::{BoardRows, EdgeRow, LabelRow, TicketRow};
use crate::store::{decode, encode};

use super::error::DomainError;
use super::ticket::{Status, Ticket, TicketBlock};
use super::ticket_label::{Label, LabelKey, Project, TicketRef};

/// The store's contents: the ordered labels and the tickets, in stored order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Contents {
    pub labels: Vec<Label>,
    pub projects: Vec<Project>,
    pub tickets: Vec<Ticket>,
    /// Every dependency edge: (dependent, blocker), in the order it was set.
    pub edges: Vec<(Uuid, Uuid)>,
}

impl Contents {
    /// The board as the store holds it, with the refs, the label keys and the
    /// dependency halves made true: a read runs the same preparation a write does,
    /// and still writes nothing.
    pub(crate) fn from_rows(rows: &BoardRows) -> Result<Self, DomainError> {
        let mut labels: Vec<Label> = rows
            .labels
            .iter()
            .map(|label| Label {
                name: label.name.clone(),
                key: label.key.clone(),
                last_number: label.last_number,
                project: label.project.clone().unwrap_or_else(|| label.name.clone()),
            })
            .collect();
        let projects: Vec<Project> = rows
            .projects
            .iter()
            .map(|project| Project {
                name: project.name.clone(),
            })
            .collect();
        let tickets: Vec<Ticket> = rows
            .tickets
            .iter()
            .map(ticket_from_row)
            .collect::<Result<_, _>>()?;
        let edges: Vec<(Uuid, Uuid)> = rows
            .edges
            .iter()
            .map(|edge| {
                Ok((
                    row_id(&edge.ticket_id, "ticket_dependencies.ticket_id")?,
                    row_id(&edge.depends_on_id, "ticket_dependencies.depends_on_id")?,
                ))
            })
            .collect::<Result<_, DomainError>>()?;
        // A label a ticket names but the label list does not carry (a hand edit) is
        // declared so it can be keyed and numbered, in first-appearance order.
        for ticket in &tickets {
            let Some(name) = &ticket.label else { continue };
            if !labels.iter().any(|label| &label.name == name) {
                labels.push(Label::new(name.clone(), ""));
            }
        }
        let mut contents = Self {
            labels,
            projects,
            tickets,
            edges,
        };
        contents.normalize();
        contents.prepare();
        // Empty projects stay listed: `projects` started as the rows' stored
        // order, and `declare_projects` only appends, so a project no label
        // names deletes explicitly, never by losing its last label in passing.
        // A project row the labels no longer name but the table still carries
        // keeps its place, in row order.
        for row in &rows.projects {
            if !contents
                .projects
                .iter()
                .any(|project| project.name == row.name)
            {
                contents.projects.push(Project::new(row.name.clone()));
            }
        }
        Ok(contents)
    }

    /// The board as rows, in stored order, ready for the adapter's diff.
    pub(crate) fn to_rows(&self) -> BoardRows {
        BoardRows {
            labels: self
                .labels
                .iter()
                .enumerate()
                .map(|(position, label)| LabelRow {
                    name: label.name.clone(),
                    key: label.key.clone(),
                    last_number: label.last_number,
                    position: position as i64,
                    project: Some(label.project.clone()),
                })
                .collect(),
            projects: self
                .projects
                .iter()
                .enumerate()
                .map(|(position, project)| crate::store::board_rows::ProjectRow {
                    name: project.name.clone(),
                    position: position as i64,
                })
                .collect(),
            tickets: self
                .tickets
                .iter()
                .enumerate()
                .map(|(position, ticket)| ticket_to_row(ticket, position as i64))
                .collect(),
            edges: self
                .edges
                .iter()
                .map(|(dependent, blocker)| EdgeRow {
                    ticket_id: dependent.to_string(),
                    depends_on_id: blocker.to_string(),
                })
                .collect(),
        }
    }

    /// Every label key in render order: the default area, the declared labels, then
    /// any name that appears only on a ticket (a hand edit), in first-appearance
    /// order, so a hand-edited store loses no grouping.
    pub fn ordered_keys(&self) -> Vec<Option<String>> {
        let mut keys: Vec<Option<String>> = vec![None];
        for label in &self.labels {
            if !keys.contains(&Some(label.name.clone())) {
                keys.push(Some(label.name.clone()));
            }
        }
        for ticket in &self.tickets {
            if let Some(label) = &ticket.label
                && !keys.contains(&Some(label.clone()))
            {
                keys.push(Some(label.clone()));
            }
        }
        keys
    }

    /// Every name that is safe to file into: each declared label plus any dangling
    /// name already on a ticket.
    pub fn known_labels(&self) -> Vec<String> {
        self.ordered_keys().into_iter().flatten().collect()
    }

    /// Every project in render order: the stored order, plus any parent a label
    /// names but the table does not yet carry (a hand edit), in label order.
    pub fn known_projects(&self) -> Vec<String> {
        let mut known: Vec<String> = self.projects.iter().map(|p| p.name.clone()).collect();
        for label in &self.labels {
            if !known.iter().any(|name| name == &label.project) {
                known.push(label.project.clone());
            }
        }
        known
    }

    /// Every project with its labels, in render order: projects in stored
    /// order, their labels beneath in label order.
    pub fn projects_with_labels(&self) -> Vec<super::ticket_label::ProjectWithLabels> {
        self.known_projects()
            .into_iter()
            .map(|name| {
                let labels = self
                    .labels
                    .iter()
                    .filter(|label| label.project == name)
                    .map(|label| label.name.clone())
                    .collect();
                super::ticket_label::ProjectWithLabels { name, labels }
            })
            .collect()
    }

    /// The index at which `key`'s block begins: the cumulative length of every
    /// preceding group. For a key that is neither declared nor present, the end of
    /// the list.
    pub fn block_start(&self, key: Option<&str>) -> usize {
        let mut start = 0;
        for candidate in self.ordered_keys() {
            if candidate.as_deref() == key {
                return start;
            }
            start += self
                .tickets
                .iter()
                .filter(|ticket| ticket.label.as_deref() == candidate.as_deref())
                .count();
        }
        start
    }

    /// Re-establish the grouping invariant, stably: groups in `ordered_keys` order,
    /// each keeping its relative order. Run before every read-modify-write body and
    /// again before the write, so the invariant holds for every operation.
    pub fn normalize(&mut self) {
        let keys = self.ordered_keys();
        let mut grouped: Vec<Ticket> = Vec::with_capacity(self.tickets.len());
        for key in keys {
            grouped.extend(
                self.tickets
                    .iter()
                    .filter(|ticket| ticket.label == key)
                    .cloned(),
            );
        }
        self.tickets = grouped;
        self.refresh_refs();
        self.refresh_blocked();
    }

    /// Make the ref scheme true for contents that predate it, idempotently: every
    /// ticket's label is declared, every label has a key, and a ticket in a label
    /// without a number gets the label's next one in `created_at` order (ties by
    /// id). Every label has exactly one parent project: a hand-added label
    /// declares the project named for it the same way it already declares the
    /// label itself.
    pub fn prepare(&mut self) {
        self.assign_keys();
        self.declare_projects();
        self.number_unnumbered();
        self.refresh_refs();
        self.refresh_blocked();
    }

    /// Every label without a key gets one, in render order, among the keys already
    /// taken.
    fn assign_keys(&mut self) {
        let mut taken: Vec<String> = self
            .labels
            .iter()
            .map(|label| label.key.to_uppercase())
            .filter(|key| !key.is_empty())
            .collect();
        for label in &mut self.labels {
            if label.key.is_empty() {
                let key = LabelKey::generate(&label.name, &taken);
                taken.push(key.clone());
                label.key = key;
            }
        }
    }

    /// A label without a parent declares the project named for it, in label order,
    /// so a hand-edited store loses no grouping. An empty parent reads as
    /// absent, never as a project named "".
    fn declare_projects(&mut self) {
        for label in &mut self.labels {
            if label.project.trim().is_empty() {
                label.project = label.name.clone();
            }
        }
        for label in &self.labels.clone() {
            if !self
                .projects
                .iter()
                .any(|project| project.name == label.project)
            {
                self.projects.push(Project::new(label.project.clone()));
            }
        }
    }

    /// A ticket in a label without a number takes the label's next one, oldest
    /// first: the backfill an installed board needs once.
    fn number_unnumbered(&mut self) {
        for i in 0..self.labels.len() {
            let name = self.labels[i].name.clone();
            let mut highest = self.labels[i].last_number;
            let mut missing: Vec<usize> = self
                .tickets
                .iter()
                .enumerate()
                .filter(|(_, ticket)| {
                    ticket.label.as_deref() == Some(name.as_str()) && ticket.number.is_none()
                })
                .map(|(index, _)| index)
                .collect();
            missing.sort_by(|left, right| {
                let (left, right) = (&self.tickets[*left], &self.tickets[*right]);
                left.created_at
                    .cmp(&right.created_at)
                    .then_with(|| left.id.cmp(&right.id))
            });
            for index in missing {
                highest += 1;
                self.tickets[index].number = Some(highest);
            }
            self.labels[i].last_number = self.labels[i].last_number.max(highest);
        }
    }

    /// The derived half: a ticket's `reference` is its label's key and its own
    /// number, or `None` when either is missing (the default area has neither).
    pub fn refresh_refs(&mut self) {
        let mut keys: HashMap<&str, &str> = HashMap::new();
        for label in &self.labels {
            if !label.key.is_empty() {
                keys.insert(label.name.as_str(), label.key.as_str());
            }
        }
        let rendered: Vec<Option<String>> = self
            .tickets
            .iter()
            .map(|ticket| {
                let name = ticket.label.as_deref()?;
                let key = keys.get(name)?;
                let number = ticket.number?;
                Some(TicketRef::render(key, number))
            })
            .collect();
        for (ticket, reference) in self.tickets.iter_mut().zip(rendered) {
            ticket.reference = reference;
        }
    }

    /// The dependency half: each ticket's `blocked_by` names the other ticket as it
    /// is now (its ref, its title, its status), and `blocks` is the same edges read
    /// from the far end, in stored order.
    pub fn refresh_blocked(&mut self) {
        let shown: HashMap<Uuid, TicketBlock> = self
            .tickets
            .iter()
            .map(|ticket| {
                (
                    ticket.id,
                    TicketBlock {
                        id: ticket.id,
                        reference: ticket.reference.clone(),
                        title: ticket.title.clone(),
                        status: ticket.status,
                    },
                )
            })
            .collect();
        let edges = self.edges.clone();
        let mut near: HashMap<Uuid, Vec<TicketBlock>> = HashMap::new();
        let mut far: HashMap<Uuid, Vec<TicketBlock>> = HashMap::new();
        // The near side keeps the order the edges were set in.
        for (dependent, blocker) in &edges {
            if let Some(blocker_block) = shown.get(blocker) {
                near.entry(*dependent)
                    .or_default()
                    .push(blocker_block.clone());
            }
        }
        // The far side has no order of its own to store, so it is the board's:
        // the same order every read already follows (ADR 0006).
        for ticket in &self.tickets {
            let Some(self_block) = shown.get(&ticket.id) else {
                continue;
            };
            for (dependent, blocker) in &edges {
                if *dependent == ticket.id {
                    far.entry(*blocker).or_default().push(self_block.clone());
                }
            }
        }
        for ticket in &mut self.tickets {
            ticket.blocked_by = near.get(&ticket.id).cloned().unwrap_or_default();
            ticket.blocks = far.get(&ticket.id).cloned().unwrap_or_default();
        }
    }

    /// Issue the label's next number: the counter rises and never goes back, so a
    /// number a moved-away ticket used is not handed out again.
    pub fn issue_number(&mut self, label: &str) -> i64 {
        let Some(i) = self
            .labels
            .iter()
            .position(|candidate| candidate.name == label)
        else {
            return 0;
        };
        self.labels[i].last_number += 1;
        self.labels[i].last_number
    }

    /// The key a label files under, or `None` when the label is not declared.
    pub fn key_of(&self, label: &str) -> Option<&str> {
        self.labels
            .iter()
            .find(|candidate| candidate.name == label)
            .map(|candidate| candidate.key.as_str())
    }

    /// Guards a label argument: `None` (the default area) is always valid; any
    /// other name must be a known label.
    pub fn validate_label(&self, label: Option<&str>) -> Result<(), DomainError> {
        let Some(label) = label else { return Ok(()) };
        if self.known_labels().iter().any(|name| name == label) {
            Ok(())
        } else {
            Err(DomainError::UnknownLabel(label.to_string()))
        }
    }

    /// The store's one rule for "which ticket is this id?": a full UUID, or the
    /// ticket's ref (`TAN-42`, case-insensitive on the key), and its index.
    pub fn resolve_ticket(&self, raw: &str) -> Option<(Uuid, usize)> {
        if let Ok(id) = Uuid::parse_str(raw)
            && let Some(index) = self.tickets.iter().position(|ticket| ticket.id == id)
        {
            return Some((id, index));
        }
        let (key, number) = TicketRef::parse(raw)?;
        let index = self.tickets.iter().position(|ticket| {
            ticket.number == Some(number)
                && ticket
                    .label
                    .as_deref()
                    .and_then(|label| self.key_of(label))
                    .is_some_and(|label_key| label_key.eq_ignore_ascii_case(&key))
        })?;
        Some((self.tickets[index].id, index))
    }

    /// The ticket a handle names, or the failure every id-taking operation raises.
    /// A handle that looks like a ref but names nothing gets the ref-specific
    /// failure: it names the label's real range, so the caller recovers in one turn.
    pub fn require_ticket(&self, raw: &str) -> Result<(Uuid, usize), DomainError> {
        if let Some(found) = self.resolve_ticket(raw) {
            return Ok(found);
        }
        if let Some((key, _)) = TicketRef::parse(raw) {
            return Err(DomainError::UnknownRef {
                reference: raw.to_string(),
                hint: self.ref_hint(&key),
            });
        }
        Err(DomainError::UnknownId {
            id: raw.to_string(),
            open: self
                .tickets
                .iter()
                .filter(|ticket| !ticket.done())
                .cloned()
                .collect(),
        })
    }

    /// What the unknown-ref failure says next: whether the key exists at all, and
    /// the range the label has issued.
    fn ref_hint(&self, key: &str) -> String {
        let Some(label) = self
            .labels
            .iter()
            .find(|label| label.key.eq_ignore_ascii_case(key))
        else {
            let keys: Vec<&str> = self
                .labels
                .iter()
                .map(|label| label.key.as_str())
                .filter(|key| !key.is_empty())
                .collect();
            if keys.is_empty() {
                return "No label has that key, and no label carries a key yet.".to_string();
            }
            return format!(
                "No label has the key \"{key}\". The keys in use: {}.",
                keys.join(", ")
            );
        };
        if label.last_number <= 0 {
            return format!(
                "Label \"{}\" ({}) has issued no numbers yet.",
                label.name, label.key
            );
        }
        format!(
            "Label \"{}\" ({}) has issued {}-1 to {}-{}.",
            label.name, label.key, label.key, label.key, label.last_number
        )
    }
}

/// The id a row's text names, or the failure that says which row could not be
/// read.
fn row_id(text: &str, column: &str) -> Result<Uuid, DomainError> {
    Uuid::parse_str(text)
        .map_err(|_| DomainError::Unreadable(format!("{column} carries an invalid id \"{text}\"")))
}

fn ticket_from_row(row: &TicketRow) -> Result<Ticket, DomainError> {
    Ok(Ticket {
        id: row_id(&row.id, "tickets.id")?,
        title: row.title.clone(),
        status: Status::stored(&row.status),
        archived: row.archived,
        description: row.description.clone(),
        assignee: row.assignee.clone(),
        origin: row.origin.clone(),
        label: row.label.clone(),
        number: row.number,
        reference: None,
        link: row.link.clone(),
        blocked_by: Vec::new(),
        blocks: Vec::new(),
        created_at: decode(&row.created_at).ok_or_else(|| {
            DomainError::Unreadable(format!(
                "a ticket row carries an unreadable created_at \"{}\"",
                row.created_at
            ))
        })?,
        completed_at: match row.completed_at.as_deref() {
            Some(raw) => Some(decode(raw).ok_or_else(|| {
                DomainError::Unreadable(format!(
                    "a ticket row carries an unreadable completed_at \"{raw}\""
                ))
            })?),
            None => None,
        },
    })
}

fn ticket_to_row(ticket: &Ticket, position: i64) -> TicketRow {
    TicketRow {
        id: ticket.id.to_string(),
        title: ticket.title.clone(),
        description: ticket.description.clone(),
        status: ticket.status.as_str().to_string(),
        archived: ticket.archived,
        label: ticket.label.clone(),
        number: ticket.number,
        origin: ticket.origin.clone(),
        assignee: ticket.assignee.clone(),
        link: ticket.link.clone(),
        position,
        created_at: encode(ticket.created_at),
        completed_at: ticket.completed_at.map(encode),
    }
}
