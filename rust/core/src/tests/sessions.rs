//! Ported from `SessionsTests`, `SessionVerbTests` and `UsageWrapperTests`: the
//! record's rules, the verb shapes those suites pinned, and the continuation the
//! usage wrapper carries.

use crate::error::DomainError;
use crate::session::{
    Ending, SessionOutcome, SessionStatus, Sessions, SkillCounts, Stamp, parse_skills,
};
use crate::store::Actor;
use crate::store::records::session_skills;

use super::support::{Sandbox, TICKET_ONE, TICKET_TWO, at};

fn start(sessions: &Sessions, actor: &Actor, id: &str, brief: &str) -> crate::session::Session {
    sessions
        .start(actor, id, "tandem", Some("builder"), brief, Some(&at(0)))
        .expect("a session starts")
        .value
}

#[test]
fn a_started_record_is_running_and_carries_nothing_else() {
    let sandbox = Sandbox::new("start");
    let sessions = Sessions::new(&sandbox.store());
    let opened = sessions
        .start(
            &sandbox.actor(),
            " ses_1 ",
            "tandem",
            Some(" builder "),
            "  Build the records domain.  ",
            Some(&at(0)),
        )
        .expect("a session starts");

    assert_eq!(opened.note, None, "the first start is the record");
    let record = opened.value;
    assert_eq!(record.id, "ses_1", "the handle is trimmed");
    assert_eq!(
        record.brief, "Build the records domain.",
        "the brief is trimmed"
    );
    assert_eq!(record.agent.as_deref(), Some("builder"));
    assert_eq!(record.project, "tandem");
    assert_eq!(record.status, SessionStatus::Running);
    assert_eq!(record.outcome, None);
    assert!(record.tickets.is_empty());
    assert_eq!(record.ended_at, None);
    assert_eq!(record.tokens, None, "absent, never a zero");
    assert_eq!(record.cost, None);
    assert_eq!(record.model.as_deref(), None);
    assert_eq!(
        record.skills, None,
        "a pass that loaded nothing counts nothing"
    );
    assert_eq!(record.started_at, at(0));
}

#[test]
fn a_second_start_returns_the_record_unchanged_with_a_note() {
    let sandbox = Sandbox::new("start-twice");
    let actor = sandbox.actor();
    let sessions = Sessions::new(&sandbox.store());
    let first = start(&sessions, &actor, "ses_1", "First.");

    let second = sessions
        .start(
            &actor,
            "ses_1",
            "tandem",
            Some("reviewer"),
            "Second.",
            Some(&at(9)),
        )
        .expect("a re-run is a success, not a failure");

    assert_eq!(second.value, first, "a record is not restarted");
    assert_eq!(
        second.note.as_deref(),
        Some("session ses_1 already recorded"),
        "the note says why nothing moved"
    );
}

#[test]
fn an_empty_brief_or_handle_is_a_domain_failure() {
    let sandbox = Sandbox::new("start-refused");
    let sessions = Sessions::new(&sandbox.store());
    let actor = sandbox.actor();

    assert_eq!(
        sessions
            .start(&actor, "ses_1", "tandem", None, "   ", None)
            .unwrap_err(),
        DomainError::EmptySessionBrief
    );
    assert_eq!(
        sessions
            .start(&actor, "  ", "tandem", None, "A brief.", None)
            .unwrap_err(),
        DomainError::EmptySessionId
    );
    assert!(
        sessions.list(None, None).unwrap().is_empty(),
        "a refused start writes nothing"
    );
}

