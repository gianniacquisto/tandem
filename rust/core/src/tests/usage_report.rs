//! Ported from `UsageReportTests`: the derivation's arithmetic, its four row
//! sets, the coverage and the overlap, against frozen numbers.
//!
//! The numbers below are written down as literals on purpose. The committed
//! fixture home (`Fixtures/port/Home`) carries no `tandem.db` yet (its README
//! says the store arrives with PRT-4), so the frozen set is the shape that home
//! describes, built in memory: six relations over four records, and every total
//! asserted against the number a reader can compute by hand.

use crate::domain::session::{Session, SessionOutcome, SessionStatus, SessionTokens};
use crate::domain::usage::{Calendar, NO_TICKET_LABEL, UsageReport, UsageTicket, usage_report};

fn tokens(
    input: Option<i64>,
    output: Option<i64>,
    reasoning: Option<i64>,
) -> Option<SessionTokens> {
    let tokens = SessionTokens {
        input,
        output,
        reasoning,
        cache_read: None,
        cache_write: None,
    };
    tokens.reported().then_some(tokens)
}

#[allow(clippy::too_many_arguments)]
fn session(
    id: &str,
    started_at: &str,
    agent: Option<&str>,
    model: Option<&str>,
    tokens: Option<SessionTokens>,
    cost: Option<f64>,
    tickets: &[&str],
) -> Session {
    Session {
        id: id.to_string(),
        project: "tandem".to_string(),
        agent: agent.map(str::to_string),
        brief: "A record the report rolls up.".to_string(),
        summary: Some("What landed.".to_string()),
        status: SessionStatus::Ended,
        outcome: Some(SessionOutcome::Succeeded),
        tickets: tickets.iter().map(|id| id.to_string()).collect(),
        started_at: started_at.to_string(),
        ended_at: Some(started_at.to_string()),
        tokens,
        cost,
        model: model.map(str::to_string),
        skills: None,
    }
}

/// The frozen home: two days, two models, two agents, two tickets and one
/// ticketless record, four in all.
fn frozen() -> Vec<Session> {
    vec![
        session(
            "ses_a",
            "2026-10-30T09:00:00Z",
            Some("builder"),
            Some("deepseek-v4.1-flash"),
            Some(SessionTokens {
                input: Some(1000),
                output: Some(500),
                reasoning: Some(100),
                cache_read: Some(200),
                cache_write: Some(50),
            }),
            Some(0.25),
            &["t1"],
        ),
        session(
            "ses_b",
            "2026-10-30T11:00:00Z",
            Some("reviewer"),
            Some("deepseek-v4.1-flash"),
            tokens(Some(2000), Some(1000), Some(0)),
            Some(0.75),
            &["t1", "t2"],
        ),
        session(
            "ses_c",
            "2026-10-31T08:00:00Z",
            Some("builder"),
            Some("gemma4:e4b-mlx"),
            tokens(Some(3000), None, None),
            Some(1.5),
            &[],
        ),
        // The record that carries no usage: it counts as a record and adds to no
        // sum, and its missing model and agent get rows of their own.
        session(
            "ses_d",
            "2026-10-31T09:00:00Z",
            None,
            None,
            None,
            None,
            &["t2"],
        ),
    ]
}

fn tickets() -> Vec<UsageTicket> {
    vec![
        UsageTicket {
            id: "t1".to_string(),
            reference: Some("PRT-1".to_string()),
            title: "the board".to_string(),
        },
        UsageTicket {
            id: "t2".to_string(),
            reference: Some("PRT-2".to_string()),
            title: "the records".to_string(),
        },
    ]
}

fn report() -> UsageReport {
    usage_report(&frozen(), &tickets(), &Calendar::Utc)
}

