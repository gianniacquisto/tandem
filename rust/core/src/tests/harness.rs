//! The harness read over the loopback API (TAN-276): `session end
//! --usage-from-harness` reads the reporting harness through its own HTTP
//! surface, never through another application's database.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;

use crate::domain::error::DomainError;
use crate::domain::harness::read_api;
use crate::domain::session::{
    Ending, HarnessSession, HarnessTransport, SessionOutcome, SessionStatus, Sessions, SkillCounts,
};

use super::support::{Sandbox, TICKET_ONE, TICKET_TWO, at};

/// A canned route: the path prefix the stub answers and the body it answers
/// with. The stub asserts the loopback password on every request, so a missing
/// or wrong `Authorization` is a 401 before any route runs.
struct Route {
    prefix: String,
    status: u16,
    body: String,
}

struct Stub {
    base: String,
    handle: Option<thread::JoinHandle<()>>,
    seen_auth: Arc<AtomicUsize>,
}

impl Stub {
    fn start(password: &str, routes: Vec<Route>, expected_hits: usize) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("the stub binds loopback");
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let password = password.to_string();
        let seen_auth = Arc::new(AtomicUsize::new(0));
        let seen = Arc::clone(&seen_auth);
        let expected = encode_basic(&password);
        let handle = thread::spawn(move || {
            for stream in listener.incoming().take(expected_hits) {
                let mut stream = stream.expect("a harness connection arrives");
                let mut reader = BufReader::new(stream.try_clone().expect("the stub clones"));
                let mut headline = String::new();
                reader
                    .read_line(&mut headline)
                    .expect("the request line arrives");
                let mut content_length = 0usize;
                let mut auth = String::new();
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).expect("headers arrive");
                    let trimmed = line.trim_end();
                    if trimmed.is_empty() {
                        break;
                    }
                    if let Some(colon) = trimmed.find(':') {
                        let (name, value) = trimmed.split_at(colon);
                        if name.eq_ignore_ascii_case("authorization") {
                            auth = value[1..].trim().to_string();
                        }
                        if name.eq_ignore_ascii_case("content-length") {
                            content_length = value[1..].trim().parse().unwrap_or(0);
                        }
                    }
                    if let Some(value) = trimmed.strip_prefix("Content-Length:") {
                        content_length = value.trim().parse().unwrap_or(0);
                    }
                }
                if content_length > 0 {
                    let mut discard = vec![0u8; content_length];
                    reader.read_exact(&mut discard).ok();
                }
                let path = headline
                    .split_whitespace()
                    .nth(1)
                    .unwrap_or("/")
                    .to_string();
                let authorized = auth == format!("Basic {expected}");
                if authorized {
                    seen.fetch_add(1, Ordering::SeqCst);
                }
                let (status, body) = if !authorized {
                    (
                        401,
                        r#"{"_tag":"UnauthorizedError","message":"Authentication required"}"#
                            .to_string(),
                    )
                } else {
                    routes
                        .iter()
                        .find(|route| path.starts_with(&route.prefix))
                        .map(|route| (route.status, route.body.clone()))
                        .unwrap_or_else(|| {
                            (
                                404,
                                r#"{"_tag":"SessionNotFoundError","message":"Session not found"}"#
                                    .to_string(),
                            )
                        })
                };
                let reason = match status {
                    200 => "OK",
                    401 => "Unauthorized",
                    404 => "Not Found",
                    _ => "Error",
                };
                let reply = format!(
                    "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream
                    .write_all(reply.as_bytes())
                    .expect("the stub answers");
            }
        });
        Self {
            base,
            handle: Some(handle),
            seen_auth,
        }
    }

    fn join(mut self) {
        if let Some(handle) = self.handle.take() {
            handle.join().expect("the stub thread ends");
        }
    }
}

impl Drop for Stub {
    fn drop(&mut self) {
        // The thread ends after its expected hits; a test that drops early
        // leaves the listener to close with the socket.
    }
}

