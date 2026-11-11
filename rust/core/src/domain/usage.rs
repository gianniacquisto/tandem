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
//! zero that counts and keeps its rows. A token piece is the sum of the
//! records' attributed shares, and stays `None` when none did.
//!
//! **A harness session is counted once.** A record stamped with
//! `--usage-from-harness` carries the harness session's cumulative total, and a
//! harness session can carry several records (a re-dispatched pass opens its
//! own, ADR 0027). [`attribute_harness_usage`] turns those snapshots into the
//! difference each record added, so the group sums to the harness session's own
//! totals while every record keeps a row. The dispatch attributes before the
//! window narrows, so a record whose earlier sibling fell outside the selected
//! window still reports its own share.
//!
//! **Three partitions and one overlap.** The day, model and agent rows are three
//! views of one number: each covers every record once, and their costs sum to
//! `cost`. A ticket row is different: every ticket a record carried gets the
//! record's whole cost and whole tokens, so the ticket rows sum to
//! `cost + overlap` rather than to `cost`. [`UsageReport::ticket_total`] is the sum and the face
//! says the overlap once.
//!
//! **The bar is the report's too.** [`UsageRow::bar_percent`] is a row's cost
//! over its table's largest, the width the Swift face derived in its view
//! (`UsageFace.bar`). The ported face formats it and derives nothing (PRT-15).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

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

/// The Usage page's rolling window (COR-6).
///
/// A pure value over literal `YYYY-MM-DD` day keys: no store read, no clock.
/// The caller computes today in the reader's local calendar and passes it in,
/// so the derivation stays clock-free and the tests name their own today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, ts_rs::TS)]
pub enum UsageWindow {
    /// Today plus the six prior local days, boundaries inclusive.
    #[serde(rename = "last-7-days")]
    Last7Days,
    /// Today plus the twenty-nine prior local days, boundaries inclusive.
    #[serde(rename = "last-30-days")]
    Last30Days,
    /// The same `YYYY-MM` local day prefix as today, through today inclusive.
    #[serde(rename = "this-month")]
    ThisMonth,
    /// No filter: every record read, today's behavior for old callers.
    #[default]
    #[serde(rename = "all-time")]
    AllTime,
}

impl UsageWindow {
    /// The wire's spelling of this window.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Last7Days => "last-7-days",
            Self::Last30Days => "last-30-days",
            Self::ThisMonth => "this-month",
            Self::AllTime => "all-time",
        }
    }

    /// Every wire value, in picker order.
    pub const WIRE_VALUES: [&'static str; 4] =
        ["last-7-days", "last-30-days", "this-month", "all-time"];

    /// The window `word` names, or `None` when it names none.
    pub fn parse(word: &str) -> Option<Self> {
        match word {
            "last-7-days" => Some(Self::Last7Days),
            "last-30-days" => Some(Self::Last30Days),
            "this-month" => Some(Self::ThisMonth),
            "all-time" => Some(Self::AllTime),
            _ => None,
        }
    }

    /// Whether the local day `day` belongs to this window when today is the
    /// local day `today`. Boundaries are inclusive: Last N days is today plus
    /// the N-1 prior local days, This month is the month prefix through today,
    /// All time keeps everything. A day or today that is not a valid
    /// `YYYY-MM-DD` key belongs to no windowed set, never to a silent match.
    pub fn contains(&self, day: &str, today: &str) -> bool {
        match self {
            Self::AllTime => true,
            Self::ThisMonth => {
                let Some(prefix) = today.get(0..7) else {
                    return false;
                };
                let Some(day_prefix) = day.get(0..7) else {
                    return false;
                };
                if day_prefix != prefix {
                    return false;
                }
                valid_day(day) && valid_day(today) && day <= today
            }
            Self::Last7Days => last_n_contains(day, today, 7),
            Self::Last30Days => last_n_contains(day, today, 30),
        }
    }
}

/// The sessions of `sessions` whose calendar day belongs to `window` when today
/// is the local day `today`, in the order they were read. The derivation runs
/// unchanged over the narrowed set, so one narrowed report feeds the header,
/// the tables and the graph.
pub fn filter_sessions_by_window(
    sessions: &[Session],
    window: &UsageWindow,
    today: &str,
    calendar: &Calendar,
) -> Vec<Session> {
    if *window == UsageWindow::AllTime {
        return sessions.to_vec();
    }
    sessions
        .iter()
        .filter(|session| window.contains(&calendar.day_key(&session.started_at), today))
        .cloned()
        .collect()
}

