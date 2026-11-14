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
    /// The folder the open note sits in: where New note files its page.
    current_dir: String,
    /// A just-created note puts the cursor in its title.
    focus_title: bool,
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
        current_dir: path
            .as_deref()
            .and_then(|open| open.rsplit_once('/'))
            .map_or(String::new(), |(folder, _)| folder.to_string()),
        focus_title: field(params, "rename") == "1",
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
        read_href: notes_href(&project, show_all, path.as_deref(), Some("read")),
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
    context: &SessionContext,
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
            // The open page follows the rename; a rename of another row leaves it.
            if field(&params, "open") == path {
                params.insert("path".to_string(), read.value);
            } else {
                params.insert("path".to_string(), field(&params, "open").to_string());
            }
            notes_response(&context, &params, None, None, None, &headers)
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

/// Delete a note, or a folder with everything in it. The page closes when it
/// was the one deleted or sat inside the deleted folder.
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
    let outcome = context.bikeshed().delete(&path);
    let error = outcome.err().map(|error| error.message());
    let gone = open == path || open.starts_with(&format!("{path}/"));
    if gone && error.is_none() {
        params.remove("path");
    } else {
        params.insert("path".to_string(), open);
    }
    notes_response(&context, &params, None, error, None, &headers)
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