fn encode_basic(password: &str) -> String {
    encode_base64(format!("opencode:{password}").as_bytes())
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut i = 0;
    while i < bytes.len() {
        let a = bytes[i] as u32;
        let b = if i + 1 < bytes.len() {
            bytes[i + 1] as u32
        } else {
            0
        };
        let c = if i + 2 < bytes.len() {
            bytes[i + 2] as u32
        } else {
            0
        };
        let triple = (a << 16) | (b << 8) | c;
        out.push(TABLE[((triple >> 18) & 63) as usize] as char);
        out.push(TABLE[((triple >> 12) & 63) as usize] as char);
        out.push(if i + 1 < bytes.len() {
            TABLE[((triple >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if i + 2 < bytes.len() {
            TABLE[(triple & 63) as usize] as char
        } else {
            '='
        });
        i += 3;
    }
    out
}

fn fixture(name: &str) -> String {
    let path = format!(
        "{}/src/tests/fixtures/harness-api/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(path).expect("the fixture is readable")
}

#[test]
fn the_api_session_and_messages_become_the_usage_triple_and_the_skill_counts() {
    let session = fixture("session.json");
    let page_one = fixture("messages-page-1.json");
    let stub = Stub::start(
        "harness-secret",
        vec![
            Route {
                prefix: "/api/session/ses_f26dab226ffed3tYjvxcXUQwEg/message?limit=100&cursor="
                    .to_string(),
                status: 200,
                body: r#"{"data":[],"cursor":{"next":null}}"#.to_string(),
            },
            Route {
                prefix: "/api/session/ses_f26dab226ffed3tYjvxcXUQwEg/message".to_string(),
                status: 200,
                body: page_one,
            },
            Route {
                prefix: "/api/session/ses_f26dab226ffed3tYjvxcXUQwEg".to_string(),
                status: 200,
                body: session,
            },
        ],
        3,
    );

    let usage = read_api(
        &stub.base,
        "harness-secret",
        "ses_f26dab226ffed3tYjvxcXUQwEg",
    )
    .expect("the loopback answers");

    assert_eq!(usage.tokens.input, Some(25_765));
    assert_eq!(usage.tokens.output, Some(1_309));
    assert_eq!(usage.tokens.reasoning, Some(2_743));
    assert_eq!(usage.tokens.cache_read, Some(78_032));
    assert_eq!(usage.tokens.cache_write, Some(0));
    assert_eq!(usage.cost, Some(0.0));
    assert_eq!(
        usage.model.as_deref(),
        Some("space-bunny-free"),
        "the model is the id inside the harness object, not the JSON"
    );
    assert_eq!(
        usage.skills,
        Some(SkillCounts::from([("opencode".to_string(), 1)])),
        "one skill tool part, counted once"
    );
    assert!(
        stub.seen_auth.load(Ordering::SeqCst) >= 2,
        "every request carries the password"
    );
    stub.join();
}

#[test]
fn repeats_count_once_blank_and_non_skill_parts_are_skipped() {
    let session = r#"{"data":{"id":"ses_1","tokens":{"input":10,"output":5,"reasoning":0,"cache":{"read":0,"write":0}},"cost":0.1,"model":{"id":"model-a"}}}"#.to_string();
    let messages = r#"{"data":[{"id":"msg_1","content":[
        {"type":"tool","id":"prt_1","tool":"skill","state":{"input":{"id":"opencode"}}},
        {"type":"tool","id":"prt_1","tool":"skill","state":{"input":{"id":"opencode"}}},
        {"type":"tool","id":"prt_2","name":"skill","state":{"input":{"id":" tdd "}}},
        {"type":"tool","id":"prt_3","name":"skill","state":{"input":{"id":"   "}}},
        {"type":"tool","id":"prt_4","name":"shell","state":{"input":{"id":"opencode"}}},
        {"type":"text","id":"prt_5","text":"hello"}
    ]}],"cursor":{"next":null}}"#
        .to_string();
    let stub = Stub::start(
        "harness-secret",
        vec![
            Route {
                prefix: "/api/session/ses_1/message".to_string(),
                status: 200,
                body: messages,
            },
            Route {
                prefix: "/api/session/ses_1".to_string(),
                status: 200,
                body: session,
            },
        ],
        2,
    );

    let usage = read_api(&stub.base, "harness-secret", "ses_1").expect("the loopback answers");

    assert_eq!(
        usage.skills,
        Some(SkillCounts::from([
            ("opencode".to_string(), 1),
            ("tdd".to_string(), 1),
        ])),
        "a repeat is one load, a blank name is no load, a non-skill tool is no load"
    );
    stub.join();
}

#[test]
fn an_empty_message_list_reports_no_skills_but_keeps_the_tokens() {
    let session = r#"{"data":{"id":"ses_1","tokens":{"input":7,"output":3},"cost":0.2,"model":{"id":"model-a"}}}"#.to_string();
    let messages = r#"{"data":[],"cursor":{"next":null}}"#.to_string();
    let stub = Stub::start(
        "harness-secret",
        vec![
            Route {
                prefix: "/api/session/ses_1/message".to_string(),
                status: 200,
                body: messages,
            },
            Route {
                prefix: "/api/session/ses_1".to_string(),
                status: 200,
                body: session,
            },
        ],
        2,
    );

    let usage = read_api(&stub.base, "harness-secret", "ses_1").expect("the loopback answers");

    assert_eq!(usage.tokens.input, Some(7));
    assert_eq!(usage.skills, None, "absence, never an empty map");
    stub.join();
}

#[test]
fn a_null_model_reports_no_model() {
    let session =
        r#"{"data":{"id":"ses_1","tokens":{"input":1},"cost":0.0,"model":null}}"#.to_string();
    let messages = r#"{"data":[],"cursor":{"next":null}}"#.to_string();
    let stub = Stub::start(
        "harness-secret",
        vec![
            Route {
                prefix: "/api/session/ses_1/message".to_string(),
                status: 200,
                body: messages,
            },
            Route {
                prefix: "/api/session/ses_1".to_string(),
                status: 200,
                body: session,
            },
        ],
        2,
    );

    let usage = read_api(&stub.base, "harness-secret", "ses_1").expect("the loopback answers");

    assert_eq!(
        usage.model, None,
        "JSON null is no model, not a model called null"
    );
    stub.join();
}

#[test]
fn an_unknown_session_answers_unknown_harness_session() {
    let stub = Stub::start("harness-secret", vec![], 1);

    assert_eq!(
        read_api(&stub.base, "harness-secret", "ses_missing").unwrap_err(),
        DomainError::UnknownHarnessSession {
            id: "ses_missing".to_string(),
        }
    );
    stub.join();
}

#[test]
fn a_wrong_password_answers_harness_unauthorized() {
    let session = fixture("session.json");
    let stub = Stub::start(
        "harness-secret",
        vec![Route {
            prefix: "/api/session/ses_1".to_string(),
            status: 200,
            body: session,
        }],
        1,
    );

    match read_api(&stub.base, "wrong-secret", "ses_1") {
        Err(DomainError::HarnessUnauthorized { url }) => {
            assert!(
                url.starts_with("http://127.0.0.1:"),
                "the failure names the server: {url}"
            );
            assert!(
                !url.contains("wrong-secret"),
                "the password never reaches the message"
            );
        }
        other => panic!("expected HarnessUnauthorized, got {other:?}"),
    }
    stub.join();
}

#[test]
fn an_unreachable_server_answers_no_harness_server() {
    let dead = TcpListener::bind("127.0.0.1:0").expect("a port is taken");
    let port = dead.local_addr().unwrap().port();
    drop(dead);
    let base = format!("http://127.0.0.1:{port}");

    match read_api(&base, "harness-secret", "ses_1") {
        Err(DomainError::NoHarnessServer { url }) => {
            assert_eq!(url, base, "the failure names what was tried");
        }
        other => panic!("expected NoHarnessServer, got {other:?}"),
    }
}

#[test]
fn a_non_loopback_url_is_refused_without_connecting() {
    match read_api("http://example.com:4096", "harness-secret", "ses_1") {
        Err(DomainError::NoHarnessServer { url }) => {
            assert_eq!(url, "http://example.com:4096");
        }
        other => panic!("expected NoHarnessServer, got {other:?}"),
    }
}

#[test]
fn a_second_page_is_followed_through_the_cursor() {
    let session =
        r#"{"data":{"id":"ses_1","tokens":{"input":1},"cost":0.0,"model":{"id":"m"}}}"#.to_string();
    let page_one = r#"{"data":[{"id":"msg_1","content":[{"type":"tool","id":"prt_1","name":"skill","state":{"input":{"id":"alpha"}}}]}],"cursor":{"next":"token-abc"}}"#.to_string();
    let page_two = r#"{"data":[{"id":"msg_2","content":[{"type":"tool","id":"prt_2","name":"skill","state":{"input":{"id":"beta"}}}]}],"cursor":{"next":null}}"#.to_string();
    let stub = Stub::start(
        "harness-secret",
        vec![
            Route {
                prefix: "/api/session/ses_1/message?limit=100&cursor=token-abc".to_string(),
                status: 200,
                body: page_two,
            },
            Route {
                prefix: "/api/session/ses_1/message".to_string(),
                status: 200,
                body: page_one,
            },
            Route {
                prefix: "/api/session/ses_1".to_string(),
                status: 200,
                body: session,
            },
        ],
        3,
    );

    let usage = read_api(&stub.base, "harness-secret", "ses_1").expect("the loopback answers");

    assert_eq!(
        usage.skills,
        Some(SkillCounts::from([
            ("alpha".to_string(), 1),
            ("beta".to_string(), 1),
        ])),
        "the cursor is followed until absent"
    );
    stub.join();
}

#[test]
fn the_read_counts_assistant_messages_and_their_tool_parts() {
    let session =
        r#"{"data":{"id":"ses_1","tokens":{"input":10},"cost":0.1,"model":{"id":"model-a"}}}"#
            .to_string();
    let messages = r#"{"data":[
        {"id":"msg_1","type":"assistant","content":[
            {"type":"tool","id":"prt_1","name":"shell","state":{"input":{"command":"ls"}}},
            {"type":"tool","id":"prt_2","name":"read","state":{"input":{"filePath":"a"}}}
        ]},
        {"id":"msg_2","type":"assistant","content":[{"type":"text","id":"prt_3","text":"done"}]},
        {"id":"msg_3","type":"user","content":[{"type":"text","id":"prt_4","text":"go"}]},
        {"id":"msg_4","content":[{"type":"tool","id":"prt_5","name":"shell"}]}
    ],"cursor":{"next":null}}"#
        .to_string();
    let stub = Stub::start(
        "harness-secret",
        vec![
            Route {
                prefix: "/api/session/ses_1/message".to_string(),
                status: 200,
                body: messages,
            },
            Route {
                prefix: "/api/session/ses_1".to_string(),
                status: 200,
                body: session,
            },
        ],
        2,
    );

    let usage = read_api(&stub.base, "harness-secret", "ses_1").expect("the loopback answers");

    assert_eq!(usage.turns, Some(2), "two assistant messages are two turns");
    assert_eq!(
        usage.calls,
        Some(2),
        "two tool parts ride the assistant messages; the user message and the \
         typeless message contribute no turn and no call"
    );
    stub.join();
}

