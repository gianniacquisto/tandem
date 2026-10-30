//! The query (ADR 0017), ported from `QueryTests.swift`: one predicate over the
//! ticket axes, shared by the CLI's flags and the app's filter row. Proved as a
//! pure function of a ticket list, never through the binary or the view.

mod support;

use tandem_core::domain::{
    Archived, AssigneeAxis, DomainError, Filter, HUMAN, LaneAxis, Status, Ticket, TicketBlock,
    TicketQuery,
};

fn ticket(title: &str) -> Ticket {
    Ticket::new(title)
}

/// A board with one ticket per interesting corner, in stored order.
fn board() -> Vec<Ticket> {
    vec![
        ticket("capture the idea"),
        Ticket {
            status: Status::OnHold,
            assignee: Some("core".to_string()),
            lane: Some("Core".to_string()),
            ..ticket("parked work")
        },
        Ticket {
            status: Status::Ready,
            assignee: Some("ui".to_string()),
            lane: Some("Core".to_string()),
            ..ticket("ready work")
        },
        Ticket {
            status: Status::InProgress,
            description: Some("Parse the flags in the core".to_string()),
            assignee: Some("ui".to_string()),
            lane: Some("CLI".to_string()),
            ..ticket("in flight")
        },
        Ticket {
            status: Status::Done,
            lane: Some("CLI".to_string()),
            ..ticket("finished")
        },
        Ticket {
            status: Status::Done,
            archived: true,
            lane: Some("CLI".to_string()),
            ..ticket("finished and filed away")
        },
        Ticket {
            archived: true,
            ..ticket("archived but not done")
        },
    ]
}

fn titles(query: &TicketQuery) -> Vec<String> {
    query
        .apply(&board())
        .into_iter()
        .map(|ticket| ticket.title)
        .collect()
}

#[test]
fn the_default_query_hides_the_archive_and_nothing_else() {
    assert_eq!(
        titles(&TicketQuery::default()),
        [
            "capture the idea",
            "parked work",
            "ready work",
            "in flight",
            "finished",
        ]
    );
}

#[test]
fn the_status_axis_selects_exactly_the_named_statuses_and_empty_means_every_status() {
    assert_eq!(
        titles(&TicketQuery {
            statuses: vec![Status::Ready],
            ..TicketQuery::default()
        }),
        ["ready work"]
    );
    assert_eq!(
        titles(&TicketQuery {
            statuses: vec![Status::Ready, Status::InProgress],
            ..TicketQuery::default()
        }),
        ["ready work", "in flight"]
    );
    assert_eq!(
        titles(&TicketQuery::default()),
        titles(&TicketQuery::default())
    );
}

#[test]
fn the_assignee_axis_selects_a_name_or_the_unowned_tickets() {
    assert_eq!(
        titles(&TicketQuery {
            assignee: AssigneeAxis::Named("ui".to_string()),
            ..TicketQuery::default()
        }),
        ["ready work", "in flight"]
    );
    assert_eq!(
        titles(&TicketQuery {
            assignee: AssigneeAxis::Unassigned,
            ..TicketQuery::default()
        }),
        ["capture the idea", "finished"]
    );
    assert_eq!(
        titles(&TicketQuery {
            assignee: AssigneeAxis::Any,
            ..TicketQuery::default()
        }),
        titles(&TicketQuery::default())
    );
    assert!(
        titles(&TicketQuery {
            assignee: AssigneeAxis::Named("nobody".to_string()),
            ..TicketQuery::default()
        })
        .is_empty()
    );
}

#[test]
fn the_lane_axis_selects_a_name_or_the_unnamed_default_area() {
    assert_eq!(
        titles(&TicketQuery {
            lane: LaneAxis::Named("Core".to_string()),
            ..TicketQuery::default()
        }),
        ["parked work", "ready work"]
    );
    assert_eq!(
        titles(&TicketQuery {
            lane: LaneAxis::Unnamed,
            ..TicketQuery::default()
        }),
        ["capture the idea"]
    );
    assert_eq!(
        titles(&TicketQuery {
            lane: LaneAxis::Any,
            ..TicketQuery::default()
        }),
        titles(&TicketQuery::default())
    );
}

#[test]
fn the_text_axis_matches_the_title_and_the_description() {
    assert_eq!(
        titles(&TicketQuery {
            text: Some("FLIGHT".to_string()),
            ..TicketQuery::default()
        }),
        ["in flight"]
    );
    assert_eq!(
        titles(&TicketQuery {
            text: Some("flags".to_string()),
            ..TicketQuery::default()
        }),
        ["in flight"],
        "matched inside a description"
    );
    assert_eq!(
        titles(&TicketQuery {
            text: Some("  ".to_string()),
            ..TicketQuery::default()
        }),
        titles(&TicketQuery::default()),
        "an empty box filters nothing"
    );
    assert!(
        titles(&TicketQuery {
            text: Some("nothing here".to_string()),
            ..TicketQuery::default()
        })
        .is_empty(),
        "no match is an empty success"
    );
}

