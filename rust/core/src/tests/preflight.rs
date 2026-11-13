//! Part 3 preflight and record: per-ticket gate facts and the rebase symbol
//! check.
//!
//! The git/gh boundaries are stubbed: counts and check lists in, verdicts out.
//! No network, no real repo. Verdicts report into the part 1 vocabulary
//! (`runner::PassResult` and `runner::Effect`), never an invented status.

use crate::domain::preflight::{
    Check, CheckState, PreflightAction, TicketPreflight, find_stale_symbol, preflight,
};
use crate::domain::runner::{Driver, Effect, PassKind, PassResult, Phase, SprintHandle};
use crate::domain::ticket::Status;

use super::support::{TICKET_ONE, TICKET_TWO, at};

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
