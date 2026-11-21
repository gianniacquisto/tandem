//! The bikeshed notes surface: the tree, the note pane, the editor and the
//! writes that move, create and save a note.

use super::board::ProjectOption;
use super::sheet::sheet_handle;
use super::*;
use crate::bikeshed::wikilink::LinkResolution;
use crate::domain::DomainError;

// MARK: - The bikeshed notes

/// `GET /notes/graph.json?project=<name>`: the vault as typed nodes and edges
/// for the graph view. The project value is matched against the board's names
/// and never builds a path. A failure answers a JSON error, never an empty graph.
pub(super) async fn note_graph(
    context: Context,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let project = field(&params, "project").trim().to_string();
    let scope = crate::bikeshed::GraphScope {
        project: Some(project).filter(|project| !project.is_empty()),
    };
    let graph = crate::rpc::vault::try_names(&context.board())
        .and_then(|names| context.bikeshed().graph(&names, &scope));
    let (status, body) = match graph {
        Ok(graph) => (
            StatusCode::OK,
            serde_json::to_string(&crate::wire::NoteGraphView::new(&graph))
                .unwrap_or_else(|_| "{}".to_string()),
        ),
        Err(error) => {
            let status = match error {
                DomainError::UnknownProjectName { .. } => StatusCode::BAD_REQUEST,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            tracing::warn!(error = ?error, "the notes graph could not be read");
            (
                status,
                serde_json::json!({ "error": error.message() }).to_string(),
            )
        }
    };
    (status, [(header::CONTENT_TYPE, "application/json")], body).into_response()
}

/// The `markdown.html` partial on its own: the notes preview's answer.
#[derive(Template)]
#[template(path = "markdown.html")]
struct MarkdownView {
    body: askama::filters::Safe<String>,
}

fn markdown_view(blocks: &[MarkdownBlock]) -> MarkdownView {
    MarkdownView {
        body: askama::filters::Safe(markdown::blocks(blocks)),
    }
}

struct TreeRowView {
    name: String,
    path: String,
    href: String,
    kind: &'static str,
    level: usize,
    indent: usize,
    folder: bool,
    selected: bool,
}

struct Crumb {
    label: String,
    current: bool,
    first: bool,
}

struct NotePane {
    path: String,
    text: String,
    baseline_hash: String,
    html: askama::filters::Safe<String>,
}

struct BacklinkRow {
    name: String,
    line: String,
    href: String,
}

struct LinkedTicket {
    reference: String,
    title: String,
    href: String,
}

/// The undo a delete's notice carries: the trash entry `/notes/restore` takes.
struct UndoView {
    trash: String,
}

struct ConflictView {
    path: String,
    mine: String,
    theirs: String,
    baseline_hash: String,
}

#[derive(Template)]
#[template(path = "notes.html")]
struct NotesView {
    rows: Vec<TreeRowView>,
    project_options: Vec<ProjectOption>,
    active_project: String,
    show_all: bool,
    path: Option<String>,
    crumbs: Vec<Crumb>,
    mode: &'static str,
    /// The folder the open note sits in: where New note files its page.
    current_dir: String,
    /// A just-created note puts the cursor in its title.
    focus_title: bool,
    note: Option<NotePane>,
    /// The lowercased targets that resolve broken at load, as JSON, for the editor.
    broken_links: String,
    backlinks: Vec<BacklinkRow>,
    backlink_count: usize,
    linked: Vec<LinkedTicket>,
    linked_count: usize,
    recoveries: Vec<String>,
    recovery_count: usize,
    notice: Option<String>,
    /// The undo a delete's notice offers; `None` for every other answer.
    undo: Option<UndoView>,
    error: Option<String>,
    conflict: Option<ConflictView>,
    focus_url: String,
    /// The open note's path and name, for the tab strip; empty when no note is open.
    current_path: String,
    current_name: String,
    /// Every note path in the vault, as JSON, so the strip can drop a stored
    /// tab whose note is gone without asking the focus-narrowed tree.
    all_notes: String,
    /// A rename or delete this response answers, so the strip can follow it.
    tab_from: String,
    tab_to: String,
    tab_gone: String,
}

/// The notes view's own URL: the focus, the show-all escape and one note.
fn notes_href(project: &str, show_all: bool, path: Option<&str>, mode: Option<&str>) -> String {
    let mut pairs: Vec<(&str, String)> = vec![("project", project.to_string())];
    if show_all {
        pairs.push(("show_all", "1".to_string()));
    }
    if let Some(path) = path {
        pairs.push(("path", path.to_string()));
    }
    if let Some(mode) = mode {
        pairs.push(("mode", mode.to_string()));
    }
    match serde_urlencoded::to_string(&pairs) {
        Ok(query) => format!("/notes?{query}"),
        Err(_) => "/notes".to_string(),
    }
}

/// The tree the focus draws: the `projects` folder narrowed to the one project
/// folder its name answers, case-insensitively. No focus, no matching folder
/// (ambiguous, renamed or absent) draws the whole vault, so a focus never hides
/// notes.
fn focus_nodes(nodes: Vec<BikeshedNode>, active: Option<&str>) -> Vec<BikeshedNode> {
    let Some(active) = active.filter(|active| !active.trim().is_empty()) else {
        return nodes;
    };
    let wanted = active.trim().to_lowercase();
    let mut out = nodes;
    for node in &mut out {
        if node.kind != NodeKind::Folder || node.path != "projects" {
            continue;
        }
        let narrowed: Vec<BikeshedNode> = node
            .children
            .iter()
            .filter(|child| child.kind == NodeKind::Folder && child.name.to_lowercase() == wanted)
            .cloned()
            .collect();
        if narrowed.len() == 1 {
            node.children = narrowed;
        }
    }
    out
}

fn note_paths(nodes: &[BikeshedNode], out: &mut Vec<String>) {
    for node in nodes {
        if node.kind == NodeKind::Folder {
            note_paths(&node.children, out);
        } else {
            out.push(node.path.clone());
        }
    }
}

fn note_name(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.strip_suffix(".md").unwrap_or(name).to_string()
}

fn flatten_tree(nodes: &[BikeshedNode], depth: usize, rows: &mut Vec<TreeRowView>) {
    for node in nodes {
        let folder = node.kind == NodeKind::Folder;
        rows.push(TreeRowView {
            name: note_name(&node.path),
            path: node.path.clone(),
            href: String::new(),
            kind: if folder { "folder" } else { "note" },
            level: depth + 1,
            indent: 8 + depth * 14,
            folder,
            selected: false,
        });
        if folder {
            flatten_tree(&node.children, depth + 1, rows);
        }
    }
}

fn recoverable_label(recovery: &crate::bikeshed::Recovery) -> String {
    match recovery.file.rsplit_once('/') {
        Some((folder, _)) if !folder.is_empty() => {
            format!("Unsaved draft kept from an interrupted save in {folder}")
        }
        _ => "Unsaved draft kept from an interrupted save".to_string(),
    }
}

fn render_notes(
    context: &Context,
    params: &HashMap<String, String>,
    notice: Option<String>,
    error: Option<String>,
    conflict: Option<ConflictView>,
) -> Result<NotesView, String> {
    let project = field(params, "project").trim().to_string();
    let show_all = !matches!(field(params, "show_all"), "" | "0" | "false");
    let path = params
        .get("path")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    // A note opens in the editor with its preview beside it; `mode=read` is
    // the page without the editor.
    let mode = if field(params, "mode") == "read" {
        "read"
    } else {
        "edit"
    };

    let board = context.board();
    let (projects, _) = board.projects_snapshot().map_err(|error| error.message())?;
    let project_options = {
        let mut options = vec![ProjectOption {
            value: String::new(),
            label: "All projects".to_string(),
            selected: project.is_empty(),
        }];
        for project in &projects {
            options.push(ProjectOption {
                value: project.name.clone(),
                label: project.name.clone(),
                selected: project.name == field(params, "project"),
            });
        }
        options
    };

    let vault = context.bikeshed();
    let tree = vault.tree();
    let mut every_note = Vec::new();
    note_paths(&tree, &mut every_note);
    let all_notes = serde_json::to_string(&every_note).unwrap_or_else(|_| "[]".to_string());
    let focused = focus_nodes(tree, if show_all { None } else { Some(&project) });
    let mut rows = Vec::new();
    flatten_tree(&focused, 0, &mut rows);
    for row in &mut rows {
        if row.folder {
            continue;
        }
        row.href = notes_href(&project, show_all, Some(&row.path), None);
        row.selected = path.as_deref() == Some(row.path.as_str());
    }

    let mut note = None;
    let mut crumbs = Vec::new();
    let mut backlinks = Vec::new();
    let mut linked = Vec::new();
    let mut broken_links = "[]".to_string();
    let mut render_error = error;

    if let Some(note_path) = &path {
        match vault.read(note_path) {
            Ok(read) => {
                let content = read.value;
                let folder = note_path
                    .rsplit_once('/')
                    .map(|(folder, _)| folder)
                    .unwrap_or("");
                let mut labels: Vec<String> = if folder.is_empty() {
                    Vec::new()
                } else {
                    folder.split('/').map(str::to_string).collect()
                };
                labels.push(note_name(note_path));
                let mut crumbs_out = vec![Crumb {
                    label: "Bikeshed".to_string(),
                    current: labels.is_empty(),
                    first: true,
                }];
                for (index, label) in labels.iter().enumerate() {
                    crumbs_out.push(Crumb {
                        label: label.clone(),
                        current: index + 1 == labels.len(),
                        first: false,
                    });
                }
                crumbs = crumbs_out;
                // One resolver (ADR 0005): the bikeshed resolves every link in
                // the text, and the renderer looks each target up here.
                let occurrences = vault.links(&content.text);
                let resolved: HashMap<String, LinkResolution> = occurrences
                    .iter()
                    .map(|occurrence| {
                        (
                            occurrence.link.target.clone(),
                            occurrence.resolution.clone(),
                        )
                    })
                    .collect();
                let mut broken: Vec<String> = occurrences
                    .iter()
                    .filter(|occurrence| occurrence.resolution == LinkResolution::Broken)
                    .map(|occurrence| occurrence.link.target.to_lowercase())
                    .filter(|target| {
                        let target = target.trim();
                        !target.starts_with("project:") && !target.starts_with("label:")
                    })
                    .collect();
                broken.sort();
                broken.dedup();
                broken_links = serde_json::to_string(&broken).unwrap_or_else(|_| "[]".to_string());
                let resolve = |target: &str| {
                    resolved
                        .get(target)
                        .cloned()
                        .unwrap_or_else(|| vault.resolve(target))
                };
                let href = |path: &str| notes_href(&project, show_all, Some(path), None);
                let html = markdown::document_with_links(
                    &TicketMarkdown::blocks(Some(&content.text)),
                    &markdown::Wikilinks {
                        resolve: &resolve,
                        href: &href,
                    },
                );
                note = Some(NotePane {
                    path: note_path.clone(),
                    text: content.text.clone(),
                    baseline_hash: content.hash,
                    html: askama::filters::Safe(html),
                });
                match vault.backlinks(note_path) {
                    Ok(read) => {
                        backlinks = read
                            .value
                            .into_iter()
                            .map(|backlink| BacklinkRow {
                                name: note_name(&backlink.path),
                                href: notes_href(&project, show_all, Some(&backlink.path), None),
                                line: backlink.line,
                            })
                            .collect();
                    }
                    Err(error) => {
                        render_error.get_or_insert(error.message());
                    }
                }
                match board.list_snapshot(&TicketQuery::default()) {
                    Ok((tickets, _)) => {
                        linked = tickets
                            .into_iter()
                            .filter(|ticket| {
                                !ticket.archived
                                    && ticket.link.as_deref() == Some(note_path.as_str())
                            })
                            .map(|ticket| LinkedTicket {
                                reference: sheet_handle(&ticket),
                                title: ticket.title,
                                href: format!("/tickets/{}", ticket.id),
                            })
                            .collect();
                    }
                    Err(error) => {
                        render_error.get_or_insert(error.message());
                    }
                }
            }
            Err(error) => {
                render_error.get_or_insert(error.message());
            }
        }
    }

    let current_path = note
        .as_ref()
        .map_or(String::new(), |pane| pane.path.clone());
    let current_name = note
        .as_ref()
        .map_or(String::new(), |pane| note_name(&pane.path));

    let recoveries: Vec<String> = vault.recoveries().iter().map(recoverable_label).collect();

    Ok(NotesView {
        rows,
        project_options,
        active_project: field(params, "project").to_string(),
        show_all,
        path: path.clone(),
        crumbs,
        mode,
        current_dir: path
            .as_deref()
            .and_then(|open| open.rsplit_once('/'))
            .map_or(String::new(), |(folder, _)| folder.to_string()),
        focus_title: field(params, "rename") == "1",
        note,
        broken_links,
        backlink_count: backlinks.len(),
        backlinks,
        linked_count: linked.len(),
        linked,
        recovery_count: recoveries.len(),
        recoveries,
        notice,
        undo: Some(field(params, "undo"))
            .filter(|trash| !trash.is_empty())
            .map(|trash| UndoView {
                trash: trash.to_string(),
            }),
        error: render_error,
        conflict,
        focus_url: notes_href(&project, show_all, path.as_deref(), None),
        current_path,
        current_name,
        all_notes,
        tab_from: field(params, "tab_from").to_string(),
        tab_to: field(params, "tab_to").to_string(),
        tab_gone: field(params, "tab_gone").to_string(),
    })
}

fn notes_response(
    context: &Context,
    params: &HashMap<String, String>,
    notice: Option<String>,
    error: Option<String>,
    conflict: Option<ConflictView>,
    headers: &HeaderMap,
) -> Response {
    if let Some(refusal) = notice.as_ref().or(error.as_ref()) {
        tracing::warn!(error = ?refusal, "notes refused");
    }
    let view = match render_notes(context, params, notice, error, conflict) {
        Ok(view) => view,
        Err(error) => {
            tracing::error!(error = ?error, "the notes could not be read");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("the notes could not be read: {error}\n"),
            )
                .into_response();
        }
    };
    let body = match view.render() {
        Ok(body) => body,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("the notes could not be drawn: {error}\n"),
            )
                .into_response();
        }
    };
    if is_partial(headers) {
        return html_response(body, StatusCode::OK);
    }
    match (PageView {
        assets: assets::urls(),
        body: askama::filters::Safe(body),
        stream: true,
        sign_out: context.access.is_token(),
        export: context.may_export(),
        tokens: context.shows_tokens(),
        people: context.shows_people(),
    })
    .render()
    {
        Ok(html) => html_response(html, StatusCode::OK),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("the shell could not be drawn: {error}\n"),
        )
            .into_response(),
    }
}