#[test]
fn the_text_axis_ignores_case_and_accents() {
    let list = vec![ticket("Über die Brücke"), ticket("plain")];
    let find = |text: &str| {
        TicketQuery {
            text: Some(text.to_string()),
            ..TicketQuery::default()
        }
        .apply(&list)
        .into_iter()
        .map(|ticket| ticket.title)
        .collect::<Vec<_>>()
    };
    assert_eq!(find("uber"), ["Über die Brücke"]);
    assert_eq!(find("brücke"), ["Über die Brücke"]);
}

#[test]
fn the_archive_axis_is_three_valued() {
    assert_eq!(
        titles(&TicketQuery {
            archived: Archived::Included,
            ..TicketQuery::default()
        })
        .len(),
        7
    );
    assert_eq!(
        titles(&TicketQuery {
            archived: Archived::Only,
            ..TicketQuery::default()
        }),
        ["finished and filed away", "archived but not done"]
    );
}

#[test]
fn the_axes_are_anded_and_an_empty_result_is_an_empty_list() {
    let query = TicketQuery {
        text: Some("work".to_string()),
        statuses: vec![Status::Ready],
        assignee: AssigneeAxis::Named("ui".to_string()),
        lane: LaneAxis::Named("Core".to_string()),
        ..TicketQuery::default()
    };
    assert_eq!(titles(&query), ["ready work"]);

    assert!(
        titles(&TicketQuery {
            statuses: vec![Status::Ready],
            lane: LaneAxis::Named("CLI".to_string()),
            ..TicketQuery::default()
        })
        .is_empty()
    );
}

#[test]
fn the_shipped_filter_keeps_meaning_what_it_means_open_done_all() {
    assert_eq!(
        Filter::Open
            .apply(&board())
            .into_iter()
            .map(|ticket| ticket.title)
            .collect::<Vec<_>>(),
        ["capture the idea", "parked work", "ready work", "in flight"]
    );
    assert_eq!(
        Filter::Done
            .apply(&board())
            .into_iter()
            .map(|ticket| ticket.title)
            .collect::<Vec<_>>(),
        ["finished"]
    );
    assert_eq!(
        Filter::All.apply(&board()).len(),
        7,
        "all is everything, the archive included"
    );
}

#[test]
fn parse_defaults_to_the_shipped_open_filter() {
    let query = TicketQuery::parse(None, None, None, None, None, false, false).unwrap();
    assert_eq!(query.archived, Archived::Excluded);
    assert_eq!(
        query.statuses,
        Status::WORKFLOW
            .into_iter()
            .filter(|status| !status.is_done())
            .collect::<Vec<_>>(),
        "open is every value the domain accepts but done"
    );
    assert_eq!(query.assignee, AssigneeAxis::Any);
    assert_eq!(query.lane, LaneAxis::Any);
    assert_eq!(query.text, None);
}

#[test]
fn parse_takes_comma_separated_statuses() {
    let statuses = |raw: &str| {
        TicketQuery::parse(None, None, Some(raw), None, None, false, false)
            .unwrap()
            .statuses
    };
    assert_eq!(
        statuses("ready,in-progress"),
        [Status::Ready, Status::InProgress]
    );
    assert_eq!(
        statuses(" ready , on-hold "),
        [Status::Ready, Status::OnHold]
    );
    assert_eq!(statuses("ready"), [Status::Ready]);
}

#[test]
fn parse_rejects_an_unknown_status_by_name_listing_the_live_values() {
    let error = TicketQuery::parse(None, None, Some("ready,blocked"), None, None, false, false)
        .unwrap_err();
    match error {
        DomainError::InvalidStatus(bad) => {
            assert_eq!(bad, "blocked");
            for value in Status::WIRE_VALUES {
                assert!(
                    DomainError::InvalidStatus(bad.clone())
                        .message()
                        .contains(value)
                );
            }
        }
        other => panic!("expected invalidStatus, got {other:?}"),
    }
}

#[test]
fn the_retired_word_is_not_a_status_the_query_accepts() {
    let error =
        TicketQuery::parse(None, None, Some("refined"), None, None, false, false).unwrap_err();
    match error {
        DomainError::InvalidStatus(bad) => {
            assert_eq!(bad, "refined");
            assert!(
                DomainError::InvalidStatus(bad)
                    .message()
                    .contains("on-hold")
            );
        }
        other => panic!("expected invalidStatus, got {other:?}"),
    }
}