#[test]
fn an_end_stamps_once_and_a_second_end_cannot_restate_it() {
    let sandbox = Sandbox::new("end");
    let actor = sandbox.actor();
    let sessions = Sessions::new(&sandbox.store());
    start(&sessions, &actor, "ses_1", "What the pass is for.");

    let stamp = sessions
        .end(
            &actor,
            "ses_1",
            Ending {
                outcome: SessionOutcome::Succeeded,
                summary: Some("  What landed.  "),
                tokens: Some(crate::session::SessionTokens {
                    input: Some(10),
                    output: Some(20),
                    reasoning: Some(5),
                    cache_read: Some(3),
                    cache_write: Some(1),
                }),
                cost: Some(0.5),
                model: Some("deepseek-v4.1-flash"),
                skills: Some(SkillCounts::from([("thinking".to_string(), 2)])),
                at: Some(&at(60)),
            },
        )
        .expect("the first end stamps");

    let Stamp {
        session,
        note,
        stamped,
    } = stamp;
    assert!(stamped);
    assert_eq!(note, None);
    assert_eq!(session.status, SessionStatus::Ended);
    assert_eq!(session.outcome, Some(SessionOutcome::Succeeded));
    assert_eq!(session.summary.as_deref(), Some("What landed."));
    assert_eq!(session.ended_at.as_deref(), Some(at(60).as_str()));
    assert_eq!(session.cost, Some(0.5));
    assert_eq!(session.model.as_deref(), Some("deepseek-v4.1-flash"));
    assert_eq!(
        session.skills,
        Some(SkillCounts::from([("thinking".to_string(), 2)]))
    );
    assert_eq!(
        session.tokens.as_ref().and_then(|tokens| tokens.input),
        Some(10)
    );

    let again = sessions
        .end(
            &actor,
            "ses_1",
            Ending {
                outcome: SessionOutcome::Failed,
                summary: Some("Something else."),
                cost: Some(99.0),
                at: Some(&at(99)),
                ..Ending::default()
            },
        )
        .expect("a second end is a success that changed nothing");

    assert!(!again.stamped, "the hook must not run twice");
    assert_eq!(again.note.as_deref(), Some("session ses_1 already ended"));
    assert_eq!(again.session, session, "the first stamp is the fact");
}

#[test]
fn an_end_of_an_unknown_session_names_the_ids_the_store_holds() {
    let sandbox = Sandbox::new("end-unknown");
    let actor = sandbox.actor();
    let sessions = Sessions::new(&sandbox.store());
    start(&sessions, &actor, "ses_1", "One.");

    let error = sessions
        .end(
            &actor,
            "ses_9",
            Ending {
                outcome: SessionOutcome::Failed,
                ..Ending::default()
            },
        )
        .unwrap_err();

    assert_eq!(
        error,
        DomainError::UnknownSession {
            id: "ses_9".to_string(),
            known: vec!["ses_1".to_string()],
        }
    );
    assert!(
        error.message().contains("ses_1"),
        "the message names what the store holds: {}",
        error.message()
    );
}

#[test]
fn a_link_appends_once_in_link_order() {
    let sandbox = Sandbox::new("link");
    let actor = sandbox.actor();
    let sessions = Sessions::new(&sandbox.store());
    start(&sessions, &actor, "ses_1", "One.");

    sessions.link(&actor, "ses_1", TICKET_TWO).unwrap();
    sessions.link(&actor, "ses_1", TICKET_ONE).unwrap();
    let repeated = sessions.link(&actor, "ses_1", TICKET_TWO).unwrap();

    assert_eq!(
        repeated.value.tickets,
        vec![TICKET_TWO.to_string(), TICKET_ONE.to_string()],
        "the order the links were made in, and no id twice"
    );
    assert_eq!(repeated.note, None);
}

