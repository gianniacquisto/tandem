//! One view over the tickets (ADR 0017).
//!
//! The single definition of "which tickets are in view", shared by the CLI's flags
//! and the app's filter row, so "the ready tickets" cannot mean one thing to a
//! human and another to an agent. It is a pure predicate: it never reorders (the
//! stored order is the order) and it never writes.

use crate::store::board_rows::ListScope;

use super::error::DomainError;
use super::text::TextMatch;
use super::ticket::{Status, Ticket};

/// The assignee axis: anyone, the unassigned tickets, or one named owner.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AssigneeAxis {
    Any,
    Unassigned,
    Named(String),
}

/// The label axis: any label, no label (inside any project), or one named label.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LabelAxis {
    Any,
    Unnamed,
    Named(String),
}

/// The archive axis is three-valued because both halves are real asks: "hide
/// finished work" is the default, and "show me only the archive" is how it is
/// undone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Archived {
    Excluded,
    Included,
    Only,
}

/// One view over the tickets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TicketQuery {
    /// Free text, matched case- and diacritic-insensitively against the **title and
    /// the description**. `None` or whitespace-only matches everything.
    pub text: Option<String>,
    /// The statuses in view; empty means every status.
    pub statuses: Vec<Status>,
    /// Restrict to finished or unfinished tickets: the shipped `done`/`open`
    /// filters. `None` means either. Derived from the ticket's finished key, so
    /// a custom finished stage is found without naming its literal key.
    pub done: Option<bool>,
    pub assignee: AssigneeAxis,
    /// One project by name, or `None` for every project.
    pub project: Option<String>,
    pub label: LabelAxis,
    pub archived: Archived,
    /// Only the tickets waiting on an open blocker (TAN-232).
    pub blocked_only: bool,
}

/// The reserved assignee: the person using the app, as opposed to an agent. Stored
/// lowercase so it is one name.
pub const HUMAN: &str = "human";

impl Default for TicketQuery {
    fn default() -> Self {
        Self {
            text: None,
            statuses: Vec::new(),
            done: None,
            assignee: AssigneeAxis::Any,
            project: None,
            label: LabelAxis::Any,
            archived: Archived::Excluded,
            blocked_only: false,
        }
    }
}

impl TicketQuery {
    /// The human's inbox: every open, non-archived ticket assigned to the person.
    ///
    /// The queue is the assignee alone.
    pub fn inbox() -> Self {
        Self {
            done: Some(false),
            assignee: AssigneeAxis::Named(HUMAN.to_string()),
            archived: Archived::Excluded,
            ..Self::default()
        }
    }

    /// The axes the store can select on exactly, so a listing reads only the rows
    /// that can match. Everything else (text, status, done, blocked) stays in
    /// [`matches`](Self::matches), the one home of those semantics, applied to the
    /// rows read.
    pub(crate) fn scope(&self) -> ListScope {
        ListScope {
            archived: match self.archived {
                Archived::Excluded => Some(false),
                Archived::Only => Some(true),
                Archived::Included => None,
            },
            project: self.project.clone(),
            label: match &self.label {
                LabelAxis::Any => None,
                LabelAxis::Unnamed => Some(None),
                LabelAxis::Named(name) => Some(Some(name.clone())),
            },
            assignee: match &self.assignee {
                AssigneeAxis::Any => None,
                AssigneeAxis::Unassigned => Some(None),
                AssigneeAxis::Named(name) => Some(Some(name.clone())),
            },
            with_description: self.normalized_text().is_some(),
        }
    }

    /// The tickets in view, in stored order. An empty result is a success.
    pub fn apply(&self, tickets: &[Ticket]) -> Vec<Ticket> {
        tickets
            .iter()
            .filter(|ticket| self.matches(ticket))
            .cloned()
            .collect()
    }

    /// One ticket against every axis: all of them must hold (the axes are ANDed).
    pub fn matches(&self, ticket: &Ticket) -> bool {
        match self.archived {
            Archived::Excluded => {
                if ticket.archived {
                    return false;
                }
            }
            Archived::Only => {
                if !ticket.archived {
                    return false;
                }
            }
            Archived::Included => {}
        }
        if !self.statuses.is_empty() && !self.statuses.contains(&ticket.status) {
            return false;
        }
        if let Some(done) = self.done
            && ticket.done() != done
        {
            return false;
        }
        if self.blocked_only && !ticket.blocked() {
            return false;
        }
        match &self.assignee {
            AssigneeAxis::Any => {}
            AssigneeAxis::Unassigned => {
                if ticket.assignee.is_some() {
                    return false;
                }
            }
            AssigneeAxis::Named(name) => {
                if ticket.assignee.as_deref() != Some(name.as_str()) {
                    return false;
                }
            }
        }
        if let Some(project) = &self.project
            && &ticket.project != project
        {
            return false;
        }
        match &self.label {
            LabelAxis::Any => {}
            LabelAxis::Unnamed => {
                if ticket.label.is_some() {
                    return false;
                }
            }
            LabelAxis::Named(name) => {
                if ticket.label.as_deref() != Some(name.as_str()) {
                    return false;
                }
            }
        }
        if let Some(needle) = self.normalized_text() {
            let in_title = TextMatch::contains(needle, &ticket.title);
            let in_description = ticket
                .description
                .as_deref()
                .is_some_and(|description| TextMatch::contains(needle, description));
            if !in_title && !in_description {
                return false;
            }
        }
        true
    }

