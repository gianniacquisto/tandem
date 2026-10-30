//! The reported usage, rolled up (TAN-208): one pure derivation over the
//! session records and the board's tickets.
//!
//! **No store read, no clock.** The calendar is passed in as [`Calendar`], the
//! way the Swift derivation took a `Calendar`, so a test builds its fixtures in
//! memory and the calendar decides what "a day" is. The face that draws the
//! report does no arithmetic of its own.
//!
//! **Absent is absent, never zero.** A record that reports no usage adds to
//! `records` and to no sum, and a record that reported `$0.00` is a reported
//! zero that counts and keeps its rows. A token piece is the sum of the records
//! that reported that piece, and stays `None` when none did.
//!
//! **Three partitions and one overlap.** The day, model and agent rows are three
//! views of one number: each covers every record once, and their costs sum to
//! `cost`. A ticket row is different: every ticket a record carried gets the
//! record's whole cost and whole tokens, exactly what the record's `usage`
//! comment on that ticket states, so the ticket rows sum to `cost + overlap`
//! rather than to `cost`. [`UsageReport::ticket_total`] is the sum and the face
//! says the overlap once.

use std::collections::BTreeMap;

use serde::Serialize;

use super::session::{Session, SessionTokens};

/// The rolling key of the ticketless row: every record that carried no ticket
/// lands here, so the ticket table accounts for the spend it cannot attribute.
pub const NO_TICKET_ID: &str = "no-ticket";

/// The ticketless row's label.
pub const NO_TICKET_LABEL: &str = "no ticket";

/// The calendar the derivation reads days through.
///
/// `Utc` is the offset-free calendar a test uses: the day is the record's own
/// date. [`Calendar::local`] is the reader's: the day is the local date of the
/// instant the record started, resolved through the engine's own `localtime`,
/// so a report spanning a daylight-saving change puts each record on the date
/// its reader lived through.
pub enum Calendar {
    /// Every record's day is its UTC date.
    Utc,
    /// Each record's day is its local date, resolved per instant.
    Local,
}

impl Calendar {
    /// The day key `started_at` belongs to, as zero-padded `YYYY-MM-DD`.
    ///
    /// A timestamp the store cannot decode answers its own first ten bytes; a
    /// report over rows a reader already parsed cannot reach that branch, and a
    /// panic on a corrupt row would take the whole Usage face down with it.
    pub fn day_key(&self, started_at: &str) -> String {
        match self {
            Self::Utc => started_at.get(0..10).unwrap_or(started_at).to_string(),
            Self::Local => match crate::store::local_day(started_at) {
                Ok(day) => day,
                Err(_) => started_at.get(0..10).unwrap_or(started_at).to_string(),
            },
        }
    }
}

/// The board row the report labels a ticket row with: its id, the lane key and
/// number that make its ref, and its title.
///
/// The port's stand-in for the Swift `Ticket` the derivation read: the records
/// domain owns the report and does not own the board, so it takes the two facts
/// the label needs rather than the whole ticket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageTicket {
    pub id: String,
    /// The lane's key and the ticket's number, when the lane issued one.
    pub reference: Option<String>,
    pub title: String,
}

impl UsageTicket {
    /// What the ticket row draws: `PRT-6: the title`, or the title alone before
    /// a lane issues the number.
    pub fn label(&self) -> String {
        match &self.reference {
            Some(reference) => format!("{reference}: {}", self.title),
            None => self.title.clone(),
        }
    }
}

/// The reported usage, rolled up.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageReport {
    /// Every record read, running or ended.
    pub records: usize,
    /// The records that carry any usage.
    pub reported: usize,
    /// The sum of the reported records' own cost.
    pub cost: f64,
    /// The summed token counts, each piece `None` when no record reported it.
    pub tokens: SessionTokens,
    /// By local calendar day, newest first.
    pub day: Vec<UsageRow>,
    /// By model string, verbatim, cost descending then label ascending.
    pub model: Vec<UsageRow>,
    /// By agent, verbatim, cost descending then label ascending.
    pub agent: Vec<UsageRow>,
    /// By carried ticket, cost descending then label ascending, `no ticket`
    /// last. Not a partition: a multi-ticket record lands whole in each row.
    pub ticket: Vec<UsageRow>,
    /// The ticket rows' excess over `cost`: `(n-1) x cost` for every record that
    /// carried `n >= 2` tickets.
    pub overlap: f64,
}

impl UsageReport {
    /// What the ticket table's rows add up to, `cost + overlap`: every ticket row
    /// and the ticketless row, whole.
    pub fn ticket_total(&self) -> f64 {
        self.cost + self.overlap
    }
}

/// One line of the report: a key, its label, the records behind it and what they
/// reported.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRow {
    /// The row's key, stable across a reload: the day, the model or agent string
    /// (empty for the unnamed row), the ticket's id, or `no-ticket`.
    pub id: String,
    /// What the face draws: the day, the exact string, the ticket's ref and
    /// title, or the unnamed row's word.
    pub label: String,
    /// Every record behind the row.
    pub records: usize,
    /// How many of those records carry usage.
    pub reported: usize,
    /// The reported records' summed cost.
    pub cost: f64,
    /// The reported records' summed token counts.
    pub tokens: SessionTokens,
    /// The records behind the row, newest `started_at` first.
    pub sessions: Vec<Session>,
}