#[test]
fn a_list_is_newest_first_and_takes_the_filters() {
    let sandbox = Sandbox::new("list");
    let actor = sandbox.actor();
    let sessions = Sessions::new(&sandbox.store());
    sessions
        .start(&actor, "ses_1", "tandem", None, "One.", Some(&at(0)))
        .unwrap();
    sessions
        .start(&actor, "ses_2", "fleet", None, "Two.", Some(&at(10)))
        .unwrap();
    sessions
        .start(&actor, "ses_3", "tandem", None, "Three.", Some(&at(20)))
        .unwrap();
    sessions
        .end(
            &actor,
            "ses_1",
            Ending {
                outcome: SessionOutcome::Unknown,
                at: Some(&at(30)),
                ..Ending::default()
            },
        )
        .unwrap();

    let all: Vec<String> = sessions
        .list(None, None)
        .unwrap()
        .into_iter()
        .map(|session| session.id)
        .collect();
    assert_eq!(all, vec!["ses_3", "ses_2", "ses_1"], "newest start first");

    let tandem: Vec<String> = sessions
        .list(Some("tandem"), None)
        .unwrap()
        .into_iter()
        .map(|session| session.id)
        .collect();
    assert_eq!(tandem, vec!["ses_3", "ses_1"]);

    let running: Vec<String> = sessions
        .list(None, Some(SessionStatus::Running))
        .unwrap()
        .into_iter()
        .map(|session| session.id)
        .collect();
    assert_eq!(running, vec!["ses_3", "ses_2"]);

    assert_eq!(sessions.show("ses_2").unwrap().project, "fleet");
    assert_eq!(
        sessions.show("ses_9").unwrap_err(),
        DomainError::UnknownSession {
            id: "ses_9".to_string(),
            known: vec![
                "ses_1".to_string(),
                "ses_2".to_string(),
                "ses_3".to_string()
            ],
        }
    );
}

#[test]
fn a_record_carrying_no_usage_reports_none() {
    let sandbox = Sandbox::new("no-usage");
    let actor = sandbox.actor();
    let sessions = Sessions::new(&sandbox.store());
    start(&sessions, &actor, "ses_1", "One.");

    let record = sessions.show("ses_1").unwrap();
    assert!(
        !record.carries_usage(),
        "nothing reported is not a reported zero"
    );
    assert_eq!(record.tokens, None);
    assert_eq!(record.cost, None);
    assert_eq!(record.skills, None);

    // And it stays absent through the store: the JSON omits the pieces rather
    // than writing nulls a reader would read as a report.
    let json = serde_json::to_value(&record).unwrap();
    assert!(json.get("tokens").is_none(), "tokens omitted: {json}");
    assert!(json.get("cost").is_none());
    assert!(json.get("model").is_none());
    assert!(json.get("skills").is_none());
    assert_eq!(json["startedAt"], at(0));
}

#[test]
fn a_record_carrying_two_tickets_lists_both_in_link_order() {
    let sandbox = Sandbox::new("two-tickets");
    let actor = sandbox.actor();
    let sessions = Sessions::new(&sandbox.store());
    start(&sessions, &actor, "ses_1", "One.");
    sessions.link(&actor, "ses_1", TICKET_ONE).unwrap();
    sessions.link(&actor, "ses_1", TICKET_TWO).unwrap();

    let record = sessions.show("ses_1").unwrap();
    assert_eq!(
        record.tickets,
        vec![TICKET_ONE.to_string(), TICKET_TWO.to_string()]
    );
}

#[test]
fn skills_parse_strictly_and_a_malformed_value_is_a_domain_failure() {
    assert_eq!(
        parse_skills("  ").unwrap(),
        None,
        "an empty flag counts nothing"
    );
    assert_eq!(
        parse_skills("thinking=2, plan=1").unwrap(),
        Some(SkillCounts::from([
            ("thinking".to_string(), 2),
            ("plan".to_string(), 1),
        ]))
    );
    assert_eq!(
        parse_skills("tdd=0").unwrap(),
        Some(SkillCounts::from([("tdd".to_string(), 0)])),
        "a reported zero is a count"
    );
    for bad in [
        "thinking",
        "=2",
        "thinking=",
        "thinking=x",
        "thinking=-1",
        "thinking=1,thinking=2",
        "a=1,,b=2",
    ] {
        assert_eq!(
            parse_skills(bad).unwrap_err(),
            DomainError::InvalidSkills(bad.to_string()),
            "{bad} is malformed"
        );
    }
}

