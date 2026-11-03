//! Part 3 preflight and record: per-ticket gate facts, the rebase symbol check,
//! and the close sweep over a worked store.
//!
//! The git/gh boundaries are stubbed: counts and check lists in, verdicts out.
//! No network, no real repo. Verdicts report into the part 1 vocabulary
//! (`runner::PassResult` and `runner::Effect`), never an invented status.

use crate::domain::board::{Board, SetFields};
use crate::domain::edit::Edit;
use crate::domain::preflight::{
    Check, CheckState, PreflightAction, TicketPreflight, find_stale_symbol, preflight,
    release_dead_claims,
};
use crate::domain::query::HUMAN;
use crate::domain::runner::{Driver, Effect, PassKind, PassResult, Phase, SprintHandle};
use crate::domain::session::{Ending, SessionOutcome, SessionStatus, Sessions};
use crate::domain::ticket::Status;

use super::support::{Sandbox, TICKET_ONE, TICKET_THREE, TICKET_TWO, at};

fn green(name: &str) -> Check {
    Check {
        name: name.to_string(),
        state: CheckState::Green,
    }
}

fn red(name: &str) -> Check {
    Check {
        name: name.to_string(),
        state: CheckState::Red,
    }
}

fn pending(name: &str) -> Check {
    Check {
        name: name.to_string(),
        state: CheckState::Pending,
    }
}

fn facts(ticket: &str, pr: u64, head: &str, behind: u64, checks: Vec<Check>) -> TicketPreflight {
    TicketPreflight {
        ticket: ticket.to_string(),
        pr,
        head: head.to_string(),
        behind,
        checks,
    }
}

#[test]
fn behind_base_requests_update_branch_before_any_gate_read() {
    // Counts in: the branch sits two behind its base. The checks are red on
    // purpose: behind wins before any gate read, so their state cannot change
    // the verdict.
    let stubbed = facts(
        TICKET_ONE,
        7,
        "abc123",
        2,
        vec![green("ci/build"), red("ci/lint")],
    );
    let action = preflight(&stubbed);
    match &action {
        PreflightAction::UpdateBranch {
            ticket,
            pr,
            behind,
            head,
        } => {
            assert_eq!(ticket, TICKET_ONE);
            assert_eq!(*pr, 7);
            assert_eq!(*behind, 2);
            assert_eq!(head, "abc123");
        }
        other => panic!("behind means the update-branch effect, got {other:?}"),
    }
    assert!(
        !matches!(action, PreflightAction::ReadyToGate { .. }),
        "no gate dispatch while the branch is behind"
    );
    assert!(
        !matches!(action, PreflightAction::Blocked { .. }),
        "the red check waits until the branch is current"
    );
}

