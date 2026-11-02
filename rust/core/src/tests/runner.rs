//! The sprint runner's transition table: the draw, every state and result, the
//! failed pass, and a scripted two-ticket run.
//!
//! The runner is pure (no I/O, no spawn, no git): these tests build ticket
//! snapshots and stubbed pass results by hand, so the only record of the run is
//! the run log the transitions emit.

use crate::domain::query::HUMAN;
use crate::domain::runner::{
    DrawnTicket, Driver, Effect, Next, PassKind, PassResult, Phase, RunnerError, SprintHandle,
    TicketSnapshot, draw, is_finished_brief,
};
use crate::domain::ticket::Status;

const AT_ONE: &str = "2026-11-02T12:00:10Z";
const AT_TWO: &str = "2026-11-02T12:00:20Z";
const AT_THREE: &str = "2026-11-02T12:00:30Z";

const BRIEF: &str = "## Why\n\nThe loop is prose.\n\n## What\n\nA driver owns it.\n\n## Done when\n\nTwo tickets close.\n\n## Open questions\n\n(none)\n";
const NO_BRIEF: &str = "Just a title-shaped hope, no sections at all.";
const OPEN_BRIEF: &str = "## Why\n\nThe loop is prose.\n\n## What\n\nA driver owns it.\n\n## Done when\n\nTwo tickets close.\n\n## Open questions\n\nWhich executor runs the passes?\n";

fn blocker(id: &str, reference: &str, status: Status) -> crate::domain::runner::BlockerRef {
    crate::domain::runner::BlockerRef {
        id: id.to_string(),
        reference: Some(reference.to_string()),
        title: format!("blocker {reference}"),
        status,
    }
}

#[allow(clippy::too_many_arguments)]
fn snap(
    id: &str,
    reference: &str,
    status: Status,
    blocked: bool,
    blockers: Vec<crate::domain::runner::BlockerRef>,
    assignee: Option<&str>,
    description: Option<&str>,
    archived: bool,
) -> TicketSnapshot {
    TicketSnapshot {
        id: id.to_string(),
        reference: Some(reference.to_string()),
        title: format!("ticket {reference}"),
        status,
        blocked,
        blockers,
        assignee: assignee.map(str::to_string),
        description: description.map(str::to_string),
        archived,
    }
}

fn ready(id: &str, reference: &str) -> TicketSnapshot {
    snap(id, reference, Status::Ready, false, vec![], None, None, false)
}

fn backlog_plain(id: &str, reference: &str) -> TicketSnapshot {
    snap(
        id,
        reference,
        Status::Backlog,
        false,
        vec![],
        None,
        Some(NO_BRIEF),
        false,
    )
}

fn handle() -> SprintHandle {
    SprintHandle {
        lane: "Tandem".to_string(),
        number: 1,
    }
}

fn driver_in(phase: Phase) -> Driver {
    Driver {
        sprint: handle(),
        batch: vec![DrawnTicket {
            id: "a".to_string(),
            needs_refine: false,
        }],
        cursor: 0,
        completed: vec![],
        parked: vec![],
        phase,
    }
}

#[test]
fn draw_takes_ready_first_up_to_five_and_skips_blocked() {
    let tickets = vec![
        ready("r1", "TAN-1"),
        backlog_plain("b1", "TAN-2"),
        snap(
            "x1",
            "TAN-9",
            Status::Ready,
            true,
            vec![blocker("h1", "TAN-3", Status::OnHold)],
            None,
            None,
            false,
        ),
        ready("r2", "TAN-4"),
        snap("o1", "TAN-5", Status::OnHold, false, vec![], None, None, false),
        snap(
            "b2",
            "TAN-6",
            Status::Backlog,
            false,
            vec![],
            None,
            Some(BRIEF),
            false,
        ),
        snap(
            "x2",
            "TAN-10",
            Status::Backlog,
            true,
            vec![blocker("w1", "TAN-7", Status::InProgress)],
            None,
            Some(NO_BRIEF),
            false,
        ),
        ready("r3", "TAN-8"),
        backlog_plain("b3", "TAN-11"),
        ready("r4", "TAN-12"),
        ready("r5", "TAN-13"),
        ready("r9", "TAN-14"),
        snap("d1", "TAN-15", Status::Done, false, vec![], None, None, false),
        snap(
            "a1",
            "TAN-16",
            Status::Ready,
            false,
            vec![],
            None,
            None,
            true,
        ),
    ];

    let drawn = draw(&tickets);
    let batch: Vec<&str> = drawn.batch.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(
        batch,
        vec!["r1", "r2", "r3", "r4", "r5"],
        "ready first in stored order, five is a cap: the backlog and TAN-14 wait"
    );
    assert!(
        drawn.batch.iter().all(|t| !t.needs_refine),
        "a ready batch needs no refine"
    );
    assert_eq!(
        drawn.skips,
        vec![
            "skipped TAN-9: blocked by TAN-3 (on-hold)",
            "skipped TAN-10: blocked by TAN-7 (in-progress)",
        ],
        "each skipped row names its blocker, in stored order"
    );
    assert_eq!(
        drawn.promotions,
        vec!["b2".to_string()],
        "the finished brief is promoted even though the cap left it out of the batch"
    );
    assert!(is_finished_brief(Some(BRIEF)));
    assert!(!is_finished_brief(Some(NO_BRIEF)));
    assert!(!is_finished_brief(Some(OPEN_BRIEF)));
    assert!(!is_finished_brief(None));

    let empty = draw(&[]);
    assert!(empty.batch.is_empty());
    assert!(empty.skips.is_empty());
    assert!(empty.promotions.is_empty());
}

