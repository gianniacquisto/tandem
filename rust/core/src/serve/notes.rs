//! The bikeshed notes surface: the tree, the note pane, the editor and the
//! writes that move, create and save a note.

use super::board::ProjectOption;
use super::sheet::sheet_handle;
use super::*;

// MARK: - The bikeshed notes

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
    note: Option<NotePane>,
    backlinks: Vec<BacklinkRow>,
    backlink_count: usize,
    linked: Vec<LinkedTicket>,
    linked_count: usize,
    recoveries: Vec<String>,
    recovery_count: usize,
    notice: Option<String>,
    error: Option<String>,
    conflict: Option<ConflictView>,
    focus_url: String,
    edit_href: String,
    read_href: String,
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
    context: &SessionContext,
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
    let mode = if field(params, "mode") == "edit" {
        "edit"
    } else {
        "read"
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
                note = Some(NotePane {
                    path: note_path.clone(),
                    text: content.text.clone(),
                    baseline_hash: content.hash,
                    html: askama::filters::Safe(markdown::document(&TicketMarkdown::blocks(Some(
                        &content.text,
                    )))),
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

    let recoveries: Vec<String> = vault.recoveries().iter().map(recoverable_label).collect();

    Ok(NotesView {
        rows,
        project_options,
        active_project: field(params, "project").to_string(),
        show_all,
        path: path.clone(),
        crumbs,
        mode,
        note,
        backlink_count: backlinks.len(),
        backlinks,
        linked_count: linked.len(),
        linked,
        recovery_count: recoveries.len(),
        recoveries,
        notice,
        error: render_error,
        conflict,
        focus_url: notes_href(&project, show_all, path.as_deref(), None),
        edit_href: notes_href(&project, show_all, path.as_deref(), Some("edit")),
        read_href: notes_href(&project, show_all, path.as_deref(), None),
    })
}

fn notes_response(
    context: &SessionContext,
    params: &HashMap<String, String>,
    notice: Option<String>,
    error: Option<String>,
    conflict: Option<ConflictView>,
    headers: &HeaderMap,
) -> Response {
    let view = match render_notes(context, params, notice, error, conflict) {
        Ok(view) => view,
        Err(error) => {
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
        body: askama::filters::Safe(body),
        stream: true,
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
    State(context): State<Arc<SessionContext>>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    notes_response(&context, &params, None, None, None, &headers)
}

pub(super) async fn note_view(
    State(context): State<Arc<SessionContext>>,
    Query(mut params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    params.remove("mode");
    notes_response(&context, &params, None, None, None, &headers)
}

pub(super) async fn note_edit(
    State(context): State<Arc<SessionContext>>,
    Query(mut params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    params.insert("mode".to_string(), "edit".to_string());
    notes_response(&context, &params, None, None, None, &headers)
}

pub(super) async fn note_preview(
    State(_context): State<Arc<SessionContext>>,
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
    State(context): State<Arc<SessionContext>>,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    if let Some(denied) = guard(&headers) {
        return denied;
    }
    let Some(params) = form_params(form) else {
        return bad_request("the form could not be read");
    };
    let path = field(&params, "path").to_string();
    let text = field(&params, "text");
    let baseline = field(&params, "baseline_hash");
    let resolution = field(&params, "resolution");
    let vault = context.bikeshed();

    let disk = match vault.read(&path) {
        Ok(read) => read.value,
        Err(error) => {
            return notes_response(
                &context,
                &params,
                None,
                Some(error.message()),
                None,
                &headers,
            );
        }
    };
    if disk.hash != baseline && resolution != "keep_mine" {
        if resolution == "take_theirs" {
            return notes_response(&context, &params, None, None, None, &headers);
        }
        let conflict = ConflictView {
            path: path.clone(),
            mine: text.to_string(),
            theirs: disk.text.clone(),
            baseline_hash: disk.hash.clone(),
        };
        return notes_response(&context, &params, None, None, Some(conflict), &headers);
    }
    match vault.write(&path, text) {
        Ok(_) => notes_response(&context, &params, None, None, None, &headers),
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
    State(context): State<Arc<SessionContext>>,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    note_create_thing(&context, headers, form, false).await
}

pub(super) async fn note_create_folder(
    State(context): State<Arc<SessionContext>>,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    note_create_thing(&context, headers, form, true).await
}

async fn note_create_thing(
    context: &SessionContext,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
    folder: bool,
) -> Response {
    if let Some(denied) = guard(&headers) {
        return denied;
    }
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

pub(super) async fn note_move(
    State(context): State<Arc<SessionContext>>,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    if let Some(denied) = guard(&headers) {
        return denied;
    }
    let Some(mut params) = form_params(form) else {
        return bad_request("the form could not be read");
    };
    let path = field(&params, "path").to_string();
    let into = field(&params, "into").to_string();
    let vault = context.bikeshed();
    match vault.move_into(&path, &into) {
        Ok(read) => {
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