/// Attribute each harness session's cumulative snapshot to the records that
/// share it (TAN-352).
///
/// A harness session carries one total, and every Tandem record that ran in it
/// stamps that same running total at `session end` (ADR 0027: a re-dispatched
/// pass opens its own record under the same pointer). This derivation turns the
/// snapshots into shares, so the report counts the session once and each record
/// keeps a row of its own:
///
/// - Group by `Session::harness_session_id`. A record with no pointer passes
///   through untouched: its own stamp is its own usage.
/// - Order each group by `(ended_at, else started_at, started_at, id)`
///   ascending, never the caller's order (`Sessions::list` answers newest
///   first).
/// - A record that does not `Session::carries_usage` passes through and becomes
///   no baseline for the next one.
/// - Per piece (`input`, `output`, `reasoning`, `cache_read`, `cache_write`) and
///   for `cost`: the attributed value is the record's stamp minus the last value
///   that piece took in the group, saturating at zero (whole tokens, and `0.0`
///   for a cost that moves backwards). A piece the group has not reported yet is
///   kept whole by the record that reports it first.
/// - Everything else on the record (`id`, times, project, agent, tickets,
///   status, outcome, brief, summary, `model`, `skills`) is copied unchanged, so
///   the rows that group by model, agent, day and ticket read the same records
///   with attributed numbers.
///
/// By construction a group's attributed pieces sum to the last value each piece
/// reported in that group: the harness session's own totals. The residual is
/// spend inside the harness session before the earliest record this home holds
/// for that pointer, which lands on that first record; the group total is still
/// the session's total, only the split is coarse.
pub fn attribute_harness_usage(sessions: &[Session]) -> Vec<Session> {
    let mut groups: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, session) in sessions.iter().enumerate() {
        if let Some(harness) = session.harness_session_id.as_deref() {
            groups.entry(harness).or_default().push(index);
        }
    }

    let mut attributed: Vec<Option<Session>> = vec![None; sessions.len()];
    for indices in groups.values() {
        let mut ordered = indices.clone();
        ordered.sort_by(|&left, &right| {
            let key = |session: &Session| {
                (
                    session
                        .ended_at
                        .clone()
                        .unwrap_or_else(|| session.started_at.clone()),
                    session.started_at.clone(),
                    session.id.clone(),
                )
            };
            key(&sessions[left]).cmp(&key(&sessions[right]))
        });

        let mut last_input: Option<i64> = None;
        let mut last_output: Option<i64> = None;
        let mut last_reasoning: Option<i64> = None;
        let mut last_cache_read: Option<i64> = None;
        let mut last_cache_write: Option<i64> = None;
        let mut last_cost: Option<f64> = None;

        for index in ordered {
            let session = &sessions[index];
            if !session.carries_usage() {
                attributed[index] = Some(session.clone());
                continue;
            }
            let mut share = session.clone();
            if let Some(tokens) = &session.tokens {
                share.tokens = Some(SessionTokens {
                    input: attributed_piece(tokens.input, &mut last_input),
                    output: attributed_piece(tokens.output, &mut last_output),
                    reasoning: attributed_piece(tokens.reasoning, &mut last_reasoning),
                    cache_read: attributed_piece(tokens.cache_read, &mut last_cache_read),
                    cache_write: attributed_piece(tokens.cache_write, &mut last_cache_write),
                });
            }
            if let Some(cost) = session.cost {
                let baseline = last_cost.unwrap_or(0.0);
                share.cost = Some((cost - baseline).max(0.0));
                last_cost = Some(cost);
            }
            attributed[index] = Some(share);
        }
    }

    sessions
        .iter()
        .enumerate()
        .map(|(index, session)| attributed[index].take().unwrap_or_else(|| session.clone()))
        .collect()
}

