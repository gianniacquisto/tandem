//! One view over the tickets (`Query.swift` and `Filter.swift`, ADR 0017).
//!
//! The single definition of "which tickets are in view", shared by the CLI's flags
//! and the app's filter row, so "the ready tickets" cannot mean one thing to a
//! human and another to an agent. It is a pure predicate: it never reorders (the
//! stored order is the order) and it never writes.

use super::description::DescriptionBody;
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

/// The lane axis: any lane, the unnamed default area, or one named lane.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LaneAxis {
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
    pub assignee: AssigneeAxis,
    pub lane: LaneAxis,
    pub archived: Archived,
    /// Whether tickets carrying an answer block are left out of view. Only the
    /// inbox sets it.
    pub exclude_answered: bool,
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
            assignee: AssigneeAxis::Any,
            lane: LaneAxis::Any,
            archived: Archived::Excluded,
            exclude_answered: false,
            blocked_only: false,
        }
    }
}

impl TicketQuery {
    /// The human's inbox: every open, non-archived ticket assigned to the person,
    /// minus the ones that already carry an answer.
    ///
    /// **Answered tickets are not waiting.** The queue asks questions; a ticket
    /// whose `## Answer` block is written has been answered whether or not the
    /// answering actor remembered to move it. The agents' read keeps showing it, so
    /// the bookkeeping debt stays visible where the actors who must settle it look.
    pub fn inbox() -> Self {
        Self {
            statuses: Status::WORKFLOW
                .into_iter()
                .filter(|status| !status.is_done())
                .collect(),
            assignee: AssigneeAxis::Named(HUMAN.to_string()),
            archived: Archived::Excluded,
            exclude_answered: true,
            ..Self::default()
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
        if self.blocked_only && !ticket.blocked() {
            return false;
        }
        if self.exclude_answered && DescriptionBody::answered(ticket.description.as_deref()) {
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
        match &self.lane {
            LaneAxis::Any => {}
            LaneAxis::Unnamed => {
                if ticket.lane.is_some() {
                    return false;
                }
            }
            LaneAxis::Named(name) => {
                if ticket.lane.as_deref() != Some(name.as_str()) {
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
    pub fn parse(
        filter: Option<&str>,
        text: Option<&str>,
        status: Option<&str>,
        assignee: Option<&str>,
        lane: Option<&str>,
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
            query.statuses = parse_statuses(status)?;
        }
        if let Some(assignee) = assignee {
            query.assignee = parse_assignee(assignee);
        }
        if let Some(lane) = lane {
            query.lane = parse_lane(lane);
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

/// The empty value is the unnamed default area.
fn parse_lane(raw: &str) -> LaneAxis {
    let name = raw.trim();
    if name.is_empty() {
        LaneAxis::Unnamed
    } else {
        LaneAxis::Named(name.to_string())
    }
}

/// Which tickets are in view: `open` (the default), `done`, or `all`
/// (`Filter.swift`).
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

    /// The query this filter stands for: `open` is every status but done, `done` is
    /// done, and `all` is everything, archived tickets included (the app's `All`).
    pub fn query(&self) -> TicketQuery {
        match self {
            Self::Open => TicketQuery {
                statuses: Status::WORKFLOW
                    .into_iter()
                    .filter(|status| !status.is_done())
                    .collect(),
                archived: Archived::Excluded,
                ..TicketQuery::default()
            },
            Self::Done => TicketQuery {
                statuses: vec![Status::Done],
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