    /// The search text with its edges trimmed. An empty filter box means no text
    /// filter, not "match nothing".
    pub fn normalized_text(&self) -> Option<&str> {
        let text = self.text.as_deref()?;
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    }

    /// The CLI boundary: raw flag values in, one query out, or a domain error
    /// naming what was wrong and what would have worked.
    ///
    /// **The CLI's filters never carry the archive.** `all` is every ticket *not*
    /// archived; the archive is reached by asking for it: `archived` puts it in
    /// view. Named with a filter, the archive joins that filter; named alone, the
    /// archive is the view (the app's Archived segment).
    #[allow(clippy::too_many_arguments)]
    pub fn parse(
        filter: Option<&str>,
        text: Option<&str>,
        status: Option<&str>,
        assignee: Option<&str>,
        project: Option<&str>,
        label: Option<&str>,
        archived: bool,
        blocked: bool,
    ) -> Result<Self, DomainError> {
        let named_filter = filter.is_some();
        let filter = Filter::parse(filter)?;
        let mut query = filter.query();
        // The CLI's filters never carry the archive; `Filter::All` still does, for
        // the app.
        query.archived = Archived::Excluded;
        if archived {
            if !named_filter {
                // The archive alone: statuses ignored.
                query.statuses = Vec::new();
                query.done = None;
                query.archived = Archived::Only;
            } else {
                // Named with a filter, the archive joins that filter.
                query.archived = Archived::Included;
            }
        }
        if let Some(text) = text {
            query.text = Some(text.to_string());
        }
        if let Some(status) = status {
            // An explicit status list overrides the shipped filter's finished
            // half, as `--filter done --status ready` has always meant.
            query.done = None;
            query.statuses = parse_statuses(status)?;
        }
        if let Some(assignee) = assignee {
            query.assignee = parse_assignee(assignee);
        }
        if let Some(project) = project {
            let project = project.trim();
            if !project.is_empty() {
                query.project = Some(project.to_string());
            }
        }
        if let Some(label) = label {
            query.label = parse_label(label);
        }
        query.blocked_only = blocked;
        Ok(query)
    }
}

/// `ready,in-progress`: comma-separated, because the flag grammar keeps only the
/// last value of a repeated flag and would silently drop the earlier ones.
fn parse_statuses(raw: &str) -> Result<Vec<Status>, DomainError> {
    let parts: Vec<&str> = raw
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() {
        return Err(DomainError::InvalidStatus(raw.to_string()));
    }
    let mut statuses: Vec<Status> = Vec::new();
    for part in parts {
        let status = Status::parse(part)?;
        if !statuses.contains(&status) {
            statuses.push(status);
        }
    }
    Ok(statuses)
}

/// The empty value is the empty field: `--assignee ""` asks for the unassigned.
fn parse_assignee(raw: &str) -> AssigneeAxis {
    let name = raw.trim();
    if name.is_empty() {
        AssigneeAxis::Unassigned
    } else {
        AssigneeAxis::Named(name.to_string())
    }
}

/// The empty value is "no label".
fn parse_label(raw: &str) -> LabelAxis {
    let name = raw.trim();
    if name.is_empty() {
        LabelAxis::Unnamed
    } else {
        LabelAxis::Named(name.to_string())
    }
}

/// Which tickets are in view: `open` (the default), `done`, or `all`
///
/// Sugar over [`TicketQuery`], not a second filter: the three shipped cases are
/// spelled once, in terms of the query's axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    Open,
    Done,
    All,
}

impl Filter {
    /// Every wire value `parse` accepts, so the tool schema's enum cannot drift.
    pub const WIRE_VALUES: [&'static str; 3] = ["open", "done", "all"];

    /// `None` (the CLI's `--filter` omitted) means the default filter, `open`.
    /// Any other value that is not one of the three cases is invalid.
    pub fn parse(raw: Option<&str>) -> Result<Self, DomainError> {
        match raw {
            None | Some("open") => Ok(Self::Open),
            Some("done") => Ok(Self::Done),
            Some("all") => Ok(Self::All),
            Some(bad) => Err(DomainError::InvalidFilter(bad.to_string())),
        }
    }

    /// The query this filter stands for: `open` is every unfinished ticket,
    /// `done` is every finished one, and `all` is everything, archived tickets
    /// included (the app's `All`). Finished is the ticket's own derived flag, so
    /// a custom finished stage is found without naming its key.
    pub fn query(&self) -> TicketQuery {
        match self {
            Self::Open => TicketQuery {
                done: Some(false),
                archived: Archived::Excluded,
                ..TicketQuery::default()
            },
            Self::Done => TicketQuery {
                done: Some(true),
                archived: Archived::Excluded,
                ..TicketQuery::default()
            },
            Self::All => TicketQuery {
                archived: Archived::Included,
                ..TicketQuery::default()
            },
        }
    }

    /// Apply this filter to a stored-order list. Ordering is never changed.
    pub fn apply(&self, tickets: &[Ticket]) -> Vec<Ticket> {
        self.query().apply(tickets)
    }
}