#[test]
fn the_totals_are_frozen() {
    let report = report();

    assert_eq!(report.records, 4, "every record read, running or ended");
    assert_eq!(
        report.reported, 3,
        "the record without usage reports nothing"
    );
    assert_eq!(report.cost, 2.5, "0.25 + 0.75 + 1.5");
    assert_eq!(report.tokens.input, Some(6000), "1000 + 2000 + 3000");
    assert_eq!(
        report.tokens.output,
        Some(1500),
        "500 + 1000, and a zero keeps its sum"
    );
    assert_eq!(report.tokens.reasoning, Some(100), "100 + 0");
    assert_eq!(report.tokens.cache_read, Some(200));
    assert_eq!(report.tokens.cache_write, Some(50));
    assert_eq!(
        report.overlap, 0.75,
        "(2 - 1) x 0.75 for the two-ticket record"
    );
    assert_eq!(report.ticket_total(), 3.25, "2.5 + 0.75");
}

#[test]
fn the_day_rows_are_the_local_days_newest_first() {
    let report = report();

    let days: Vec<(&str, usize, usize, f64)> = report
        .day
        .iter()
        .map(|row| (row.id.as_str(), row.records, row.reported, row.cost))
        .collect();
    assert_eq!(
        days,
        vec![("2026-10-31", 2, 1, 1.5), ("2026-10-30", 2, 2, 1.0),],
        "a day is the day the record started, newest first"
    );
    assert_eq!(report.day[0].label, "2026-10-31");
    assert_eq!(
        report.day[1]
            .sessions
            .iter()
            .map(|session| session.id.as_str())
            .collect::<Vec<_>>(),
        vec!["ses_b", "ses_a"],
        "newest start first"
    );
}

#[test]
fn the_model_and_agent_rows_are_verbatim_partitions() {
    let report = report();

    let models: Vec<(&str, &str, usize, f64)> = report
        .model
        .iter()
        .map(|row| (row.id.as_str(), row.label.as_str(), row.records, row.cost))
        .collect();
    assert_eq!(
        models,
        vec![
            ("gemma4:e4b-mlx", "gemma4:e4b-mlx", 1, 1.5),
            ("deepseek-v4.1-flash", "deepseek-v4.1-flash", 2, 1.0),
            ("", "unreported", 1, 0.0),
        ],
        "cost descending, and the missing model gets its own row"
    );

    let agents: Vec<(&str, &str, usize, f64)> = report
        .agent
        .iter()
        .map(|row| (row.id.as_str(), row.label.as_str(), row.records, row.cost))
        .collect();
    assert_eq!(
        agents,
        vec![
            ("builder", "builder", 2, 1.75),
            ("reviewer", "reviewer", 1, 0.75),
            ("", "unattributed", 1, 0.0),
        ]
    );
}

#[test]
fn the_ticket_rows_are_whole_per_ticket_and_the_ticketless_row_is_last() {
    let report = report();

    let rows: Vec<(&str, &str, usize, usize, f64)> = report
        .ticket
        .iter()
        .map(|row| {
            (
                row.id.as_str(),
                row.label.as_str(),
                row.records,
                row.reported,
                row.cost,
            )
        })
        .collect();
    assert_eq!(
        rows,
        vec![
            ("t1", "PRT-1: the board", 2, 2, 1.0),
            ("t2", "PRT-2: the records", 2, 1, 0.75),
            ("no-ticket", NO_TICKET_LABEL, 1, 1, 1.5),
        ],
        "a multi-ticket record lands whole in each row, and the ticketless row is last whatever it costs"
    );
    assert_eq!(report.ticket[2].label, "no ticket");
}

#[test]
fn the_partitions_cover_every_record_once_and_the_tickets_state_their_excess() {
    let report = report();

    for rows in [&report.day, &report.model, &report.agent] {
        let records: usize = rows.iter().map(|row| row.records).sum();
        let reported: usize = rows.iter().map(|row| row.reported).sum();
        let cost: f64 = rows.iter().map(|row| row.cost).sum();
        assert_eq!(
            records, report.records,
            "a partition covers every record once"
        );
        assert_eq!(reported, report.reported);
        assert!((cost - report.cost).abs() < 1e-9, "the costs sum to cost");
    }

    let ticket_cost: f64 = report.ticket.iter().map(|row| row.cost).sum();
    assert!(
        (ticket_cost - report.ticket_total()).abs() < 1e-9,
        "the ticket rows sum to cost + overlap: {ticket_cost}"
    );
}

