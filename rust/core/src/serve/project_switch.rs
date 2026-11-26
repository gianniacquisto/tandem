//! The project switcher the shell draws above the Board and Notes faces
//! (TAN-530), and the rules that pick the project a page opens on.
//!
//! One project is open at a time. The URL (`?project=<name>`) is the source of
//! truth; a `tandem_<port>_last` cookie remembers the last project and face, so
//! a reopened browser lands where it was left. The cookie is read only to
//! choose among the board's own project names: a value naming a project the
//! board no longer holds is ignored, and no value ever builds a path.

use axum::http::{HeaderMap, HeaderValue, header};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use super::board::ProjectOption;
use super::{Context, StatusCode, ui_state};

/// The two faces of a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Face {
    Board,
    Notes,
}

impl Face {
    /// The face's route.
    pub(super) fn path(self) -> &'static str {
        match self {
            Face::Board => "/board",
            Face::Notes => "/notes",
        }
    }
}

/// What the `last` cookie holds.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
struct Last {
    project: String,
    face: Face,
}

/// What the shell draws: the select, the form it submits and the two tabs.
pub(super) struct Switcher {
    /// The face's own route the select submits to.
    pub(super) action: String,
    pub(super) options: Vec<ProjectOption>,
    /// The fields the face keeps across a project change.
    pub(super) hidden: Vec<(String, String)>,
    pub(super) board_href: String,
    pub(super) notes_href: String,
    pub(super) face: Face,
    /// There is no project to choose: the select is drawn disabled.
    pub(super) disabled: bool,
    /// The selected project's name, empty when none is.
    pub(super) selected: String,
}

impl Switcher {
    pub(super) fn board_current(&self) -> bool {
        self.face == Face::Board
    }

    pub(super) fn notes_current(&self) -> bool {
        self.face == Face::Notes
    }
}

/// The switcher for `face` with `project` selected. `None` when the board's
/// projects cannot be read: the page then draws without one.
pub(super) fn switcher(
    context: &Context,
    face: Face,
    project: &str,
    action: &str,
    hidden: Vec<(String, String)>,
) -> Option<Switcher> {
    let (projects, _) = context.board().projects_snapshot().ok()?;
    let known = projects.iter().any(|candidate| candidate.name == project);
    let selected = if known {
        project.to_string()
    } else {
        String::new()
    };
    let mut options: Vec<ProjectOption> = Vec::new();
    if projects.is_empty() {
        options.push(ProjectOption {
            value: String::new(),
            label: "No projects".to_string(),
            selected: true,
        });
    } else if !known {
        // A page under no project (a stray link) says so, rather than the
        // browser showing the first project as if it were chosen.
        options.push(ProjectOption {
            value: String::new(),
            label: "Choose a project".to_string(),
            selected: true,
        });
    }
    options.extend(projects.iter().map(|candidate| ProjectOption {
        value: candidate.name.clone(),
        label: candidate.name.clone(),
        selected: candidate.name == selected,
    }));
    Some(Switcher {
        action: action.to_string(),
        options,
        hidden,
        board_href: face_href(Face::Board, &selected),
        notes_href: face_href(Face::Notes, &selected),
        face,
        disabled: projects.is_empty(),
        selected,
    })
}

/// A face's route, carrying the project when one is selected.
pub(super) fn face_href(face: Face, project: &str) -> String {
    if project.is_empty() {
        return face.path().to_string();
    }
    redirect_target(face.path(), project, &[])
}

/// `path?project=<name>` plus the other pairs, `show_all` (retired) and any
/// earlier `project` dropped.
fn redirect_target(path: &str, project: &str, rest: &[(String, String)]) -> String {
    let mut pairs: Vec<(&str, &str)> = vec![("project", project)];
    pairs.extend(
        rest.iter()
            .filter(|(key, _)| key != "project" && key != "show_all")
            .map(|(key, value)| (key.as_str(), value.as_str())),
    );
    match serde_urlencoded::to_string(&pairs) {
        Ok(query) => format!("{path}?{query}"),
        Err(_) => path.to_string(),
    }
}

/// Where a request for a project stands.
pub(super) enum Resolved {
    /// The URL names a project the board holds.
    Known(String),
    /// The URL names none, or an unknown one: the project to go to instead,
    /// the cookie's when it is still a project, else the first.
    Fallback(String),
    /// The board holds no project, so there is nothing to choose.
    NoProjects,
}

/// Picks the project for `requested` (the URL's value, trimmed).
pub(super) fn resolve(context: &Context, headers: &HeaderMap, requested: &str) -> Resolved {
    let Ok((projects, _)) = context.board().projects_snapshot() else {
        return Resolved::NoProjects;
    };
    if projects.is_empty() {
        return Resolved::NoProjects;
    }
    if projects.iter().any(|project| project.name == requested) {
        return Resolved::Known(requested.to_string());
    }
    let remembered = last(context, headers)
        .map(|last| last.project)
        .filter(|name| projects.iter().any(|project| &project.name == name));
    Resolved::Fallback(remembered.unwrap_or_else(|| projects[0].name.clone()))
}

fn last(context: &Context, headers: &HeaderMap) -> Option<Last> {
    ui_state::read(headers, &context.ui_cookie(), "last")
}

/// The face the cookie remembers, for `/`; the board without a usable cookie
/// (one that names a project the board no longer holds is not usable).
pub(super) fn last_face(context: &Context, headers: &HeaderMap) -> Face {
    let Some(last) = last(context, headers) else {
        return Face::Board;
    };
    let known = context
        .board()
        .projects_snapshot()
        .is_ok_and(|(projects, _)| projects.iter().any(|project| project.name == last.project));
    if known { last.face } else { Face::Board }
}

/// A 303 to `path` scoped to `project`, keeping the other pairs. Never cached:
/// the answer depends on the cookie.
pub(super) fn redirect(path: &str, project: &str, rest: &[(String, String)]) -> Response {
    let location = redirect_target(path, project, rest);
    let mut response = StatusCode::SEE_OTHER.into_response();
    if let Ok(value) = HeaderValue::from_str(&location) {
        response.headers_mut().insert(header::LOCATION, value);
    }
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

/// The `Set-Cookie` line that remembers the switcher's project and face: a
/// year, `Path=/`, `SameSite=Lax`, `HttpOnly` because only the server reads
/// it. `None` when no project is selected.
pub(super) fn last_cookie(context: &Context, switcher: Option<&Switcher>) -> Option<HeaderValue> {
    let switcher = switcher.filter(|switcher| !switcher.selected.is_empty())?;
    let json = serde_json::to_string(&Last {
        project: switcher.selected.clone(),
        face: switcher.face,
    })
    .ok()?;
    let encoded = ui_state::percent_encode(&json);
    if encoded.len() > ui_state::MAX_ENCODED {
        return None;
    }
    HeaderValue::from_str(&format!(
        "{}last={encoded}; Path=/; Max-Age=31536000; SameSite=Lax; HttpOnly",
        context.ui_cookie()
    ))
    .ok()
}

/// Adds the cookie to a response.
pub(super) fn stamp(response: &mut Response, cookie: Option<HeaderValue>) {
    if let Some(cookie) = cookie {
        response.headers_mut().append(header::SET_COOKIE, cookie);
    }
}