/// One piece's share: the stamp minus the last value the group reported for it,
/// saturating at zero, and the stamp becomes the next baseline. A piece the
/// group has not reported yet is kept whole.
fn attributed_piece(value: Option<i64>, last: &mut Option<i64>) -> Option<i64> {
    let value = value?;
    let share = match *last {
        Some(previous) => (value - previous).max(0),
        None => value,
    };
    *last = Some(value);
    Some(share)
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
    /// The bar's width, as a percentage of the largest cost in the row's own
    /// table (`0` to `100`, and `0` when no row of that table reported a cost).
    /// It lives in the report, not the face, so the face formats it and computes
    /// no percentage of its own (PRT-15).
    pub bar_percent: f64,
    /// The reported records' summed token counts.
    pub tokens: SessionTokens,
    /// The per-reported-record averages: each token piece over `reported`,
    /// rounded to whole tokens (.5 up); `None` for a piece no record reported,
    /// and every piece `None` when the row reported nothing.
    pub average_tokens: SessionTokens,
    /// The reported cost over `reported`, at full precision for the face's
    /// `money`; `0.0` when the row reported nothing.
    pub average_cost: f64,
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
    with_bars(&mut model_rows);

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
    with_bars(&mut agent_rows);

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
    with_bars(&mut ticket_rows);

    // Day rows read newest first; the key is a zero-padded local date.
    let mut day_rows: Vec<UsageRow> = day_buckets
        .into_iter()
        .rev()
        .map(|(key, records)| row(key.clone(), key, records))
        .collect();
    with_bars(&mut day_rows);

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

    /// The per-reported-record averages: every piece and the cost over
    /// `reported`, the tokens rounded to whole tokens (.5 up) and the cost at
    /// full precision. A row that reported nothing carries absent pieces and a
    /// zero cost, so the face's reported==0 guard draws the absent mark.
    fn averages(&self) -> (SessionTokens, f64) {
        if self.reported == 0 {
            return (SessionTokens::default(), 0.0);
        }
        let denominator = self.reported as f64;
        let piece =
            |total: Option<i64>| total.map(|total| (total as f64 / denominator).round() as i64);
        (
            SessionTokens {
                input: piece(self.input),
                output: piece(self.output),
                reasoning: piece(self.reasoning),
                cache_read: piece(self.cache_read),
                cache_write: piece(self.cache_write),
            },
            self.cost / denominator,
        )
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
    let (average_tokens, average_cost) = summary.averages();
    UsageRow {
        id,
        label,
        records: summary.records,
        reported: summary.reported,
        cost: summary.cost,
        bar_percent: 0.0,
        tokens: summary.tokens(),
        average_tokens,
        average_cost,
        sessions,
    }
}

/// Scale each row's bar to the largest cost in the table being drawn: the one
/// quantity the Swift face derived itself (`UsageFace.bar`), moved into the
/// report so the ported face does no arithmetic (PRT-15). A table whose rows all
/// report nothing draws empty bars rather than a division by zero.
fn with_bars(rows: &mut [UsageRow]) {
    let largest = rows.iter().map(|row| row.cost).fold(0.0, f64::max);
    if largest <= 0.0 {
        return;
    }
    for row in rows {
        row.bar_percent = row.cost / largest * 100.0;
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

/// Whether `day` and `today` name a Last N window with `day` inside it: today
/// plus the N-1 prior days, both ends inclusive. The comparison is
/// lexicographic on zero-padded keys, so the cutoff arithmetic below is the
/// only date math.
fn last_n_contains(day: &str, today: &str, days: i64) -> bool {
    if !valid_day(day) || !valid_day(today) {
        return false;
    }
    if day > today {
        return false;
    }
    let Some(cutoff) = shift_days(today, -(days - 1)) else {
        return false;
    };
    day >= cutoff.as_str()
}

/// Whether `text` is a real `YYYY-MM-DD` calendar day, not just ten
/// day-shaped characters: February 30th is shaped like a day and is not one.
fn valid_day(text: &str) -> bool {
    let Some((year, month, day)) = split_day(text) else {
        return false;
    };
    days_from_civil(year, month, day)
        .and_then(civil_from_days)
        .is_some_and(|(back_year, back_month, back_day)| {
            back_year == year && back_month == month && back_day == day
        })
}

/// `today` moved by `delta` days, as a `YYYY-MM-DD` key, or `None` when today
/// is not a valid day. Howard Hinnant's civil arithmetic, the store's own date
/// math beside `timestamp`: a month or year edge is an ordinary subtraction.
fn shift_days(today: &str, delta: i64) -> Option<String> {
    let (year, month, day) = split_day(today)?;
    let count = days_from_civil(year, month, day)? + delta;
    let (year, month, day) = civil_from_days(count)?;
    Some(format!("{year:04}-{month:02}-{day:02}"))
}

/// The `YYYY-MM-DD` key split into its three numbers, or `None` when the shape
/// is wrong.
fn split_day(text: &str) -> Option<(i64, u32, u32)> {
    let bytes = text.as_bytes();
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return None;
    }
    let year: i64 = text[0..4].parse().ok()?;
    let month: u32 = text[5..7].parse().ok()?;
    let day: u32 = text[8..10].parse().ok()?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some((year, month, day))
}

/// The day count `year-month-day` names since the epoch, or `None` for a month
/// or day the calendar never holds. The inverse of [`civil_from_days`], the
/// same arithmetic the store's `timestamp` keeps.
fn days_from_civil(year: i64, month: u32, day: u32) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month_prime = if month > 2 { month - 3 } else { month + 9 } as i64;
    let day_of_year = (153 * month_prime + 2) / 5 + day as i64 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some(era * 146_097 + day_of_era - 719_468)
}

/// The civil date `days` since the epoch names, or `None` when out of range.
fn civil_from_days(days: i64) -> Option<(i64, u32, u32)> {
    let shifted = days.checked_add(719_468)?;
    let era = shifted.div_euclid(146_097);
    let of_era = shifted - era * 146_097;
    let year_of_era = (of_era - of_era / 1_460 + of_era / 36_524 - of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * month_prime + 2) / 5 + 1) as u32;
    let month = if month_prime < 10 {
        month_prime + 3
    } else {
        month_prime - 9
    } as u32;
    let year = if month <= 2 { year + 1 } else { year };
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    Some((year, month, day))
}