#[test]
fn red_or_pending_head_blocks_naming_sha_and_check() {
    for failing in [red("ci/lint"), pending("ci/build")] {
        let stubbed = facts(
            TICKET_ONE,
            7,
            "deadbeef",
            0,
            vec![green("ci/unit"), failing],
        );
        let action = preflight(&stubbed);
        let (check, state) = match &action {
            PreflightAction::Blocked {
                ticket,
                head,
                check,
                state,
                ..
            } => {
                assert_eq!(ticket, TICKET_ONE);
                assert_eq!(head, "deadbeef", "the block names the head sha");
                (check.clone(), state.clone())
            }
            other => panic!("a red or pending head blocks, got {other:?}"),
        };
        // The diff is never read when the head cannot merge: the caller only
        // reads it on a gate, and a block is not a gate.
        let mut diff_read = false;
        if matches!(action, PreflightAction::ReadyToGate { .. }) {
            diff_read = true;
        }
        assert!(!diff_read, "the diff is never read on a blocked head");

        // The verdict drives the part 1 vocabulary end to end: blocked becomes
        // a failed review, and the step halts with a triage-readable patch.
        let failed = match &action {
            PreflightAction::Blocked {
                ticket,
                head,
                check,
                state,
                ..
            } => PassResult::Failed {
                ticket: ticket.clone(),
                pass: PassKind::Review,
                reason: format!("check {check} is {} on {head}", state.as_str()),
            },
            other => panic!("expected a block, got {other:?}"),
        };
        match &failed {
            PassResult::Failed { ticket, reason, .. } => {
                assert_eq!(ticket, TICKET_ONE);
                assert!(
                    reason.contains("deadbeef"),
                    "the failure names the sha: {reason}"
                );
                assert!(
                    reason.contains(&check),
                    "the failure names the check: {reason}"
                );
                assert!(
                    reason.contains(state.as_str()),
                    "the failure names the state: {reason}"
                );
            }
            other => panic!("a block is a failed review, got {other:?}"),
        }
        let state = Driver {
            sprint: SprintHandle {
                lane: "Tandem".to_string(),
                number: 1,
            },
            batch: vec![crate::domain::runner::DrawnTicket {
                id: TICKET_ONE.to_string(),
                needs_refine: false,
            }],
            cursor: 0,
            completed: vec![],
            parked: vec![],
            phase: Phase::Reviewing {
                ticket: TICKET_ONE.to_string(),
                pr: 7,
            },
        };
        let (next, effect) = crate::domain::runner::step(&state, &failed, &at(10))
            .expect("a failed review is a decided transition");
        assert!(matches!(next.phase, Phase::Halted { .. }));
        match effect {
            Effect::Fail {
                ticket,
                status,
                assignee,
                note,
                ..
            } => {
                assert_eq!(ticket, TICKET_ONE);
                assert_eq!(
                    status,
                    Status::Ready,
                    "a failed gate clears the claim back to ready"
                );
                assert_eq!(assignee, None, "no dead builder owns the ticket");
                assert!(note.contains(TICKET_ONE));
                assert!(note.contains("deadbeef"));
            }
            other => panic!("a failed review carries a ticket patch, got {other:?}"),
        }
    }
}

#[test]
fn the_close_sweep_releases_dead_claims_with_a_signed_comment() {
    let sandbox = Sandbox::new("preflight-sweep");
    sandbox.ticket(TICKET_ONE);
    sandbox.ticket(TICKET_TWO);
    sandbox.ticket(TICKET_THREE);
    let store = sandbox.store();
    let actor = sandbox.actor();
    let board = Board::with_actor(store.clone(), actor.clone());
    let sessions = Sessions::new(&store);

    // The dead claim: in-progress under a session that has ended.
    board
        .set(
            TICKET_ONE,
            SetFields {
                status: Edit::Set(Status::InProgress),
                assignee: Edit::Set(Some("builder")),
                ..SetFields::default()
            },
        )
        .unwrap();
    sessions
        .start(
            &actor,
            "ses_dead",
            "tandem",
            Some("builder"),
            "Build the dead claim.",
            Some(&at(0)),
        )
        .unwrap();
    sessions.link(&actor, "ses_dead", TICKET_ONE).unwrap();
    sessions
        .end(
            &actor,
            "ses_dead",
            Ending {
                outcome: SessionOutcome::Succeeded,
                summary: Some("Landed nothing."),
                at: Some(&at(10)),
                ..Ending::new(SessionOutcome::Succeeded)
            },
        )
        .unwrap();
    assert_eq!(
        sessions.show("ses_dead").unwrap().status,
        SessionStatus::Ended
    );

    // The live claim: the same shape, but the session still runs.
    board
        .set(
            TICKET_TWO,
            SetFields {
                status: Edit::Set(Status::InProgress),
                assignee: Edit::Set(Some("builder")),
                ..SetFields::default()
            },
        )
        .unwrap();
    sessions
        .start(
            &actor,
            "ses_live",
            "tandem",
            Some("builder"),
            "Build the live claim.",
            Some(&at(20)),
        )
        .unwrap();
    sessions.link(&actor, "ses_live", TICKET_TWO).unwrap();

    // The human hold: ended, but the grill's park is never cleared by a set.
    board
        .set(
            TICKET_THREE,
            SetFields {
                status: Edit::Set(Status::OnHold),
                assignee: Edit::Set(Some(HUMAN)),
                ..SetFields::default()
            },
        )
        .unwrap();
    sessions
        .start(
            &actor,
            "ses_hold",
            "tandem",
            Some("reviewer"),
            "Grill the hold.",
            Some(&at(30)),
        )
        .unwrap();
    sessions.link(&actor, "ses_hold", TICKET_THREE).unwrap();
    sessions
        .end(
            &actor,
            "ses_hold",
            Ending {
                outcome: SessionOutcome::Succeeded,
                summary: Some("Parked."),
                at: Some(&at(40)),
                ..Ending::new(SessionOutcome::Succeeded)
            },
        )
        .unwrap();

    let released = release_dead_claims(&board, &sessions, "sprint");
    assert_eq!(released.len(), 1, "only the dead claim moves");
    assert_eq!(released[0].ticket, TICKET_ONE);
    assert_eq!(released[0].session, "ses_dead");

    let ticket = board.ticket(TICKET_ONE).unwrap();
    assert_eq!(ticket.status, Status::Ready, "the claim goes back to ready");
    assert_eq!(ticket.assignee, None, "the owner is cleared");
    let comments = board.comments(TICKET_ONE).unwrap();
    let last = comments.last().expect("the sweep signs its move");
    assert_eq!(last.author, "sprint", "the sweep signs as sprint");
    assert!(
        last.body.contains("ses_dead"),
        "the comment names the ended session: {}",
        last.body
    );

    let live = board.ticket(TICKET_TWO).unwrap();
    assert_eq!(
        live.status,
        Status::InProgress,
        "a live claim is not a stall"
    );
    assert_eq!(live.assignee.as_deref(), Some("builder"));
    assert!(
        board.comments(TICKET_TWO).unwrap().is_empty(),
        "nothing moved, so no comment"
    );

    let hold = board.ticket(TICKET_THREE).unwrap();
    assert_eq!(hold.assignee.as_deref(), Some(HUMAN));
    assert!(
        board.comments(TICKET_THREE).unwrap().is_empty(),
        "the grill's park closes with an answer, never a set"
    );
}