pub(super) async fn notes_page(
    context: Context,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    notes_response(&context, &params, None, None, None, &headers)
}

pub(super) async fn note_view(
    context: Context,
    Query(mut params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    params.insert("mode".to_string(), "read".to_string());
    notes_response(&context, &params, None, None, None, &headers)
}

/// Follow a wikilink the editor drew: resolve `target` through the bikeshed and
/// open the note it names, else stay on `from` with the banner. The target
/// never builds a path; only a path the bikeshed resolved is opened.
pub(super) async fn note_open(
    context: Context,
    Query(mut params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    let target = field(&params, "target").to_string();
    let from = field(&params, "from").to_string();
    params.remove("path");
    match context.bikeshed().resolve(&target) {
        LinkResolution::Resolved(path) => {
            params.insert("path".to_string(), path);
            params.insert("mode".to_string(), "edit".to_string());
            notes_response(&context, &params, None, None, None, &headers)
        }
        LinkResolution::Broken | LinkResolution::Project(_) | LinkResolution::Label(_) => {
            if !from.is_empty() {
                params.insert("path".to_string(), from);
            }
            params.insert("mode".to_string(), "edit".to_string());
            let banner =
                format!("No single note named {target}: none exists, or two notes share the name.");
            notes_response(&context, &params, None, Some(banner), None, &headers)
        }
    }
}

pub(super) async fn note_edit(
    context: Context,
    Query(mut params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    params.insert("mode".to_string(), "edit".to_string());
    notes_response(&context, &params, None, None, None, &headers)
}

pub(super) async fn note_preview(
    _context: Context,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    let Some(params) = form_params(form) else {
        return bad_request("the form could not be read");
    };
    let blocks = TicketMarkdown::blocks(Some(field(&params, "text")));
    match markdown_view(&blocks).render() {
        Ok(html) => html_response(html, StatusCode::OK),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("the preview could not be drawn: {error}\n"),
        )
            .into_response(),
    }
}

pub(super) async fn note_save(
    context: Context,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    let Some(params) = form_params(form) else {
        return bad_request("the form could not be read");
    };
    let path = field(&params, "path").to_string();
    let text = field(&params, "text");
    let baseline = field(&params, "baseline_hash");
    let resolution = field(&params, "resolution");
    let vault = context.bikeshed();

    if resolution == "take_theirs" {
        return notes_response(&context, &params, None, None, None, &headers);
    }
    // The core compares the baseline with the disk inside the vault lock, in
    // the same critical section as the write; "keep mine" skips the check.
    let expected = (resolution != "keep_mine").then_some(baseline);
    match vault.write(&path, text, expected) {
        Ok(_) => {
            tracing::info!(
                actor = ?context.actor.as_str(),
                path = ?path,
                revision = context.revision(),
                "note saved"
            );
            notes_response(&context, &params, None, None, None, &headers)
        }
        Err(crate::domain::DomainError::StaleNote(_)) => {
            let conflict = vault.read(&path).ok().map(|read| ConflictView {
                path: path.clone(),
                mine: text.to_string(),
                theirs: read.value.text.clone(),
                baseline_hash: read.value.hash.clone(),
            });
            match conflict {
                Some(conflict) => {
                    notes_response(&context, &params, None, None, Some(conflict), &headers)
                }
                None => notes_response(
                    &context,
                    &params,
                    None,
                    Some(crate::domain::DomainError::UnknownNote(path.clone()).message()),
                    None,
                    &headers,
                ),
            }
        }
        Err(error) => notes_response(
            &context,
            &params,
            None,
            Some(error.message()),
            None,
            &headers,
        ),
    }
}

pub(super) async fn note_create(
    context: Context,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    note_create_thing(&context, headers, form, false).await
}

pub(super) async fn note_create_folder(
    context: Context,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    note_create_thing(&context, headers, form, true).await
}

async fn note_create_thing(
    context: &Context,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
    folder: bool,
) -> Response {
    let Some(mut params) = form_params(form) else {
        return bad_request("the form could not be read");
    };
    let path = field(&params, "path").to_string();
    let vault = context.bikeshed();
    let outcome = if folder {
        vault.create_folder(&path)
    } else {
        vault.create(&path)
    };
    match outcome {
        Ok(read) => {
            tracing::info!(
                actor = ?context.actor.as_str(),
                path = ?path,
                folder,
                revision = context.revision(),
                "note created"
            );
            if !folder {
                params.insert("path".to_string(), read.value);
            }
            notes_response(context, &params, None, None, None, &headers)
        }
        Err(error) => notes_response(
            context,
            &params,
            None,
            Some(error.message()),
            None,
            &headers,
        ),
    }
}

/// A well-formed `YYYY-MM-DD`: the only shape a daily note's name may take, so a
/// client-supplied date can never steer the path.
fn is_iso_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 10
        && bytes.iter().enumerate().all(|(index, byte)| match index {
            4 | 7 => *byte == b'-',
            _ => byte.is_ascii_digit(),
        })
}

/// Today in UTC, the fallback when the browser sends no date of its own.
fn utc_today() -> String {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let days = i64::try_from(seconds / 86_400).unwrap_or(0);
    let (year, month, day) = super::sheet::civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// The daily note: today's page under `daily/`, created on first use and opened
/// on every use after. The browser names its own local date.
pub(super) async fn note_daily(
    context: Context,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    let Some(mut params) = form_params(form) else {
        return bad_request("the form could not be read");
    };
    let sent = field(&params, "date").to_string();
    let date = if is_iso_date(&sent) {
        sent
    } else {
        utc_today()
    };
    let path = format!("daily/{date}.md");
    let vault = context.bikeshed();
    // An existing page is the common case and not an error: open it.
    let opened = match vault.create(&path) {
        Ok(read) => read.value,
        Err(_) => path,
    };
    tracing::info!(
        actor = ?context.actor.as_str(),
        path = ?opened,
        revision = context.revision(),
        "daily note opened"
    );
    params.insert("path".to_string(), opened);
    notes_response(&context, &params, None, None, None, &headers)
}

/// Rename a note in place: the new name keeps the folder and the `.md`
/// extension, and the core rewrites every link that pointed at the old path.
pub(super) async fn note_rename(
    context: Context,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    let Some(mut params) = form_params(form) else {
        return bad_request("the form could not be read");
    };
    let path = field(&params, "path").to_string();
    let name = field(&params, "name")
        .trim()
        .trim_end_matches(".md")
        .to_string();
    let folder = path.rsplit_once('/').map_or("", |(folder, _)| folder);
    let target = if folder.is_empty() {
        format!("{name}.md")
    } else {
        format!("{folder}/{name}.md")
    };
    if target == path {
        params.insert("path".to_string(), field(&params, "open").to_string());
        return notes_response(&context, &params, None, None, None, &headers);
    }
    match context.bikeshed().rename(&path, &target) {
        Ok(read) => {
            tracing::info!(
                actor = ?context.actor.as_str(),
                from = ?path,
                to = ?read.value,
                revision = context.revision(),
                "note renamed"
            );
            // The tab strip follows the rename whichever row it was.
            params.insert("tab_from".to_string(), path.clone());
            params.insert("tab_to".to_string(), read.value.clone());
            // The open page follows the rename; a rename of another row leaves it.
            let followed = field(&params, "open") == path;
            if followed {
                params.insert("path".to_string(), read.value);
            } else {
                params.insert("path".to_string(), field(&params, "open").to_string());
            }
            let mut response = notes_response(&context, &params, None, None, None, &headers);
            // The address bar still names the old path; only the server knows
            // the final one, so a reload after a rename lands on the note.
            if followed {
                let url = notes_href(
                    field(&params, "project"),
                    !matches!(field(&params, "show_all"), "" | "0" | "false"),
                    Some(field(&params, "path")),
                    Some(field(&params, "mode")).filter(|mode| !mode.is_empty()),
                );
                if let Ok(value) = HeaderValue::from_str(&url) {
                    response.headers_mut().insert("hx-replace-url", value);
                }
            }
            response
        }
        Err(error) => {
            params.insert("path".to_string(), field(&params, "open").to_string());
            notes_response(
                &context,
                &params,
                None,
                Some(error.message()),
                None,
                &headers,
            )
        }
    }
}

/// Delete a note, or a folder with everything in it: the vault's trash holds it,
/// and the notice offers the undo. The page closes when it was the one deleted
/// or sat inside the deleted folder.
pub(super) async fn note_delete(
    context: Context,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    let Some(mut params) = form_params(form) else {
        return bad_request("the form could not be read");
    };
    let path = field(&params, "path").to_string();
    let open = field(&params, "open").to_string();
    let mut notice = None;
    let error = match context.bikeshed().delete(&path) {
        Ok(read) => {
            tracing::info!(
                actor = ?context.actor.as_str(),
                path = ?path,
                revision = context.revision(),
                "note deleted"
            );
            params.insert("tab_gone".to_string(), path.clone());
            params.insert("undo".to_string(), read.value.id);
            notice = Some(format!("Deleted {path}."));
            None
        }
        Err(error) => Some(error.message()),
    };
    let gone = open == path || open.starts_with(&format!("{path}/"));
    if gone && error.is_none() {
        params.remove("path");
    } else {
        params.insert("path".to_string(), open);
    }
    notes_response(&context, &params, notice, error, None, &headers)
}

/// Undo a delete: move the trash entry back and open the note it returns. The
/// entry id is validated by the core, so a crafted one is refused on the page.
pub(super) async fn note_restore(
    context: Context,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    let Some(mut params) = form_params(form) else {
        return bad_request("the form could not be read");
    };
    let trash = field(&params, "trash").to_string();
    match context.bikeshed().restore(&trash) {
        Ok(read) => {
            tracing::info!(
                actor = ?context.actor.as_str(),
                path = ?read.value,
                revision = context.revision(),
                "note restored"
            );
            params.insert("path".to_string(), read.value.clone());
            let notice = format!("Restored {}.", read.value);
            notes_response(&context, &params, Some(notice), None, None, &headers)
        }
        Err(error) => {
            params.remove("path");
            notes_response(
                &context,
                &params,
                None,
                Some(error.message()),
                None,
                &headers,
            )
        }
    }
}

/// New note: an empty `Untitled` page in the folder the human is in, with the
/// cursor in its title so the first act is naming it.
pub(super) async fn note_new(
    context: Context,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    let Some(mut params) = form_params(form) else {
        return bad_request("the form could not be read");
    };
    let folder = field(&params, "dir").trim().trim_matches('/').to_string();
    let vault = context.bikeshed();
    let mut created = None;
    for number in 1..100 {
        let name = if number == 1 {
            "Untitled".to_string()
        } else {
            format!("Untitled {number}")
        };
        let path = if folder.is_empty() {
            format!("{name}.md")
        } else {
            format!("{folder}/{name}.md")
        };
        if let Ok(read) = vault.create(&path) {
            created = Some(read.value);
            break;
        }
    }
    match created {
        Some(path) => {
            tracing::info!(
                actor = ?context.actor.as_str(),
                path = ?path,
                revision = context.revision(),
                "note created"
            );
            params.insert("path".to_string(), path);
            params.insert("rename".to_string(), "1".to_string());
            params.insert("mode".to_string(), "edit".to_string());
            notes_response(&context, &params, None, None, None, &headers)
        }
        None => notes_response(
            &context,
            &params,
            None,
            Some("Could not create a new note here.".to_string()),
            None,
            &headers,
        ),
    }
}

pub(super) async fn note_move(
    context: Context,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    let Some(mut params) = form_params(form) else {
        return bad_request("the form could not be read");
    };
    let path = field(&params, "path").to_string();
    let into = field(&params, "into").to_string();
    let vault = context.bikeshed();
    match vault.move_into(&path, &into) {
        Ok(read) => {
            tracing::info!(
                actor = ?context.actor.as_str(),
                from = ?path,
                into = ?into,
                revision = context.revision(),
                "note moved"
            );
            if !read.value.is_folder {
                params.insert("path".to_string(), read.value.path);
            } else {
                params.remove("path");
            }
            notes_response(&context, &params, None, None, None, &headers)
        }
        Err(error) => notes_response(
            &context,
            &params,
            None,
            Some(error.message()),
            None,
            &headers,
        ),
    }
}
