//! The remote CLI (ADR 0047): with `TANDEM_SERVER` set, each verb is one MCP
//! `tools/call` to `<server>/mcp`, and prints what the local verb prints.
//!
//! The verb-to-tool table is the core's (`tandem_core::mcp::remote_verb`); this
//! module only turns argv into the tool's arguments and the answer into stdout
//! and an exit code. The token comes from `TANDEM_TOKEN`, is sent only as a
//! bearer header and only over https or to a loopback host, and is never
//! echoed.

use std::time::Duration;

use serde_json::{Map, Value, json};

use super::*;

/// The environment variable that names the remote server's base URL.
pub(super) const SERVER_VARIABLE: &str = "TANDEM_SERVER";

/// Run `command` against the server at `server`.
pub(super) fn run(server: &str, command: &Command) -> Result<(), Failure> {
    let (verb, mut arguments) = call_for(command)?;
    // The local verb prints the whole ticket, so the remote one asks for it:
    // the lean answer is the agent's default, never the CLI's (MCP-3). A tool
    // with no such argument ignores it.
    arguments.insert("detail".into(), Value::String("full".into()));
    let remote = tandem_core::mcp::remote_verb(&verb).ok_or_else(|| {
        Failure::message(format!(
            "tandem {verb}: this verb has no remote form; it runs against the local store only."
        ))
    })?;
    let token = env::var(super::TOKEN_VARIABLE)
        .ok()
        .filter(|token| !token.is_empty());
    let target = Target::parse(server, token.as_deref())?;
    let frame = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": {"name": remote.tool, "arguments": Value::Object(arguments)},
    });
    let answer = target.post(&frame.to_string(), token.as_deref())?;
    let result = tool_result(&answer)?;
    let stdout = remote.stdout(result);
    print(&stdout)?;
    if verb == "doctor" && stdout.get("ok") == Some(&Value::Bool(false)) {
        let failures: Vec<String> = stdout
            .get("checks")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter(|check| check.get("ok") == Some(&Value::Bool(false)))
            .map(|check| {
                format!(
                    "doctor: {} \u{2014} {}",
                    check["name"].as_str().unwrap_or(""),
                    check["detail"].as_str().unwrap_or("")
                )
            })
            .collect();
        return Err(Failure {
            message: failures.join("\n"),
            exit: 1,
        });
    }
    Ok(())
}

/// The server's URL, checked: where the POST goes and whether a token may ride.
struct Target {
    url: String,
}

impl Target {
    fn parse(server: &str, token: Option<&str>) -> Result<Self, Failure> {
        let server = server.trim();
        let Some((scheme, rest)) = server.split_once("://") else {
            return Err(Failure::message(format!(
                "{SERVER_VARIABLE} must be an http:// or https:// URL."
            )));
        };
        let scheme = scheme.to_ascii_lowercase();
        if scheme != "http" && scheme != "https" {
            return Err(Failure::message(format!(
                "{SERVER_VARIABLE} must be an http:// or https:// URL."
            )));
        }
        let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
        if authority.is_empty() || authority.contains('@') {
            return Err(Failure::message(format!(
                "{SERVER_VARIABLE} must name a host, without credentials in the URL."
            )));
        }
        if scheme == "http" && token.is_some() && !loopback_host(authority) {
            return Err(Failure::message(format!(
                "Refusing to send {} over plain http to a non-loopback host. Use an https URL, \
                 or a loopback address.",
                super::TOKEN_VARIABLE
            )));
        }
        Ok(Self {
            url: format!("{scheme}://{}/mcp", server_base(rest)),
        })
    }

    /// One POST, no redirect followed (a redirect would carry the token to a
    /// host the human did not name), and the body of a 200 answer.
    fn post(&self, body: &str, token: Option<&str>) -> Result<String, Failure> {
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .max_redirects(0)
            .timeout_global(Some(Duration::from_secs(60)))
            .build()
            .into();
        let mut request = agent
            .post(&self.url)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("X-Tandem-Agent", Actor::current().as_str());
        if let Some(token) = token {
            request = request.header("Authorization", &format!("Bearer {token}"));
        }
        let mut response = request.send(body).map_err(|error| {
            Failure::message(format!(
                "Cannot reach the tandem server at {}: {}. Try again.",
                self.url,
                error_text(&error)
            ))
        })?;
        let status = response.status().as_u16();
        let text = response.body_mut().read_to_string().unwrap_or_default();
        match status {
            200 => Ok(text),
            401 => Err(Failure::message(
                "The server refused the token (401). Check TANDEM_TOKEN.",
            )),
            _ => Err(Failure::message(format!(
                "The server refused the request ({status}): {}",
                text.lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(200)
                    .collect::<String>()
            ))),
        }
    }
}

/// An error's text without any URL header: the library's own words only.
fn error_text(error: &ureq::Error) -> String {
    error.to_string()
}

/// The authority and path of the URL with no trailing slash.
fn server_base(rest: &str) -> String {
    rest.trim_end_matches('/').to_string()
}