#[test]
fn every_state_and_result_names_a_next_state() {
    let refining = Phase::Refining {
        ticket: "a".to_string(),
    };
    let building = Phase::Building {
        ticket: "a".to_string(),
    };
    let reviewing = Phase::Reviewing {
        ticket: "a".to_string(),
        pr: 7,
    };

    let (next, effect) = crate::domain::runner::step(
        &driver_in(refining.clone()),
        &PassResult::Refined {
            ticket: "a".to_string(),
            ready: true,
        },
        AT_ONE,
    )
    .expect("a ready brief builds");
    assert_eq!(next.phase, building.clone());
    assert_eq!(
        effect,
        Effect::Spawn {
            ticket: "a".to_string(),
            pass: PassKind::Build,
        }
    );

    let (next, effect) = crate::domain::runner::step(
        &driver_in(building.clone()),
        &PassResult::Built {
            ticket: "a".to_string(),
            pr: 7,
        },
        AT_ONE,
    )
    .expect("a diff reviews");
    assert_eq!(next.phase, reviewing.clone());
    assert_eq!(
        effect,
        Effect::Spawn {
            ticket: "a".to_string(),
            pass: PassKind::Review,
        }
    );

    let (next, effect) = crate::domain::runner::step(
        &driver_in(reviewing.clone()),
        &PassResult::Reviewed {
            ticket: "a".to_string(),
            merged: true,
        },
        AT_ONE,
    )
    .expect("a merge records and closes a one-ticket batch");
    assert_eq!(next.phase, Phase::Closing);
    match effect {
        Effect::RecordAndAdvance { ticket, pr, next, .. } => {
            assert_eq!(ticket, "a");
            assert_eq!(pr, 7);
            assert_eq!(next, Next::Close);
        }
        other => panic!("a merge records its run entry, got {other:?}"),
    }

    let (next, effect) = crate::domain::runner::step(
        &driver_in(reviewing.clone()),
        &PassResult::Reviewed {
            ticket: "a".to_string(),
            merged: false,
        },
        AT_ONE,
    )
    .expect("an escalation is a decided halt, not a default");
    assert!(matches!(next.phase, Phase::Halted { .. }));
    assert!(matches!(effect, Effect::Escalate { .. }));

    let (next, effect) = crate::domain::runner::step(
        &driver_in(refining.clone()),
        &PassResult::Refined {
            ticket: "a".to_string(),
            ready: false,
        },
        AT_ONE,
    )
    .expect("a parked brief stalls a one-ticket batch");
    assert!(matches!(next.phase, Phase::Halted { .. }));
    assert!(matches!(effect, Effect::ParkAndAdvance { .. }));

    let (next, effect) = crate::domain::runner::step(
        &driver_in(building.clone()),
        &PassResult::Failed {
            ticket: "a".to_string(),
            pass: PassKind::Build,
            reason: "the worktree died".to_string(),
        },
        AT_ONE,
    )
    .expect("a failed pass halts");
    assert!(matches!(next.phase, Phase::Halted { .. }));
    assert!(matches!(effect, Effect::Fail { .. }));

    for phase in [
        Phase::Closing,
        Phase::Done,
        Phase::Halted {
            ticket: "a".to_string(),
            reason: "the worktree died".to_string(),
        },
    ] {
        let refused = crate::domain::runner::step(
            &driver_in(phase.clone()),
            &PassResult::Built {
                ticket: "a".to_string(),
                pr: 7,
            },
            AT_ONE,
        )
        .unwrap_err();
        assert_eq!(
            refused,
            RunnerError::NoPassOutstanding(format!("{phase:?} carries no pass")),
            "a terminal state refuses pass results, never defaults them"
        );
    }

    let orphan = crate::domain::runner::step(
        &driver_in(refining.clone()),
        &PassResult::Built {
            ticket: "b".to_string(),
            pr: 7,
        },
        AT_ONE,
    )
    .unwrap_err();
    assert_eq!(
        orphan,
        RunnerError::OrphanResult {
            expected: "a".to_string(),
            got: "b".to_string(),
        }
    );

    let wrong_pass = crate::domain::runner::step(
        &driver_in(refining.clone()),
        &PassResult::Built {
            ticket: "a".to_string(),
            pr: 7,
        },
        AT_ONE,
    )
    .unwrap_err();
    assert!(matches!(wrong_pass, RunnerError::UnexpectedResult(_)));

    for phase in [refining, building, reviewing] {
        let unknown = crate::domain::runner::step(
            &driver_in(phase),
            &PassResult::Unknown {
                raw: "merged-ish".to_string(),
            },
            AT_ONE,
        )
        .unwrap_err();
        assert_eq!(
            unknown,
            RunnerError::UnknownResult("merged-ish".to_string()),
            "an unknown result is refused, never defaulted"
        );
    }

    assert_eq!(
        PassKind::parse("quilt").unwrap_err(),
        RunnerError::UnknownResult("quilt".to_string())
    );

    let bad_stamp = crate::domain::runner::step(
        &driver_in(Phase::Building {
            ticket: "a".to_string(),
        }),
        &PassResult::Built {
            ticket: "a".to_string(),
            pr: 7,
        },
        "soon",
    )
    .unwrap_err();
    assert_eq!(bad_stamp, RunnerError::InvalidStamp("soon".to_string()));
}