#[test]
fn parse_treats_an_empty_value_as_the_empty_field() {
    let parse = |assignee: Option<&str>, lane: Option<&str>| {
        TicketQuery::parse(None, None, None, assignee, lane, false, false).unwrap()
    };
    assert_eq!(parse(Some(""), None).assignee, AssigneeAxis::Unassigned);
    assert_eq!(
        parse(Some("ui"), None).assignee,
        AssigneeAxis::Named("ui".to_string())
    );
    assert_eq!(parse(None, Some("")).lane, LaneAxis::Unnamed);
    assert_eq!(
        parse(None, Some("Core")).lane,
        LaneAxis::Named("Core".to_string())
    );
    assert_eq!(
        TicketQuery::parse(None, Some("  "), None, None, None, false, false)
            .unwrap()
            .normalized_text(),
        None
    );
}

#[test]
fn parse_keeps_the_shipped_filter_as_sugar_and_the_clis_filters_never_carry_the_archive() {
    assert_eq!(
        TicketQuery::parse(Some("all"), None, None, None, None, false, false)
            .unwrap()
            .archived,
        Archived::Excluded
    );
    assert_eq!(
        TicketQuery::parse(Some("done"), None, None, None, None, false, false)
            .unwrap()
            .statuses,
        [Status::Done]
    );
    assert_eq!(
        TicketQuery::parse(Some("open"), None, None, None, None, false, false)
            .unwrap()
            .statuses,
        [
            Status::Backlog,
            Status::OnHold,
            Status::Ready,
            Status::InProgress,
            Status::InReview
        ]
    );
}

#[test]
fn the_clis_three_filters_never_carry_the_archive() {
    for filter in [None, Some("open"), Some("done"), Some("all")] {
        assert_eq!(
            TicketQuery::parse(filter, None, None, None, None, false, false)
                .unwrap()
                .archived,
            Archived::Excluded
        );
    }
    assert_eq!(
        titles(&TicketQuery::parse(None, None, None, None, None, false, false).unwrap()),
        ["capture the idea", "parked work", "ready work", "in flight"]
    );
}

#[test]
fn archived_alone_is_the_archive_statuses_ignored() {
    let query = TicketQuery::parse(None, None, None, None, None, true, false).unwrap();
    assert_eq!(query.archived, Archived::Only);
    assert!(query.statuses.is_empty());
    assert_eq!(
        titles(&query),
        ["finished and filed away", "archived but not done"]
    );
}

#[test]
fn archived_named_with_a_filter_joins_that_filter() {
    let all = TicketQuery::parse(Some("all"), None, None, None, None, true, false).unwrap();
    assert_eq!(all.archived, Archived::Included);
    assert_eq!(titles(&all).len(), 7, "every ticket, the archive included");

    let done = TicketQuery::parse(Some("done"), None, None, None, None, true, false).unwrap();
    assert_eq!(done.archived, Archived::Included);
    assert_eq!(titles(&done), ["finished", "finished and filed away"]);
}

#[test]
fn an_explicit_status_narrows_the_archive_it_was_named_with() {
    let query = TicketQuery::parse(None, None, Some("backlog"), None, None, true, false).unwrap();
    assert_eq!(query.statuses, [Status::Backlog]);
    assert_eq!(titles(&query), ["archived but not done"]);
}

#[test]
fn the_inbox_is_the_open_non_archived_tickets_assigned_to_human() {
    let tickets = vec![
        Ticket {
            status: Status::Ready,
            assignee: Some(HUMAN.to_string()),
            ..ticket("asked of the person")
        },
        Ticket {
            status: Status::Done,
            assignee: Some(HUMAN.to_string()),
            ..ticket("asked and done")
        },
        Ticket {
            archived: true,
            assignee: Some(HUMAN.to_string()),
            ..ticket("asked and filed away")
        },
        Ticket {
            status: Status::Ready,
            assignee: Some("core".to_string()),
            ..ticket("agent work")
        },
        ticket("nobody's"),
    ];
    assert_eq!(
        TicketQuery::inbox()
            .apply(&tickets)
            .into_iter()
            .map(|ticket| ticket.title)
            .collect::<Vec<_>>(),
        ["asked of the person"]
    );
}

#[test]
fn the_inbox_is_empty_when_nothing_is_assigned_not_an_error() {
    let tickets = vec![Ticket {
        assignee: Some("ui".to_string()),
        ..ticket("agent work")
    }];
    assert!(TicketQuery::inbox().apply(&tickets).is_empty());
}