#[test]
fn the_skill_counts_are_a_column_a_record_written_before_it_reads_empty() {
    let sandbox = Sandbox::new("skills-column");
    let actor = sandbox.actor();
    let store = sandbox.store();
    let sessions = Sessions::new(&store);

    // v6 adds the column: a fresh home carries it, and a record that reports no
    // skills stores NULL rather than an empty object.
    sessions
        .start(&actor, "ses_old", "tandem", None, "One.", Some(&at(0)))
        .unwrap();
    sessions
        .end(
            &actor,
            "ses_old",
            Ending {
                outcome: SessionOutcome::Succeeded,
                skills: Some(SkillCounts::from([("opencode".to_string(), 1)])),
                at: Some(&at(5)),
                ..Ending::default()
            },
        )
        .unwrap();

    let connection = rusqlite::Connection::open(store.database()).unwrap();
    assert_eq!(
        session_skills(&connection, "ses_old").unwrap(),
        Some("{\"opencode\":1}".to_string()),
        "the counts are one JSON object in the skills column"
    );
    assert_eq!(
        sessions.show("ses_old").unwrap().skills,
        Some(SkillCounts::from([("opencode".to_string(), 1)]))
    );
}

#[test]
fn the_continuation_mints_the_next_free_suffix_and_carries_the_tickets() {
    let sandbox = Sandbox::new("continuation");
    let actor = sandbox.actor();
    let sessions = Sessions::new(&sandbox.store());
    start(&sessions, &actor, "ses_1", "First pass.");
    sessions.link(&actor, "ses_1", TICKET_ONE).unwrap();
    sessions.link(&actor, "ses_1", TICKET_TWO).unwrap();
    sessions
        .end(
            &actor,
            "ses_1",
            Ending {
                outcome: SessionOutcome::Succeeded,
                summary: Some("Landed."),
                at: Some(&at(60)),
                ..Ending::default()
            },
        )
        .unwrap();

    let continued = sessions
        .continue_after(&actor, "ses_1", "A resumed pass.", Some(&at(90)))
        .expect("the pass gets its own record");

    assert_eq!(continued.note, None);
    assert_eq!(continued.value.id, "ses_1-2");
    assert_eq!(
        continued.value.project, "tandem",
        "the ended record's project"
    );
    assert_eq!(continued.value.agent.as_deref(), Some("builder"));
    assert_eq!(continued.value.status, SessionStatus::Running);
    assert_eq!(
        continued.value.tickets,
        vec![TICKET_ONE.to_string(), TICKET_TWO.to_string()],
        "the usage comment needs the board the pass carried"
    );

    // A third pass takes the next free suffix, not `-2` again.
    sessions
        .end(
            &actor,
            "ses_1-2",
            Ending {
                outcome: SessionOutcome::Succeeded,
                at: Some(&at(120)),
                ..Ending::default()
            },
        )
        .unwrap();
    let third = sessions
        .continue_after(&actor, "ses_1", "A third pass.", Some(&at(150)))
        .unwrap();
    assert_eq!(third.value.id, "ses_1-3");
}

#[test]
fn a_running_record_is_not_continued() {
    let sandbox = Sandbox::new("continuation-running");
    let actor = sandbox.actor();
    let sessions = Sessions::new(&sandbox.store());
    start(&sessions, &actor, "ses_1", "One.");

    let outcome = sessions
        .continue_after(&actor, "ses_1", "A resumed pass.", Some(&at(10)))
        .expect("a running record takes the usage itself");

    assert_eq!(outcome.value.id, "ses_1");
    assert_eq!(
        outcome.note.as_deref(),
        Some("session ses_1 is still running")
    );
    assert_eq!(
        sessions.list(None, None).unwrap().len(),
        1,
        "nothing was minted"
    );
}