#[test]
fn a_failed_pass_leaves_a_state_triage_can_read() {
    let state = driver_in(Phase::Building {
        ticket: "TAN-1".to_string(),
    });
    let state = Driver {
        batch: vec![DrawnTicket {
            id: "TAN-1".to_string(),
            needs_refine: false,
        }],
        ..state
    };
    let (next, effect) = crate::domain::runner::step(
        &state,
        &PassResult::Failed {
            ticket: "TAN-1".to_string(),
            pass: PassKind::Build,
            reason: "the worktree died".to_string(),
        },
        AT_ONE,
    )
    .expect("a failure is a decided transition");
    assert_eq!(
        next.phase,
        Phase::Halted {
            ticket: "TAN-1".to_string(),
            reason: "the worktree died".to_string(),
        }
    );
    match effect {
        Effect::Fail {
            ticket,
            status,
            assignee,
            at,
            note,
        } => {
            assert_eq!(ticket, "TAN-1");
            assert_eq!(
                status,
                Status::Ready,
                "the claim is cleared back to ready, never left in-progress behind a dead session"
            );
            assert_eq!(assignee, None, "no dead builder owns the ticket");
            assert_eq!(at, AT_ONE, "the run entry carries its stamp");
            assert!(
                note.contains("TAN-1") && note.contains("the worktree died"),
                "the entry names the ticket and the reason: {note}"
            );
        }
        other => panic!("a failure carries a ticket patch and a dated entry, got {other:?}"),
    }
    assert_eq!(HUMAN, "human");
}

#[test]
fn a_scripted_two_ticket_run_closes_with_the_run_log_only() {
    let tickets = vec![
        ready("TAN-2", "TAN-2"),
        backlog_plain("TAN-1", "TAN-1"),
    ];
    let drawn = draw(&tickets);
    let batch: Vec<&str> = drawn.batch.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(batch, vec!["TAN-2", "TAN-1"]);
    assert_eq!(
        drawn.batch[1].needs_refine, true,
        "the backlog ticket refines first"
    );

    let (mut state, first) =
        crate::domain::runner::start(handle(), &drawn.batch);
    assert_eq!(
        first,
        Effect::Spawn {
            ticket: "TAN-2".to_string(),
            pass: PassKind::Build,
        }
    );

    let mut run_log: Vec<String> = vec![];
    let script = vec![
        (
            PassResult::Built {
                ticket: "TAN-2".to_string(),
                pr: 11,
            },
            AT_ONE,
        ),
        (
            PassResult::Reviewed {
                ticket: "TAN-2".to_string(),
                merged: true,
            },
            AT_TWO,
        ),
        (
            PassResult::Refined {
                ticket: "TAN-1".to_string(),
                ready: true,
            },
            AT_TWO,
        ),
        (
            PassResult::Built {
                ticket: "TAN-1".to_string(),
                pr: 12,
            },
            AT_THREE,
        ),
        (
            PassResult::Reviewed {
                ticket: "TAN-1".to_string(),
                merged: true,
            },
            AT_THREE,
        ),
    ];
    for (result, at) in &script {
        let (next, effect) = crate::domain::runner::step(&state, result, at)
            .unwrap_or_else(|error| panic!("the scripted result {result:?} steps: {error:?}"));
        match effect {
            Effect::Spawn { .. } => {}
            Effect::RecordAndAdvance { at, note, .. } => run_log.push(format!("{at} {note}")),
            other => panic!("the happy path only spawns and records, got {other:?}"),
        }
        state = next;
    }

    assert_eq!(state.phase, Phase::Closing);
    assert_eq!(state.completed, vec!["TAN-2".to_string(), "TAN-1".to_string()]);
    assert_eq!(run_log.len(), 2, "the run log is the only record of the run");
    assert!(run_log[0].contains("TAN-2"), "entries land in run order: {}", run_log[0]);
    assert!(run_log[1].contains("TAN-1"), "entries land in run order: {}", run_log[1]);

    let note = crate::domain::runner::close_note(&state);
    assert!(
        note.contains("TAN-2") && note.contains("TAN-1"),
        "the close names what landed: {note}"
    );
    let done = crate::domain::runner::closed(&state).expect("closing closes");
    assert_eq!(done.phase, Phase::Done);

    let (empty, none) = crate::domain::runner::start(handle(), &[]);
    assert_eq!(empty.phase, Phase::Done, "zero drawn opens no sprint");
    assert_eq!(none, Effect::None);
}