#[test]
fn the_counts_sum_across_pages_and_skip_a_child_session_s_parts() {
    let session =
        r#"{"data":{"id":"ses_1","tokens":{"input":10},"cost":0.1,"model":{"id":"model-a"}}}"#
            .to_string();
    let page_one = r#"{"data":[
        {"id":"msg_1","type":"assistant","content":[
            {"type":"tool","id":"prt_1","name":"shell","sessionID":"ses_1"}
        ]}
    ],"cursor":{"next":"page-2"}}"#
        .to_string();
    let page_two = r#"{"data":[
        {"id":"msg_2","type":"assistant","content":[
            {"type":"tool","id":"prt_2","name":"shell","sessionID":"ses_1"},
            {"type":"tool","id":"prt_3","name":"shell","sessionID":"ses_child"}
        ]},
        {"id":"msg_3","type":"user","content":[
            {"type":"tool","id":"prt_4","name":"shell","sessionID":"ses_1"}
        ]}
    ],"cursor":{"next":null}}"#
        .to_string();
    let stub = Stub::start(
        "harness-secret",
        vec![
            Route {
                prefix: "/api/session/ses_1/message?limit=100&cursor=page-2".to_string(),
                status: 200,
                body: page_two,
            },
            Route {
                prefix: "/api/session/ses_1/message".to_string(),
                status: 200,
                body: page_one,
            },
            Route {
                prefix: "/api/session/ses_1".to_string(),
                status: 200,
                body: session,
            },
        ],
        3,
    );

    let usage = read_api(&stub.base, "harness-secret", "ses_1").expect("the loopback answers");

    assert_eq!(usage.turns, Some(2), "the turns sum across pages");
    assert_eq!(
        usage.calls,
        Some(2),
        "a child session's tool part is not this session's call, and a user \
         message carries no call"
    );
    stub.join();
}