/// Roll the records up (TAN-208).
///
/// An empty record set is a zeroed report, never an error.
pub fn usage_report(
    sessions: &[Session],
    tickets: &[UsageTicket],
    calendar: &Calendar,
) -> UsageReport {
    let total = summarize(sessions);

    let mut day_buckets: BTreeMap<String, Vec<Session>> = BTreeMap::new();
    let mut model_buckets: BTreeMap<String, Vec<Session>> = BTreeMap::new();
    let mut agent_buckets: BTreeMap<String, Vec<Session>> = BTreeMap::new();
    let mut ticket_buckets: BTreeMap<String, Vec<Session>> = BTreeMap::new();
    let mut ticketless: Vec<Session> = Vec::new();

    for session in sessions {
        // A day is the calendar day the record started, whatever day it ended.
        day_buckets
            .entry(calendar.day_key(&session.started_at))
            .or_default()
            .push(session.clone());
        // Grouped by what the record says, verbatim: two spellings are two rows,
        // and the missing value gets its own row rather than vanishing.
        model_buckets
            .entry(session.model.clone().unwrap_or_default())
            .or_default()
            .push(session.clone());
        agent_buckets
            .entry(session.agent.clone().unwrap_or_default())
            .or_default()
            .push(session.clone());
        if session.tickets.is_empty() {
            ticketless.push(session.clone());
        } else {
            for ticket in &session.tickets {
                ticket_buckets
                    .entry(ticket.clone())
                    .or_default()
                    .push(session.clone());
            }
        }
    }

    let mut model_rows: Vec<UsageRow> = model_buckets
        .into_iter()
        .map(|(key, records)| {
            let label = if key.is_empty() {
                "unreported".to_string()
            } else {
                key.clone()
            };
            row(key, label, records)
        })
        .collect();
    cost_then_label(&mut model_rows);

    let mut agent_rows: Vec<UsageRow> = agent_buckets
        .into_iter()
        .map(|(key, records)| {
            let label = if key.is_empty() {
                "unattributed".to_string()
            } else {
                key.clone()
            };
            row(key, label, records)
        })
        .collect();
    cost_then_label(&mut agent_rows);

    let mut ticket_rows: Vec<UsageRow> = ticket_buckets
        .into_iter()
        .map(|(id, records)| {
            let label = match tickets.iter().find(|ticket| ticket.id == id) {
                Some(ticket) => ticket.label(),
                None => id.clone(),
            };
            row(id, label, records)
        })
        .collect();
    cost_then_label(&mut ticket_rows);

    // The ticketless row is last, whatever it costs, so the table's bottom line
    // is the spend the tickets cannot account for.
    if !ticketless.is_empty() {
        ticket_rows.push(row(
            NO_TICKET_ID.to_string(),
            NO_TICKET_LABEL.to_string(),
            ticketless,
        ));
    }

    // Day rows read newest first; the key is a zero-padded local date.
    let day_rows: Vec<UsageRow> = day_buckets
        .into_iter()
        .rev()
        .map(|(key, records)| row(key.clone(), key, records))
        .collect();

    let mut overlap = 0.0;
    for session in sessions {
        if session.tickets.len() >= 2 && session.carries_usage() {
            overlap += (session.tickets.len() - 1) as f64 * session.cost.unwrap_or(0.0);
        }
    }

    UsageReport {
        records: total.records,
        reported: total.reported,
        cost: total.cost,
        tokens: total.tokens(),
        day: day_rows,
        model: model_rows,
        agent: agent_rows,
        ticket: ticket_rows,
        overlap,
    }
}

/// The summed record count, reported count, cost and tokens.
#[derive(Default)]
struct Summary {
    records: usize,
    reported: usize,
    cost: f64,
    input: Option<i64>,
    output: Option<i64>,
    reasoning: Option<i64>,
    cache_read: Option<i64>,
    cache_write: Option<i64>,
}

impl Summary {
    fn tokens(&self) -> SessionTokens {
        SessionTokens {
            input: self.input,
            output: self.output,
            reasoning: self.reasoning,
            cache_read: self.cache_read,
            cache_write: self.cache_write,
        }
    }
}

fn summarize(sessions: &[Session]) -> Summary {
    let mut summary = Summary::default();
    for session in sessions {
        summary.records += 1;
        if !session.carries_usage() {
            continue;
        }
        summary.reported += 1;
        summary.cost += session.cost.unwrap_or(0.0);
        if let Some(tokens) = &session.tokens {
            // A piece sums the records that reported it, and a reported zero adds
            // zero; a piece no record reported stays `None`.
            if let Some(value) = tokens.input {
                summary.input = Some(summary.input.unwrap_or(0) + value);
            }
            if let Some(value) = tokens.output {
                summary.output = Some(summary.output.unwrap_or(0) + value);
            }
            if let Some(value) = tokens.reasoning {
                summary.reasoning = Some(summary.reasoning.unwrap_or(0) + value);
            }
            if let Some(value) = tokens.cache_read {
                summary.cache_read = Some(summary.cache_read.unwrap_or(0) + value);
            }
            if let Some(value) = tokens.cache_write {
                summary.cache_write = Some(summary.cache_write.unwrap_or(0) + value);
            }
        }
    }
    summary
}

fn row(id: String, label: String, mut sessions: Vec<Session>) -> UsageRow {
    let summary = summarize(&sessions);
    newest_first(&mut sessions);
    UsageRow {
        id,
        label,
        records: summary.records,
        reported: summary.reported,
        cost: summary.cost,
        tokens: summary.tokens(),
        sessions,
    }
}

/// Cost descending, ties by label ascending, so two reads agree.
fn cost_then_label(rows: &mut [UsageRow]) {
    rows.sort_by(|a, b| {
        b.cost
            .total_cmp(&a.cost)
            .then_with(|| a.label.cmp(&b.label))
    });
}

/// Newest `started_at` first; records that share a timestamp keep the order they
/// were read in.
fn newest_first(sessions: &mut [Session]) {
    sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at));
}