#[test]
fn an_answered_ticket_leaves_the_inbox_and_an_unanswered_one_stays() {
    let tickets = vec![
        Ticket {
            status: Status::OnHold,
            description: Some("## Why\n\nwhich option?".to_string()),
            assignee: Some(HUMAN.to_string()),
            ..ticket("still asking")
        },
        Ticket {
            status: Status::OnHold,
            description: Some(
                "## Why\n\nwhich option?\n\n## Answer \u{2014} 2026-10-26\nA".to_string(),
            ),
            assignee: Some(HUMAN.to_string()),
            ..ticket("decided")
        },
        Ticket {
            status: Status::OnHold,
            description: Some("The `## Answer` is recorded on `F1CEFA57`".to_string()),
            assignee: Some(HUMAN.to_string()),
            ..ticket("inline mention")
        },
    ];
    assert_eq!(
        TicketQuery::inbox()
            .apply(&tickets)
            .into_iter()
            .map(|ticket| ticket.title)
            .collect::<Vec<_>>(),
        ["still asking", "inline mention"]
    );
}

#[test]
fn only_the_inbox_hides_answered_tickets_the_agents_read_keeps_them_as_debt() {
    let answered = Ticket {
        status: Status::OnHold,
        description: Some("## Answer \u{2014} 2026-10-26\nA".to_string()),
        assignee: Some(HUMAN.to_string()),
        ..ticket("decided")
    };
    assert_eq!(
        TicketQuery {
            assignee: AssigneeAxis::Named(HUMAN.to_string()),
            ..TicketQuery::default()
        }
        .apply(std::slice::from_ref(&answered))
        .len(),
        1
    );
    assert_eq!(TicketQuery::default().apply(&[answered]).len(), 1);
}

#[test]
fn parse_rejects_a_bad_filter_or_status_by_name() {
    assert_eq!(
        TicketQuery::parse(Some("bogus"), None, None, None, None, false, false),
        Err(DomainError::InvalidFilter("bogus".to_string()))
    );
    assert_eq!(
        TicketQuery::parse(None, None, Some(""), None, None, false, false),
        Err(DomainError::InvalidStatus(String::new()))
    );
}

/// A ticket waiting on `blockers`, the derived `blocked` reading the edge alone.
fn waiting(title: &str, status: Status, blockers: &[(uuid::Uuid, Status)]) -> Ticket {
    Ticket {
        status,
        blocked_by: blockers
            .iter()
            .map(|(id, blocker_status)| TicketBlock {
                id: *id,
                reference: None,
                title: "blocker".to_string(),
                status: *blocker_status,
            })
            .collect(),
        ..ticket(title)
    }
}

#[test]
fn blocked_keeps_the_blocked_rows_and_drops_the_rest() {
    let tickets = vec![
        waiting(
            "waits on open work",
            Status::Ready,
            &[(uuid::Uuid::new_v4(), Status::InProgress)],
        ),
        Ticket {
            status: Status::Ready,
            ..ticket("free work")
        },
        waiting(
            "waits on finished work",
            Status::Ready,
            &[(uuid::Uuid::new_v4(), Status::Done)],
        ),
    ];
    let query = TicketQuery {
        blocked_only: true,
        ..TicketQuery::default()
    };
    assert_eq!(
        query
            .apply(&tickets)
            .into_iter()
            .map(|ticket| ticket.title)
            .collect::<Vec<_>>(),
        ["waits on open work"]
    );
    let open = TicketQuery {
        statuses: vec![Status::Ready, Status::InProgress],
        blocked_only: true,
        ..TicketQuery::default()
    };
    assert_eq!(
        open.apply(&tickets)
            .into_iter()
            .map(|ticket| ticket.title)
            .collect::<Vec<_>>(),
        ["waits on open work"]
    );
}

#[test]
fn blocked_combines_with_status() {
    let query = TicketQuery::parse(None, None, Some("ready"), None, None, false, true).unwrap();
    assert!(query.blocked_only);
    let tickets = vec![
        waiting(
            "blocked and ready",
            Status::Ready,
            &[(uuid::Uuid::new_v4(), Status::Backlog)],
        ),
        waiting(
            "blocked and backlog",
            Status::Backlog,
            &[(uuid::Uuid::new_v4(), Status::Backlog)],
        ),
        Ticket {
            status: Status::Ready,
            ..ticket("ready and free")
        },
    ];
    assert_eq!(
        query
            .apply(&tickets)
            .into_iter()
            .map(|ticket| ticket.title)
            .collect::<Vec<_>>(),
        ["blocked and ready"]
    );
}

#[test]
fn a_row_whose_blocker_is_done_is_not_blocked() {
    let lifted = waiting(
        "lifted",
        Status::Ready,
        &[(uuid::Uuid::new_v4(), Status::Done)],
    );
    assert!(!lifted.blocked());
    assert!(
        TicketQuery {
            blocked_only: true,
            ..TicketQuery::default()
        }
        .apply(&[lifted])
        .is_empty()
    );
}