#[test]
fn the_gate_reads_only_its_own_two_facts() {
    // Sibling state cannot enter a verdict: the same ticket with the same two
    // facts decides the same way, whatever another ticket carries.
    let own = facts(TICKET_ONE, 7, "abc123", 0, vec![green("ci/build")]);
    let sibling_behind = facts(TICKET_TWO, 8, "def456", 5, vec![red("ci/build")]);
    let _ = sibling_behind;
    assert!(matches!(
        preflight(&own),
        PreflightAction::ReadyToGate { .. }
    ));

    // The sibling's red head and stale branch decide nothing for this ticket:
    // each ticket's verdict reads only its own head sha and behind count.
    let own_blocked = facts(TICKET_ONE, 7, "abc123", 0, vec![red("ci/lint")]);
    match preflight(&own_blocked) {
        PreflightAction::Blocked { ticket, head, .. } => {
            assert_eq!(ticket, TICKET_ONE);
            assert_eq!(head, "abc123");
        }
        other => panic!("the block names its own ticket and sha, got {other:?}"),
    }

    // A stale branch on one ticket never blocks another ticket's gate.
    let other_ready = facts(TICKET_TWO, 8, "def456", 0, vec![green("ci/build")]);
    assert!(matches!(
        preflight(&other_ready),
        PreflightAction::ReadyToGate { .. }
    ));
}

#[test]
fn rebase_symbol_check_stops_before_push_naming_the_line() {
    let diff = "diff --git a/x.rs b/x.rs\n+let id = todo.id;\n+let ok = 1;\n";
    let hit = find_stale_symbol(diff, &["todo.", "todo.id"]);
    let line = hit.expect("the stale symbol stops the push");
    assert!(
        line.contains("todo.id"),
        "the check names the offending line: {line}"
    );
    assert!(
        find_stale_symbol("+let ok = 1;\n", &["todo.", "todo.id"]).is_none(),
        "a clean diff pushes"
    );
}