#[test]
fn parts_of_another_session_are_not_this_session_usage() {
    let session =
        r#"{"data":{"id":"ses_1","tokens":{"input":1},"cost":0.0,"model":{"id":"m"}}}"#.to_string();
    let messages = r#"{"data":[{"id":"msg_1","content":[
        {"type":"tool","id":"prt_1","name":"skill","sessionID":"ses_other","state":{"input":{"id":"other"}}},
        {"type":"tool","id":"prt_2","name":"skill","sessionID":"ses_1","state":{"input":{"id":"mine"}}}
    ]}],"cursor":{"next":null}}"#.to_string();
    let stub = Stub::start(
        "harness-secret",
        vec![
            Route {
                prefix: "/api/session/ses_1/message".to_string(),
                status: 200,
                body: messages,
            },
            Route {
                prefix: "/api/session/ses_1".to_string(),
                status: 200,
                body: session,
            },
        ],
        2,
    );

    let usage = read_api(&stub.base, "harness-secret", "ses_1").expect("the loopback answers");

    assert_eq!(
        usage.skills,
        Some(SkillCounts::from([("mine".to_string(), 1)])),
        "another session's parts are not this session's usage"
    );
    stub.join();
}

#[test]
fn a_second_pass_reads_the_harness_into_a_record_of_its_own() {
    let sandbox = Sandbox::new("harness-wrapper");
    let session = r#"{"data":{"id":"ses_1","tokens":{"input":25765,"output":3100,"reasoning":900,"cache":{"read":12000,"write":150}},"cost":0.42,"model":{"id":"deepseek-v4.1-flash"}}}"#.to_string();
    let messages = r#"{"data":[
        {"id":"msg_1","content":[{"type":"reasoning","text":"thinking"}]},
        {"id":"msg_2","content":[{"type":"tool","id":"a","name":"skill","state":{"input":{"id":"opencode"}}}]},
        {"id":"msg_3","content":[{"type":"tool","id":"b","name":"skill","state":{"input":{"id":"opencode"}}},{"type":"tool","id":"c","name":"skill","state":{"input":{"id":" tdd "}}}]}
    ],"cursor":{"next":null}}"#.to_string();
    let stub = Stub::start(
        "harness-secret",
        vec![
            Route {
                prefix: "/api/session/ses_1/message".to_string(),
                status: 200,
                body: messages,
            },
            Route {
                prefix: "/api/session/ses_1".to_string(),
                status: 200,
                body: session,
            },
        ],
        2,
    );
    let store = sandbox.store();
    let sessions = Sessions::new(&store);
    let actor = sandbox.actor();
    let harness = HarnessSession {
        id: "ses_1".to_string(),
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
        .expect("the first record starts");
    sessions.link(&actor, "ses_pass_1", TICKET_ONE).unwrap();
    sessions.link(&actor, "ses_pass_1", TICKET_TWO).unwrap();
    sessions
        .end(
            &actor,
            "ses_pass_1",
            Ending {
                outcome: SessionOutcome::Succeeded,
                summary: Some("The first pass landed."),
                at: Some(&at(60)),
                ..Ending::default()
            },
        )
        .expect("the first record stamps");

    sessions
        .start_with_harness(
            &actor,
            "ses_pass_2",
            "tandem",
            Some("builder"),
            "A second pass in the same harness session.",
            harness,
            Some(&at(90)),
        )
        .expect("the second pass gets its own record");
    sessions.link(&actor, "ses_pass_2", TICKET_ONE).unwrap();
    sessions.link(&actor, "ses_pass_2", TICKET_TWO).unwrap();

    let usage = read_api(&stub.base, "harness-secret", "ses_1").expect("the harness answers");
    let stamp = sessions
        .end(
            &actor,
            "ses_pass_2",
            Ending {
                outcome: SessionOutcome::Succeeded,
                summary: Some("What the second pass landed."),
                tokens: usage.tokens(),
                cost: usage.cost,
                model: usage.model.as_deref(),
                skills: usage.skills.clone(),
                turns: usage.turns,
                calls: usage.calls,
                at: Some(&at(150)),
            },
        )
        .expect("the second record stamps");
    stub.join();

    assert!(stamp.stamped, "the stamp lands once");
    assert_eq!(stamp.session.id, "ses_pass_2");
    assert_eq!(
        stamp.session.harness_session_id.as_deref(),
        Some("ses_1"),
        "the harness session is the pointer, not the handle"
    );
    assert_eq!(stamp.session.status, SessionStatus::Ended);
    assert_eq!(
        stamp.session.tickets,
        vec![TICKET_ONE.to_string(), TICKET_TWO.to_string()]
    );
    assert_eq!(stamp.session.cost, Some(0.42));
    assert_eq!(stamp.session.model.as_deref(), Some("deepseek-v4.1-flash"));
    assert_eq!(
        stamp.session.skills,
        Some(SkillCounts::from([
            ("opencode".to_string(), 2),
            ("tdd".to_string(), 1),
        ]))
    );
    assert_eq!(
        stamp
            .session
            .tokens
            .as_ref()
            .and_then(|tokens| tokens.input),
        Some(25_765)
    );

    let first = sessions.show("ses_pass_1").unwrap();
    assert_eq!(first.cost, None);
    assert_eq!(first.status, SessionStatus::Ended);
    assert_eq!(
        first.harness_session_id.as_deref(),
        Some("ses_1"),
        "both records still name the harness session they ran in"
    );
    assert_eq!(sessions.list(None, None).unwrap().len(), 2);
}