#[test]
fn two_spellings_are_two_rows() {
    let sessions = vec![
        session(
            "ses_a",
            "2026-10-30T09:00:00Z",
            Some("builder"),
            Some("deepseek-v4"),
            tokens(Some(1), None, None),
            Some(1.0),
            &[],
        ),
        session(
            "ses_b",
            "2026-10-30T10:00:00Z",
            Some("Builder"),
            Some("deepseek-v4.1"),
            tokens(Some(2), None, None),
            Some(1.0),
            &[],
        ),
    ];
    let report = usage_report(&sessions, &[], &Calendar::Utc);

    assert_eq!(report.model.len(), 2, "a spelling is the key, verbatim");
    assert_eq!(report.agent.len(), 2, "a case is a spelling too");
    assert_eq!(report.reported, 2);
    assert_eq!(report.cost, 2.0);
}

#[test]
fn a_record_that_reports_only_a_model_still_counts_as_reported() {
    let sessions = vec![session(
        "ses_a",
        "2026-10-30T09:00:00Z",
        Some("builder"),
        Some("deepseek-v4"),
        None,
        None,
        &[],
    )];
    let report = usage_report(&sessions, &[], &Calendar::Utc);

    assert_eq!(report.records, 1);
    assert_eq!(
        report.reported, 1,
        "the model is usage the Sessions face draws too"
    );
    assert_eq!(report.cost, 0.0);
    assert_eq!(
        report.tokens.input, None,
        "no piece was reported, so none is summed"
    );
}

#[test]
fn a_home_whose_records_all_predate_the_usage_fields_reports_no_usage() {
    // Every record was written before the harness reported anything: they all
    // count, none is reported, and no sum invents a zero.
    let sessions: Vec<Session> = (0..3)
        .map(|index| {
            session(
                &format!("ses_{index}"),
                &format!("2026-10-3{index}T09:00:00Z"),
                Some("builder"),
                None,
                None,
                None,
                &[],
            )
        })
        .collect();
    let report = usage_report(&sessions, &[], &Calendar::Utc);

    assert_eq!(report.records, 3);
    assert_eq!(report.reported, 0);
    assert_eq!(report.cost, 0.0);
    assert_eq!(
        report.overlap, 0.0,
        "no usage means no ticket excess either"
    );
    assert_eq!(report.tokens.input, None);
    assert_eq!(report.day.len(), 3, "the days still list the records");
    assert_eq!(
        report.model[0].label, "unreported",
        "the missing model is its own row, not a vanished one"
    );
    assert_eq!(report.model[0].reported, 0);
}

#[test]
fn an_empty_record_set_is_a_zeroed_report() {
    let report = usage_report(&[], &[], &Calendar::Utc);

    assert_eq!(report.records, 0);
    assert_eq!(report.reported, 0);
    assert_eq!(report.cost, 0.0);
    assert_eq!(report.overlap, 0.0);
    assert_eq!(report.tokens.input, None, "absent stays absent, never zero");
    assert!(report.day.is_empty());
    assert!(report.model.is_empty());
    assert!(report.agent.is_empty());
    assert!(report.ticket.is_empty());
}

#[test]
fn the_calendar_decides_what_a_day_is() {
    // The offset-free calendar reads the record's own date.
    assert_eq!(Calendar::Utc.day_key("2026-10-30T23:30:00Z"), "2026-10-30");
    // The local calendar answers the engine's own local date for that instant,
    // whatever zone this machine is in: the report never guesses a zone.
    assert_eq!(
        Calendar::Local.day_key("2026-10-30T23:30:00Z"),
        crate::store::local_day("2026-10-30T23:30:00Z").unwrap()
    );
}