/// Whether the URL's host is `localhost` or a loopback IP.
fn loopback_host(authority: &str) -> bool {
    let host = match authority.strip_prefix('[') {
        Some(bracketed) => bracketed.split(']').next().unwrap_or(""),
        None => authority
            .rsplit_once(':')
            .map_or(authority, |(host, _)| host),
    };
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

/// The tool result's JSON, or the failure its answer is: a JSON-RPC error and a
/// tool result with `isError` both exit 1, in the server's words.
fn tool_result(answer: &str) -> Result<Value, Failure> {
    let frame: Value = serde_json::from_str(answer)
        .map_err(|_| Failure::message("The server's answer is not readable. Try again."))?;
    if let Some(error) = frame.get("error") {
        return Err(Failure::message(
            error["message"].as_str().unwrap_or("the call failed"),
        ));
    }
    let text = frame["result"]["content"][0]["text"].as_str().unwrap_or("");
    if frame["result"]["isError"] == Value::Bool(true) {
        return Err(Failure::message(text));
    }
    serde_json::from_str(text)
        .map_err(|_| Failure::message("The server's result is not readable. Try again."))
}

// MARK: - argv to arguments

fn put(map: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    if let Some(value) = value {
        map.insert(key.to_string(), Value::String(value.to_string()));
    }
}

fn index(raw: Option<&str>) -> Result<Option<Value>, Failure> {
    match raw {
        None => Ok(None),
        Some(raw) => raw
            .parse::<i64>()
            .map(|index| Some(json!(index)))
            .map_err(|_| Failure::domain(DomainError::InvalidIndex(raw.to_string()))),
    }
}

/// The verb the command spells and the tool arguments it carries. A flag the
/// wire has no field for is refused by name, never dropped.
fn call_for(command: &Command) -> Result<(String, Map<String, Value>), Failure> {
    let mut map = Map::new();
    let verb = match command {
        Command::Serve(_) => {
            return Err(Failure::message(
                "tandem serve: this verb has no remote form; it runs here.",
            ));
        }
        Command::Export(_) | Command::Import(_) => {
            // Neither verb is an MCP tool: both open the home's files.
            let name = if matches!(command, Command::Export(_)) {
                "export"
            } else {
                "import"
            };
            return Err(Failure::message(format!(
                "tandem {name}: this verb has no remote form; it opens a home on this machine. Unset \
                 TANDEM_SERVER, or run it on the machine that holds the home{}.",
                if name == "import" {
                    " (for import, pass --home <empty dir>)"
                } else {
                    ""
                }
            )));
        }
        Command::Doctor => "doctor",
        Command::Ticket { command } => {
            let command = command.as_ref().ok_or_else(Failure::usage)?;
            match command {
                TicketCommand::List(args) => {
                    put(&mut map, "filter", args.filter.as_deref());
                    put(&mut map, "text", args.text.as_deref());
                    put(&mut map, "status", args.status.as_deref());
                    put(&mut map, "assignee", args.assignee.as_deref());
                    put(&mut map, "project", args.project.as_deref());
                    put(&mut map, "label", args.label.as_deref());
                    if args.archived {
                        map.insert("archived".into(), Value::Bool(true));
                    }
                    if args.blocked {
                        map.insert("blocked".into(), Value::Bool(true));
                    }
                    "ticket list"
                }
                TicketCommand::Show(args) => {
                    put(&mut map, "id", args.id.as_deref());
                    "ticket show"
                }
                TicketCommand::Add(args) => {
                    put(&mut map, "title", args.title.as_deref());
                    put(&mut map, "status", args.status.as_deref());
                    put(&mut map, "assignee", args.assignee.as_deref());
                    put(&mut map, "project", args.project.as_deref());
                    put(&mut map, "label", args.label.as_deref());
                    put(&mut map, "origin", args.origin.as_deref());
                    let description = add_description_body(args.description.as_deref())?;
                    put(&mut map, "description", description.as_deref());
                    "ticket add"
                }
                TicketCommand::Set(args) => {
                    if args.description.is_some() && args.append_description.is_some() {
                        return Err(Failure::domain(DomainError::ConflictingDescriptionEdit));
                    }
                    put(&mut map, "id", args.id.as_deref());
                    put(&mut map, "title", args.title.as_deref());
                    put(&mut map, "status", args.status.as_deref());
                    put(&mut map, "assignee", args.assignee.as_deref());
                    if let Some(raw) = args.description.as_deref() {
                        map.insert("description".into(), Value::String(description_body(raw)?));
                    }
                    if let Some(raw) = args.append_description.as_deref() {
                        map.insert(
                            "appendDescription".into(),
                            Value::String(append_description_body(raw)?),
                        );
                    }
                    "ticket set"
                }
                TicketCommand::Move(args) => {
                    put(&mut map, "id", args.id.as_deref());
                    put(&mut map, "label", args.label.as_deref());
                    if let Some(at) = index(args.index.as_deref())? {
                        map.insert("index".into(), at);
                    }
                    "ticket move"
                }
                TicketCommand::Block(args) => {
                    put(&mut map, "id", args.id.as_deref());
                    put(&mut map, "on", args.on.as_deref());
                    "ticket block"
                }
                TicketCommand::Unblock(args) => {
                    put(&mut map, "id", args.id.as_deref());
                    put(&mut map, "on", args.on.as_deref());
                    "ticket unblock"
                }
                TicketCommand::Complete(args) => {
                    put(&mut map, "id", args.id.as_deref());
                    "ticket complete"
                }
                TicketCommand::Archive(args) => {
                    put(&mut map, "id", args.id.as_deref());
                    "ticket archive"
                }
                TicketCommand::Comment { command } => {
                    match command.as_ref().ok_or_else(Failure::usage)? {
                        CommentCommand::Add(args) => {
                            put(&mut map, "id", args.id.as_deref());
                            put(&mut map, "author", args.author.as_deref());
                            let body = match args.body.as_deref() {
                                Some("-") => read_standard_input()?,
                                Some(body) => body.to_string(),
                                None => String::new(),
                            };
                            map.insert("body".into(), Value::String(body));
                            "ticket comment add"
                        }
                        CommentCommand::List(args) => {
                            put(&mut map, "id", args.id.as_deref());
                            "ticket comment list"
                        }
                    }
                }
            }
        }
        Command::Label { command } => match command.as_ref().ok_or_else(Failure::usage)? {
            LabelCommand::List => "label list",
            LabelCommand::Add(args) => {
                put(&mut map, "name", args.name.as_deref());
                put(&mut map, "project", args.project.as_deref());
                "label add"
            }
            LabelCommand::Rename(args) => {
                put(&mut map, "from", args.from.as_deref());
                put(&mut map, "to", args.to.as_deref());
                put(&mut map, "project", args.project.as_deref());
                "label rename"
            }
            LabelCommand::Delete(args) => {
                put(&mut map, "name", args.name.as_deref());
                "label delete"
            }
            LabelCommand::Move(args) => {
                put(&mut map, "name", args.name.as_deref());
                if let Some(at) = index(args.index.as_deref())? {
                    map.insert("index".into(), at);
                }
                "label move"
            }
        },
        Command::Project { command } => match command.as_ref().ok_or_else(Failure::usage)? {
            ProjectCommand::List => "project list",
            ProjectCommand::Add(args) => {
                put(&mut map, "name", args.name.as_deref());
                put(&mut map, "abbr", args.abbr.as_deref());
                "project add"
            }
            ProjectCommand::Rename(args) => {
                put(&mut map, "from", args.from.as_deref());
                put(&mut map, "to", args.to.as_deref());
                put(&mut map, "abbr", args.abbr.as_deref());
                "project rename"
            }
            ProjectCommand::Delete(args) => {
                put(&mut map, "name", args.name.as_deref());
                "project delete"
            }
        },
        Command::Note { command } => match command.as_ref().ok_or_else(Failure::usage)? {
            NoteCommand::Rename(_) => "note rename",
            NoteCommand::Stats(_) => "note stats",
            NoteCommand::Links => "note links",
            NoteCommand::Write(args) => {
                put(&mut map, "path", args.path.as_deref());
                let text = match args.text.as_deref() {
                    None | Some("-") => read_standard_input()?,
                    Some(text) => text.to_string(),
                };
                map.insert("text".into(), Value::String(text));
                if let Some(hash) = args.expected_hash.as_deref() {
                    map.insert("expectedHash".into(), Value::String(hash.to_string()));
                }
                "note write"
            }
            NoteCommand::Move(args) => {
                put(&mut map, "path", args.path.as_deref());
                put(&mut map, "into", args.into.as_deref());
                "note move"
            }
            NoteCommand::Create(args) => {
                put(&mut map, "path", args.path.as_deref());
                "note create"
            }
            NoteCommand::CreateFolder(args) => {
                put(&mut map, "path", args.path.as_deref());
                "note create-folder"
            }
        },
    };
    Ok((verb.to_string(), map))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_is_never_sent_in_clear_off_loopback() {
        let refused = Target::parse("http://board.example:7700", Some("secret"));
        assert!(refused.is_err(), "http to a named host with a token");
        let message = refused
            .err()
            .map(|failure| failure.message)
            .unwrap_or_default();
        assert!(!message.contains("secret"), "the token is not echoed");
        assert!(Target::parse("https://board.example", Some("secret")).is_ok());
        assert!(Target::parse("http://127.0.0.1:7700", Some("secret")).is_ok());
        assert!(Target::parse("http://localhost:7700/", Some("secret")).is_ok());
        assert!(Target::parse("http://[::1]:7700", Some("secret")).is_ok());
        assert!(Target::parse("http://evil.example@127.0.0.1", Some("s")).is_err());
        assert!(Target::parse("ftp://x", None).is_err());
    }

    #[test]
    fn the_endpoint_is_the_base_plus_mcp() {
        let target = Target::parse("http://127.0.0.1:7700/", None).expect("parses");
        assert_eq!(target.url, "http://127.0.0.1:7700/mcp");
    }
}
