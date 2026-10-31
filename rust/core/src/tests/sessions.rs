//! Ported from `SessionsTests`, `SessionVerbTests` and `UsageWrapperTests`: the
//! record's rules, the verb shapes those suites pinned, and the continuation the
//! usage wrapper carries.

use crate::domain::error::DomainError;
use crate::domain::session::{
    Ending, HarnessSession, HarnessTransport, SessionOutcome, SessionStatus, Sessions, SkillCounts,
    Stamp, parse_skills,
};
use crate::store::Actor;
use crate::store::records::session_skills;

use super::support::{Sandbox, TICKET_ONE, TICKET_TWO, at};

fn start(
    sessions: &Sessions,
    actor: &Actor,
    id: &str,
    brief: &str,
) -> crate::domain::session::Session {
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
                tokens: Some(crate::domain::session::SessionTokens {
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
fn a_stamp_the_store_cannot_read_is_refused_before_it_is_written() {
    let sandbox = Sandbox::new("stamp");
    let actor = sandbox.actor();
    let sessions = Sessions::new(&sandbox.store());

    assert_eq!(
        sessions
            .start(
                &actor,
                "ses_1",
                "tandem",
                None,
                "One.",
                Some("2026-10-30 12:00:00"),
            )
            .unwrap_err(),
        DomainError::InvalidStamp("2026-10-30 12:00:00".to_string()),
        "an undecodable stamp is caught at the write, not at every later read"
    );
    assert!(
        sessions.list(None, None).unwrap().is_empty(),
        "a refused stamp writes nothing"
    );

    start(&sessions, &actor, "ses_1", "One.");
    let refused = sessions
        .end(
            &actor,
            "ses_1",
            Ending {
                outcome: SessionOutcome::Succeeded,
                at: Some("yesterday"),
                ..Ending::default()
            },
        )
        .unwrap_err();
    assert_eq!(refused, DomainError::InvalidStamp("yesterday".to_string()));
    assert_eq!(
        sessions.show("ses_1").unwrap().status,
        SessionStatus::Running,
        "the record is untouched"
    );
}

#[test]
fn a_read_of_a_v2_home_answers_its_records_with_no_skill_counts() {
    let sandbox = Sandbox::new("v2-home");
    let store = sandbox.store();
    std::fs::create_dir_all(store.home().directory()).expect("the home directory is creatable");
    {
        // A home the previous build wrote: the seam at v2, the session row
        // without the skills column that v3 adds.
        let connection = rusqlite::Connection::open(store.database()).expect("the v2 file opens");
        crate::store::schema::migrate_with(
            &connection,
            &crate::store::schema::MIGRATIONS[..2],
            &Actor::Human,
        )
        .expect("a v2 home is the shape the previous build writes");
        connection
            .execute(
                "INSERT INTO sessions
                    (id, project, agent, brief, status, outcome, summary, started_at,
                     ended_at, tokens_input, tokens_output, tokens_reasoning,
                     tokens_cache_read, tokens_cache_write, seconds, cost, model,
                     tickets, author)
                 VALUES (?1, 'tandem', NULL, 'A record the v2 build wrote.', 'ended',
                         'succeeded', NULL, ?2, ?3, NULL, NULL, NULL, NULL, NULL,
                         NULL, NULL, NULL, '[]', 'human')",
                rusqlite::params!["ses_v2", at(0), at(5)],
            )
            .expect("the v2 row lands");
    }

    let sessions = Sessions::new(&store);
    let listed = sessions
        .list(None, None)
        .expect("a read never writes (ADR 0031), and a v2 home still reads");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, "ses_v2");
    assert_eq!(
        listed[0].skills, None,
        "the additive column reads NULL, not as an engine error"
    );
    assert_eq!(
        listed[0].harness_session_id, None,
        "the harness columns are additive too: a row written before them answers absent"
    );
    assert_eq!(listed[0].harness_transport, None);
    assert_eq!(
        sessions
            .show("ses_v2")
            .expect("show reads the same row")
            .skills,
        None
    );
    assert_eq!(
        store.schema_version().unwrap(),
        Some(2),
        "the read migrated nothing"
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

    // v3 adds the column: a fresh home carries it, and a record that reports no
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
fn two_records_point_at_one_harness_session_and_neither_is_overwritten() {
    let sandbox = Sandbox::new("harness-pointer");
    let actor = sandbox.actor();
    let sessions = Sessions::new(&sandbox.store());
    let harness = HarnessSession {
        id: "ses_harness_1".to_string(),
        transport: HarnessTransport::Database,
    };

    // Two passes of one harness session: neither id is the harness's, and the
    // second start writes a record rather than continuing the first (RET-44).
    let first = sessions
        .start_with_harness(
            &actor,
            "ses_pass_1",
            "tandem",
            Some("builder"),
            "The first pass.",
            harness.clone(),
            Some(&at(0)),
        )
        .expect("the first pass records")
        .value;
    let second = sessions
        .start_with_harness(
            &actor,
            "ses_pass_2",
            "tandem",
            Some("refiner"),
            "The second pass.",
            harness.clone(),
            Some(&at(10)),
        )
        .expect("the second pass records")
        .value;

    assert_eq!(first.id, "ses_pass_1");
    assert_eq!(second.id, "ses_pass_2", "a record is Tandem's own handle");
    assert_eq!(
        first.harness_session_id.as_deref(),
        Some("ses_harness_1"),
        "the harness session is a foreign pointer, and both records name it"
    );
    assert_eq!(second.harness_session_id.as_deref(), Some("ses_harness_1"));
    assert_eq!(first.harness_transport, Some(HarnessTransport::Database));
    assert_eq!(second.harness_transport, Some(HarnessTransport::Database));

    sessions
        .end(
            &actor,
            "ses_pass_1",
            Ending {
                outcome: SessionOutcome::Succeeded,
                summary: Some("The first pass landed."),
                cost: Some(0.25),
                model: Some("deepseek-v4.1-flash"),
                at: Some(&at(60)),
                ..Ending::default()
            },
        )
        .expect("the first record stamps");
    sessions
        .end(
            &actor,
            "ses_pass_2",
            Ending {
                outcome: SessionOutcome::Succeeded,
                summary: Some("The second pass landed."),
                cost: Some(0.5),
                model: Some("claude-sonnet-5"),
                at: Some(&at(120)),
                ..Ending::default()
            },
        )
        .expect("the second record stamps");

    assert_eq!(
        sessions.list(None, None).unwrap().len(),
        2,
        "two records, not one overwritten"
    );
    let first = sessions.show("ses_pass_1").unwrap();
    let second = sessions.show("ses_pass_2").unwrap();
    assert_eq!(first.agent.as_deref(), Some("builder"));
    assert_eq!(second.agent.as_deref(), Some("refiner"));
    assert_eq!(first.summary.as_deref(), Some("The first pass landed."));
    assert_eq!(second.summary.as_deref(), Some("The second pass landed."));
    assert_eq!(first.cost, Some(0.25), "the first stamp survives");
    assert_eq!(second.cost, Some(0.5));
    assert_eq!(
        first.harness_session_id, second.harness_session_id,
        "one harness session, two attributed records"
    );
    assert_eq!(first.status, SessionStatus::Ended);
    assert_eq!(second.status, SessionStatus::Ended);
}

#[test]
fn a_record_with_no_harness_session_records_and_answers_absent() {
    let sandbox = Sandbox::new("no-harness");
    let actor = sandbox.actor();
    let sessions = Sessions::new(&sandbox.store());

    // The hand-driven case: a caller names the record and the environment names
    // no harness, and the run still records.
    let record = start(&sessions, &actor, "ses_hand", "A hand-driven pass.");
    assert_eq!(record.harness_session_id, None);
    assert_eq!(record.harness_transport, None);
    assert_eq!(sessions.list(None, None).unwrap().len(), 1);

    let record = sessions.show("ses_hand").unwrap();
    assert_eq!(record.status, SessionStatus::Running);
    let json = serde_json::to_value(&record).unwrap();
    assert!(
        json.get("harnessSessionId").is_none(),
        "an absent harness session is absent on the wire, never null: {json}"
    );
    assert!(json.get("harnessTransport").is_none());
}

#[test]
fn a_harness_pointer_resolves_to_the_newest_record_that_names_it() {
    let sandbox = Sandbox::new("pointer-resolve");
    let actor = sandbox.actor();
    let sessions = Sessions::new(&sandbox.store());
    let harness = HarnessSession {
        id: "ses_harness_1".to_string(),
        transport: HarnessTransport::Database,
    };
    sessions
        .start_with_harness(
            &actor,
            "ses_pass_1",
            "tandem",
            Some("builder"),
            "The first pass.",
            harness.clone(),
            Some(&at(0)),
        )
        .unwrap();
    sessions
        .start_with_harness(
            &actor,
            "ses_pass_2",
            "tandem",
            Some("refiner"),
            "The second pass.",
            harness,
            Some(&at(10)),
        )
        .unwrap();

    assert_eq!(
        sessions.resolve("ses_harness_1").unwrap(),
        "ses_pass_2",
        "the external wrapper's harness id reaches the newest record through the column"
    );
    assert_eq!(
        sessions.resolve("ses_pass_1").unwrap(),
        "ses_pass_1",
        "an exact record id is never second-guessed"
    );
    assert_eq!(
        sessions.resolve("ses_missing").unwrap_err(),
        DomainError::UnknownSession {
            id: "ses_missing".to_string(),
            known: vec!["ses_pass_1".to_string(), "ses_pass_2".to_string()],
        }
    );
}

#[test]
fn a_harness_session_is_trimmed_and_an_empty_one_is_refused() {
    let sandbox = Sandbox::new("harness-empty");
    let actor = sandbox.actor();
    let sessions = Sessions::new(&sandbox.store());

    let trimmed = sessions
        .start_with_harness(
            &actor,
            "ses_pass_1",
            "tandem",
            None,
            "One.",
            HarnessSession {
                id: "  ses_harness_1  ".to_string(),
                transport: HarnessTransport::EventStream,
            },
            Some(&at(0)),
        )
        .expect("a named harness session records")
        .value;
    assert_eq!(trimmed.harness_session_id.as_deref(), Some("ses_harness_1"));
    assert_eq!(
        trimmed.harness_transport,
        Some(HarnessTransport::EventStream)
    );

    assert_eq!(
        sessions
            .start_with_harness(
                &actor,
                "ses_pass_2",
                "tandem",
                None,
                "Two.",
                HarnessSession {
                    id: "   ".to_string(),
                    transport: HarnessTransport::Database,
                },
                None,
            )
            .unwrap_err(),
        DomainError::EmptyHarnessSessionId
    );
    assert_eq!(
        sessions.list(None, None).unwrap().len(),
        1,
        "a refused start writes nothing"
    );
}
