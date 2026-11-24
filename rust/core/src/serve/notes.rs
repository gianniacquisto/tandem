//! The notes surface: the tree, the note pane, the editor and the
//! writes that move, create and save a note.

use super::board::ProjectOption;
use super::sheet::sheet_handle;
use super::*;
use crate::domain::DomainError;
use crate::domain::ticket::Status;
use crate::notes::wikilink::{BoardNames, ConceptKind, LinkResolution, typed_target};

// MARK: - The notes page

/// `GET /notes/graph.json?project=<name>`: the vault as typed nodes and edges
/// for the graph view. The project value is matched against the board's names
/// and never builds a path. A failure answers a JSON error, never an empty graph.
pub(super) async fn note_graph(
    context: Context,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let project = field(&params, "project").trim().to_string();
    let scope = crate::notes::GraphScope {
        project: Some(project).filter(|project| !project.is_empty()),
    };
    let graph = crate::rpc::vault::try_names(&context.board())
        .and_then(|names| context.notes().graph(&names, &scope));
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

/// What a `[[` link can point at: the board's projects, its labels with their
/// project, and every note with the shortest target that resolves to it. Writes
/// nothing; the board's names and the vault's paths are joined here, as
/// `note_health` joins them.
pub(super) async fn note_targets(context: Context) -> Response {
    let board = context.board();
    let read = board.projects_snapshot().map(|(projects, _)| projects);
    let (status, body) = match read {
        Ok(projects) => {
            let by_name = |a: &str, b: &str| a.to_lowercase().cmp(&b.to_lowercase());
            let mut project_rows: Vec<&str> = projects.iter().map(|p| p.name.as_str()).collect();
            project_rows.sort_by(|a, b| by_name(a, b));
            let mut label_rows: Vec<(&str, &str)> = projects
                .iter()
                .flat_map(|p| p.labels.iter().map(|l| (l.as_str(), p.name.as_str())))
                .collect();
            label_rows.sort_by(|a, b| by_name(a.0, b.0));
            let mut note_rows = context.notes().link_targets();
            note_rows.sort_by(|a, b| by_name(&a.0, &b.0));
            let body = serde_json::json!({
                "projects": project_rows.iter().map(|name| serde_json::json!({ "name": name })).collect::<Vec<_>>(),
                "labels": label_rows.iter().map(|(name, project)| serde_json::json!({ "name": name, "project": project })).collect::<Vec<_>>(),
                "notes": note_rows.iter().map(|(path, target)| serde_json::json!({ "path": path, "target": target })).collect::<Vec<_>>(),
            });
            (StatusCode::OK, body.to_string())
        }
        Err(error) => {
            tracing::warn!(error = ?error, "the link targets could not be read");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
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
    /// A folder drawn open (its twisty down, `aria-expanded` true).
    expanded: bool,
    /// A row under a shut folder, drawn `hidden` by the server (TAN-880).
    hidden: bool,
    /// `project`, `label` or `no label` on a folder the board names; else empty.
    tag: &'static str,
    /// The tag is a warning: a project subfolder no label of the project names.
    tag_warn: bool,
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

/// One row of a Relations section on a concept page, or of a note's Links to.
struct RelRow {
    name: String,
    detail: String,
    /// `None` is a row with nothing to open.
    href: Option<String>,
    /// A notes-face link swaps the pane; a ticket link leaves it.
    swap: bool,
}

/// A collapsible Relations section drawn from a list.
struct RelSection {
    key: &'static str,
    title: &'static str,
    open: bool,
    empty: &'static str,
    rows: Vec<RelRow>,
}

impl RelSection {
    fn new(
        key: &'static str,
        title: &'static str,
        open: bool,
        empty: &'static str,
        rows: Vec<RelRow>,
    ) -> Self {
        Self {
            open: open && !rows.is_empty(),
            key,
            title,
            empty,
            rows,
        }
    }
}

/// The label page's main column: the label's name and its one summary line.
struct LabelPage {
    name: String,
    summary: String,
}

/// The project or label a concept page is about, as the board spells it.
struct Concept {
    kind: ConceptKind,
    name: String,
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

/// What the graph view's container carries for `graph.js`: where the JSON
/// lives and the URL prefixes a node click follows. The browser appends the
/// node's own path or name, so the focus params ride along.
struct GraphPane {
    src: String,
    note_href: String,
    concept_href: String,
}

#[derive(Template)]
#[template(path = "notes.html")]
struct NotesView {
    /// `Some` draws the graph in the main column, `None` the note view.
    graph: Option<GraphPane>,
    /// The Note | Graph switch's two links, both keeping the focus params.
    note_switch_href: String,
    graph_switch_href: String,
    /// Where the focus picker's form goes: `/notes` or `/notes/graph`.
    focus_action: &'static str,
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
    /// The sections a concept page draws, and a note's Links to.
    sections: Vec<RelSection>,
    /// A concept page draws its own sections where a note draws Backlinks and
    /// Tickets.
    note_sections: bool,
    /// The project or label page's kind and name, carried through the editor's
    /// forms so a save keeps the page.
    concept_kind: String,
    concept_name: String,
    label_page: Option<LabelPage>,
    /// Every row the Relations pane lists, for the toggle's count.
    relations_count: usize,
    recoveries: Vec<String>,
    recovery_count: usize,
    notice: Option<String>,
    /// The undo a delete's notice offers; `None` for every other answer.
    undo: Option<UndoView>,
    error: Option<String>,
    conflict: Option<ConflictView>,
    focus_url: String,
    /// The tab strip the server draws (TAN-887); empty draws no strip.
    tabs: Vec<TabView>,
    /// The list the response stores in the `tabs` cookie.
    tabs_state: ui_state::TabsState,
}

/// One tab of the strip.
struct TabView {
    path: String,
    name: String,
    /// The folder, only where two tabs share a name.
    folder: String,
    selected: bool,
    /// Whether the tab is a tab stop: the open note's, else the first.
    stop: bool,
    href: String,
    /// The request that closes the tab and lands on its neighbour.
    close_href: String,
}

/// The notes view's own URL: the focus, the show-all escape and one note.
fn notes_href(project: &str, show_all: bool, path: Option<&str>, mode: Option<&str>) -> String {
    notes_href_closing(project, show_all, path, mode, None)
}

/// `notes_href`, plus the tab the request closes.
fn notes_href_closing(
    project: &str,
    show_all: bool,
    path: Option<&str>,
    mode: Option<&str>,
    close: Option<&str>,
) -> String {
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
    if let Some(close) = close {
        pairs.push(("close", close.to_string()));
    }
    match serde_urlencoded::to_string(&pairs) {
        Ok(query) => format!("/notes?{query}"),
        Err(_) => "/notes".to_string(),
    }
}

/// The graph view's own URL, keeping the focus and the show-all escape.
fn graph_href(project: &str, show_all: bool) -> String {
    let mut pairs: Vec<(&str, String)> = vec![("project", project.to_string())];
    if show_all {
        pairs.push(("show_all", "1".to_string()));
    }
    match serde_urlencoded::to_string(&pairs) {
        Ok(query) => format!("/notes/graph?{query}"),
        Err(_) => "/notes/graph".to_string(),
    }
}

/// What the graph container carries: the JSON source (scoped by the focus) and
/// the click prefixes, each already holding the focus params.
fn graph_pane(project: &str, show_all: bool) -> GraphPane {
    let mut pairs: Vec<(&str, String)> = vec![("project", project.to_string())];
    if show_all {
        pairs.push(("show_all", "1".to_string()));
    }
    let query = serde_urlencoded::to_string(&pairs).unwrap_or_default();
    let src = if project.is_empty() {
        "/notes/graph.json".to_string()
    } else {
        match serde_urlencoded::to_string([("project", project)]) {
            Ok(query) => format!("/notes/graph.json?{query}"),
            Err(_) => "/notes/graph.json".to_string(),
        }
    };
    GraphPane {
        src,
        note_href: format!("/notes?{query}"),
        concept_href: format!("/notes/concept?{query}"),
    }
}

/// The tree the focus draws: the `projects` folder narrowed to the one project
/// folder its name answers, case-insensitively. No focus, no matching folder
/// (ambiguous, renamed or absent) draws the whole vault, so a focus never hides
/// notes.
fn focus_nodes(nodes: Vec<NotesNode>, active: Option<&str>) -> Vec<NotesNode> {
    let Some(active) = active.filter(|active| !active.trim().is_empty()) else {
        return nodes;
    };
    let wanted = active.trim().to_lowercase();
    let mut out = nodes;
    for node in &mut out {
        if node.kind != NodeKind::Folder || node.path != "projects" {
            continue;
        }
        let narrowed: Vec<NotesNode> = node
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

fn note_paths(nodes: &[NotesNode], out: &mut Vec<String>) {
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

/// The tag a folder carries in the tree when it matches the board: `project` on
/// `projects/<project>`, `label` on `projects/<project>/<label>`, and `no label`
/// (a warning) on a project subfolder none of the project's labels names. Deeper
/// folders and folders outside `projects/` carry none.
fn tree_tag(path: &str, names: &BoardNames) -> (&'static str, bool) {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.len() < 2 || !parts[0].eq_ignore_ascii_case("projects") {
        return ("", false);
    }
    let Some(project) = names.project(parts[1]) else {
        return ("", false);
    };
    match parts.len() {
        2 => ("project", false),
        3 => {
            let named = project
                .labels
                .iter()
                .any(|label| label.to_lowercase() == parts[2].to_lowercase());
            if named {
                ("label", false)
            } else {
                ("no label", true)
            }
        }
        _ => ("", false),
    }
}

/// Draw each folder's fold and hide the rows under a shut one, from the
/// remembered `tree` state and the open note, so the first paint is final.
/// `tandem.js` reads the same state with the same rules (TAN-880).
fn fold_rows(rows: &mut [TreeRowView], state: Option<&ui_state::TreeState>) {
    let folders: Vec<&str> = rows
        .iter()
        .filter(|row| row.folder)
        .map(|row| row.path.as_str())
        .collect();
    let open = rows
        .iter()
        .find(|row| row.selected && !row.folder)
        .map(|row| row.path.as_str());
    let shut = ui_state::shut_folders(state, &folders, open);
    let mut hide_below: Option<usize> = None;
    for row in rows.iter_mut() {
        if row.folder {
            row.expanded = !shut.contains(&row.path);
        }
        if let Some(level) = hide_below {
            if row.level > level {
                row.hidden = true;
                continue;
            }
            hide_below = None;
        }
        if row.folder && !row.expanded {
            hide_below = Some(row.level);
        }
    }
}

fn flatten_tree(
    nodes: &[NotesNode],
    depth: usize,
    names: &BoardNames,
    rows: &mut Vec<TreeRowView>,
) {
    for node in nodes {
        let folder = node.kind == NodeKind::Folder;
        let (tag, tag_warn) = if folder {
            tree_tag(&node.path, names)
        } else {
            ("", false)
        };
        rows.push(TreeRowView {
            name: note_name(&node.path),
            path: node.path.clone(),
            href: String::new(),
            kind: if folder { "folder" } else { "note" },
            level: depth + 1,
            indent: 8 + depth * 14,
            folder,
            selected: false,
            expanded: true,
            hidden: false,
            tag,
            tag_warn,
        });
        if folder {
            flatten_tree(&node.children, depth + 1, names, rows);
        }
    }
}

/// Where a project or label page opens.
fn concept_href(project: &str, show_all: bool, kind: ConceptKind, name: &str) -> String {
    let mut pairs: Vec<(&str, String)> = vec![
        ("kind", kind.word().to_string()),
        ("name", name.to_string()),
        ("project", project.to_string()),
    ];
    if show_all {
        pairs.push(("show_all", "1".to_string()));
    }
    match serde_urlencoded::to_string(&pairs) {
        Ok(query) => format!("/notes/concept?{query}"),
        Err(_) => "/notes".to_string(),
    }
}

/// The board's own spelling of a project or label, matched ignoring case. The
/// query's words only ever select a name the board holds.
fn resolve_concept(names: &BoardNames, kind: &str, name: &str) -> Option<Concept> {
    let kind = ConceptKind::parse(kind)?;
    let name = name.trim();
    let found = match kind {
        ConceptKind::Project => names.project(name).map(|project| project.name.clone()),
        ConceptKind::Label => names.label(name).map(str::to_string),
    }?;
    Some(Concept { kind, name: found })
}

/// The direct child folder whose name matches `name` ignoring case.
fn child_folder<'a>(nodes: &'a [NotesNode], name: &str) -> Option<&'a NotesNode> {
    nodes.iter().find(|node| {
        node.kind == NodeKind::Folder && node.name.to_lowercase() == name.to_lowercase()
    })
}

/// The vault's folder for a project, or for one of its labels.
fn board_folder<'a>(
    tree: &'a [NotesNode],
    project: &str,
    label: Option<&str>,
) -> Option<&'a NotesNode> {
    let projects = tree
        .iter()
        .find(|node| node.kind == NodeKind::Folder && node.name.eq_ignore_ascii_case("projects"))?;
    let folder = child_folder(&projects.children, project)?;
    match label {
        None => Some(folder),
        Some(label) => child_folder(&folder.children, label),
    }
}

/// The project's entry note: the note in the project folder's root named like
/// the project, ignoring case. Created on the first visit when there is none.
/// The path comes from the vault's own tree or from the board's name through
/// the vault's path guard, never from the query.
fn entry_note(vault: &crate::notes::Notes, project: &str) -> Result<String, String> {
    if !crate::notes::health::is_component(project) {
        return Err(format!(
            "The project {project} has no folder of its own, so it has no entry note."
        ));
    }
    let tree = vault.tree();
    let folder = board_folder(&tree, project, None);
    if let Some(found) = folder
        .into_iter()
        .flat_map(|folder| folder.children.iter())
        .find(|node| {
            node.kind != NodeKind::Folder
                && note_name(&node.path).to_lowercase() == project.to_lowercase()
        })
    {
        return Ok(found.path.clone());
    }
    let parent = folder.map_or_else(
        || format!("projects/{project}"),
        |folder| folder.path.clone(),
    );
    vault
        .create(&format!("{parent}/{project}.md"))
        .map(|read| read.value)
        .map_err(|error| error.message())
}

fn count_of(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

fn notes_under(node: &NotesNode) -> Vec<String> {
    let mut out = Vec::new();
    note_paths(&node.children, &mut out);
    out
}

/// The crumbs of a trail of folder and page names under the vault's root.
fn crumbs_of(labels: &[String]) -> Vec<Crumb> {
    let mut crumbs = vec![Crumb {
        label: "Notes".to_string(),
        current: labels.is_empty(),
        first: true,
    }];
    for (index, label) in labels.iter().enumerate() {
        crumbs.push(Crumb {
            label: label.clone(),
            current: index + 1 == labels.len(),
            first: false,
        });
    }
    crumbs
}

fn recoverable_label(recovery: &crate::notes::Recovery) -> String {
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
    fold_state: Option<&ui_state::TreeState>,
    tabs_cookie: Option<&ui_state::TabsState>,
) -> Result<NotesView, String> {
    let project = field(params, "project").trim().to_string();
    let show_all = !matches!(field(params, "show_all"), "" | "0" | "false");
    let graph_view = field(params, "view") == "graph";
    let mut path = params
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
    let names = crate::rpc::vault::try_names(&board).map_err(|error| error.message())?;
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

    let vault = context.notes();
    let mut render_error = error;

    // A project or label page: the board's name decides, never the query.
    let mut concept = None;
    if !field(params, "concept").is_empty() {
        let wanted = field(params, "concept_name");
        match resolve_concept(&names, field(params, "concept"), wanted) {
            Some(found) => concept = Some(found),
            None => {
                render_error.get_or_insert(format!(
                    "No {} named {} on the board.",
                    if field(params, "concept") == "label" {
                        "label"
                    } else if field(params, "concept") == "project" {
                        "project"
                    } else {
                        "project or label"
                    },
                    wanted.trim()
                ));
            }
        }
    }
    match concept
        .as_ref()
        .map(|found| (found.kind, found.name.clone()))
    {
        Some((ConceptKind::Project, name)) => match entry_note(&vault, &name) {
            Ok(entry) => path = Some(entry),
            Err(reason) => {
                render_error.get_or_insert(reason);
                concept = None;
            }
        },
        Some((ConceptKind::Label, _)) => path = None,
        None => {}
    }

    let tree = vault.tree();
    let mut every_note = Vec::new();
    note_paths(&tree, &mut every_note);

    // The concept page's sections, from the board and the vault together.
    let mut sections: Vec<RelSection> = Vec::new();
    let mut label_page = None;
    let mut label_trail: Vec<String> = Vec::new();
    if let Some(found) = &concept {
        let tickets = match board.list_snapshot(&TicketQuery::default()) {
            Ok((tickets, _)) => tickets,
            Err(error) => {
                render_error.get_or_insert(error.message());
                Vec::new()
            }
        };
        let ticket_rows = |keep: &dyn Fn(&crate::domain::ticket::Ticket) -> bool| -> Vec<RelRow> {
            tickets
                .iter()
                .filter(|ticket| !ticket.archived && ticket.status != Status::Done && keep(ticket))
                .map(|ticket| RelRow {
                    name: sheet_handle(ticket),
                    detail: ticket.title.clone(),
                    href: Some(format!("/tickets/{}", ticket.id)),
                    swap: false,
                })
                .collect()
        };
        let note_row = |note_path: &str, detail: String| RelRow {
            name: note_name(note_path),
            detail,
            href: Some(notes_href(&project, show_all, Some(note_path), None)),
            swap: true,
        };
        let mut linked_from: Vec<(String, String)> = Vec::new();
        match vault.typed_backlinks(found.kind, &found.name, &names) {
            Ok(read) => {
                linked_from = read
                    .value
                    .into_iter()
                    .map(|row| (row.path, row.line))
                    .collect();
            }
            Err(error) => {
                render_error.get_or_insert(error.message());
            }
        }
        match found.kind {
            ConceptKind::Label => {
                let owner = names
                    .projects
                    .iter()
                    .find(|owner| owner.labels.contains(&found.name))
                    .map(|owner| owner.name.clone())
                    .unwrap_or_default();
                let folder = if owner.to_lowercase() == found.name.to_lowercase() {
                    None
                } else {
                    board_folder(&tree, &owner, Some(&found.name))
                };
                let filed: Vec<String> = folder.map(notes_under).unwrap_or_default();
                let wanted = found.name.to_lowercase();
                let open = ticket_rows(&|ticket| {
                    ticket.label.as_deref().map(str::to_lowercase).as_deref()
                        == Some(wanted.as_str())
                });
                label_page = Some(LabelPage {
                    name: found.name.clone(),
                    summary: format!(
                        "Label in project {owner}. {} filed here, {} to it, {}.",
                        count_of(filed.len(), "note", "notes"),
                        if linked_from.len() == 1 {
                            "1 note links".to_string()
                        } else {
                            format!("{} notes link", linked_from.len())
                        },
                        count_of(open.len(), "open ticket", "open tickets"),
                    ),
                });
                label_trail = vec!["projects".to_string(), owner.clone(), found.name.clone()];
                sections.push(RelSection::new(
                    "filed",
                    "Filed here",
                    true,
                    "No notes in this folder yet.",
                    filed
                        .iter()
                        .map(|note| note_row(note, note.clone()))
                        .collect(),
                ));
                sections.push(RelSection::new(
                    "linked",
                    "Linked from",
                    true,
                    "No note links here with [[label:...]] yet.",
                    linked_from
                        .iter()
                        .map(|(note, line)| note_row(note, line.clone()))
                        .collect(),
                ));
                sections.push(RelSection::new(
                    "tickets",
                    "Tickets",
                    false,
                    "No open tickets in this label.",
                    open,
                ));
            }
            ConceptKind::Project => {
                let entry = path.clone().unwrap_or_default();
                let folder = board_folder(&tree, &found.name, None);
                let mut filed = Vec::new();
                if let Some(folder) = folder {
                    let owner = names.project(&found.name);
                    for child in folder
                        .children
                        .iter()
                        .filter(|node| node.kind == NodeKind::Folder)
                    {
                        let is_label = owner.is_some_and(|owner| {
                            owner
                                .labels
                                .iter()
                                .any(|label| label.to_lowercase() == child.name.to_lowercase())
                        });
                        let count = count_of(notes_under(child).len(), "note", "notes");
                        filed.push(if is_label {
                            RelRow {
                                name: child.name.clone(),
                                detail: format!("label, {count}"),
                                href: Some(concept_href(
                                    &project,
                                    show_all,
                                    ConceptKind::Label,
                                    &child.name,
                                )),
                                swap: true,
                            }
                        } else {
                            RelRow {
                                name: child.name.clone(),
                                detail: format!("folder, no label, {count}"),
                                href: None,
                                swap: false,
                            }
                        });
                    }
                    for child in folder
                        .children
                        .iter()
                        .filter(|node| node.kind != NodeKind::Folder && node.path != entry)
                    {
                        filed.push(note_row(&child.path, "note".to_string()));
                    }
                }
                let mut merged: Vec<(String, String)> = Vec::new();
                match vault.backlinks(&entry) {
                    Ok(read) => {
                        merged.extend(read.value.into_iter().map(|row| (row.path, row.line)));
                    }
                    Err(error) => {
                        render_error.get_or_insert(error.message());
                    }
                }
                for row in linked_from {
                    if !merged.iter().any(|(note, _)| *note == row.0) {
                        merged.push(row);
                    }
                }
                merged.sort_by_key(|(note, _)| note.to_lowercase());
                let wanted = found.name.to_lowercase();
                let open = ticket_rows(&|ticket| ticket.project.to_lowercase() == wanted);
                sections.push(RelSection::new(
                    "filed",
                    "Filed here",
                    true,
                    "Nothing is filed under this project yet.",
                    filed,
                ));
                sections.push(RelSection::new(
                    "linked",
                    "Linked from",
                    true,
                    "No note links here yet.",
                    merged
                        .iter()
                        .map(|(note, line)| note_row(note, line.clone()))
                        .collect(),
                ));
                sections.push(RelSection::new(
                    "tickets",
                    "Tickets",
                    false,
                    "No open tickets in this project.",
                    open,
                ));
            }
        }
    }

    let focused = focus_nodes(tree, if show_all { None } else { Some(&project) });
    let mut rows = Vec::new();
    flatten_tree(&focused, 0, &names, &mut rows);
    for row in &mut rows {
        if row.folder {
            continue;
        }
        row.href = notes_href(&project, show_all, Some(&row.path), None);
        row.selected = path.as_deref() == Some(row.path.as_str());
    }
    fold_rows(&mut rows, fold_state);

    let mut note = None;
    let mut crumbs = Vec::new();
    let mut backlinks = Vec::new();
    let mut linked = Vec::new();
    let mut links_to: Vec<RelRow> = Vec::new();
    let mut broken_links = "[]".to_string();

    if label_page.is_some() {
        crumbs = crumbs_of(&label_trail);
    }

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
                crumbs = crumbs_of(&labels);
                // One resolver (ADR 0005): the notes store resolves every link in
                // the text, and the renderer looks each target up here.
                let occurrences = vault.links_with(&content.text, &names);
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
                    .map(|occurrence| broken_key(&occurrence.link.target))
                    .collect();
                broken.sort();
                broken.dedup();
                broken_links = serde_json::to_string(&broken).unwrap_or_else(|_| "[]".to_string());
                for occurrence in &occurrences {
                    let (kind, name) = match &occurrence.resolution {
                        LinkResolution::Project(name) => (ConceptKind::Project, name),
                        LinkResolution::Label(name) => (ConceptKind::Label, name),
                        _ => continue,
                    };
                    if links_to
                        .iter()
                        .any(|row| row.name == *name && row.detail.starts_with(kind.word()))
                    {
                        continue;
                    }
                    let detail = match kind {
                        ConceptKind::Project => "project".to_string(),
                        ConceptKind::Label => names
                            .projects
                            .iter()
                            .find(|owner| owner.labels.contains(name))
                            .map_or_else(
                                || "label".to_string(),
                                |owner| format!("label in {}", owner.name),
                            ),
                    };
                    links_to.push(RelRow {
                        name: name.clone(),
                        detail,
                        href: Some(concept_href(&project, show_all, kind, name)),
                        swap: true,
                    });
                }
                let resolve = |target: &str| {
                    resolved
                        .get(target)
                        .cloned()
                        .unwrap_or_else(|| vault.resolve_with(target, &names))
                };
                let href = |path: &str| notes_href(&project, show_all, Some(path), None);
                let concept_link =
                    |kind: ConceptKind, name: &str| concept_href(&project, show_all, kind, name);
                let html = markdown::document_with_links(
                    &TicketMarkdown::blocks(Some(&content.text)),
                    &markdown::Wikilinks {
                        resolve: &resolve,
                        href: &href,
                        concept: &concept_link,
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

    // A concept page draws its own sections where a note draws Backlinks and
    // Tickets; a note adds the projects and labels it links to.
    let note_sections = concept.is_none();
    if !note_sections {
        backlinks.clear();
        linked.clear();
    } else {
        sections.push(RelSection::new(
            "links-to",
            "Links to",
            true,
            "This note links to no project or label.",
            links_to,
        ));
    }
    let recoveries: Vec<String> = vault.recoveries().iter().map(recoverable_label).collect();
    let relations_count = backlinks.len()
        + linked.len()
        + recoveries.len()
        + sections
            .iter()
            .map(|section| section.rows.len())
            .sum::<usize>();

    let current_path = note
        .as_ref()
        .map_or(String::new(), |pane| pane.path.clone());
    let change = ui_state::TabChange {
        current: &current_path,
        from: field(params, "tab_from"),
        to: field(params, "tab_to"),
        gone: field(params, "tab_gone"),
        close: field(params, "close"),
    };
    let stored = tabs_cookie
        .map(|state| state.open.as_slice())
        .unwrap_or(&[]);
    let open = ui_state::sync_tabs(stored, &change, &every_note);
    let tabs_state = ui_state::fit_tabs(open, &current_path);
    let focus_stop = tabs_state
        .open
        .iter()
        .position(|path| *path == current_path)
        .unwrap_or(0);
    let tabs: Vec<TabView> = ui_state::tab_labels(&tabs_state.open)
        .into_iter()
        .enumerate()
        .map(|(at, label)| {
            let next = if label.path == current_path {
                ui_state::close_target(&tabs_state.open, &label.path)
            } else {
                current_path.clone()
            };
            TabView {
                href: notes_href(&project, show_all, Some(&label.path), None),
                close_href: notes_href_closing(
                    &project,
                    show_all,
                    Some(&next)
                        .filter(|next| !next.is_empty())
                        .map(String::as_str),
                    None,
                    Some(&label.path),
                ),
                selected: label.path == current_path,
                stop: at == focus_stop,
                path: label.path,
                name: label.name,
                folder: label.folder,
            }
        })
        .collect();

    Ok(NotesView {
        graph: graph_view.then(|| graph_pane(&project, show_all)),
        note_switch_href: notes_href(&project, show_all, None, None),
        graph_switch_href: graph_href(&project, show_all),
        focus_action: if graph_view { "/notes/graph" } else { "/notes" },
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
        sections,
        note_sections,
        concept_kind: concept
            .as_ref()
            .filter(|found| found.kind == ConceptKind::Project)
            .map_or(String::new(), |found| found.kind.word().to_string()),
        concept_name: concept
            .as_ref()
            .filter(|found| found.kind == ConceptKind::Project)
            .map_or(String::new(), |found| found.name.clone()),
        label_page,
        relations_count,
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
        tabs,
        tabs_state,
    })
}

/// The key the editor looks a broken link up by: the lowercased target, a typed
/// one as `kind:name` with the spaces around the colon dropped.
fn broken_key(target: &str) -> String {
    match typed_target(target) {
        Some((is_project, name)) => format!(
            "{}:{}",
            if is_project { "project" } else { "label" },
            name.to_lowercase()
        ),
        None => target.to_lowercase(),
    }
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
    let tree: Option<ui_state::TreeState> = ui_state::read(headers, &context.ui_cookie(), "tree");
    let stored_tabs: Option<ui_state::TabsState> =
        ui_state::read(headers, &context.ui_cookie(), "tabs");
    let view = match render_notes(
        context,
        params,
        notice,
        error,
        conflict,
        tree.as_ref(),
        stored_tabs.as_ref(),
    ) {
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
    // The server is the tab list's only writer: the cookie goes back whenever
    // the list or the open note differs from what the browser sent.
    let tabs_cookie = if stored_tabs.clone().unwrap_or_default() == view.tabs_state {
        None
    } else {
        ui_state::set_cookie(&context.ui_cookie(), "tabs", &view.tabs_state)
    };
    let mut response = notes_page_response(context, headers, body);
    // A close request is spent once answered: the address bar drops `close`,
    // so a reload never closes the tab again.
    if !field(params, "close").is_empty() && is_partial(headers) {
        let url = notes_href(
            field(params, "project").trim(),
            !matches!(field(params, "show_all"), "" | "0" | "false"),
            Some(field(params, "path")).filter(|path| !path.is_empty()),
            None,
        );
        if let Ok(value) = HeaderValue::from_str(&url) {
            response.headers_mut().insert("hx-replace-url", value);
        }
    }
    if let Some(cookie) = tabs_cookie {
        response.headers_mut().append(header::SET_COOKIE, cookie);
    }
    response
}

fn notes_page_response(context: &Context, headers: &HeaderMap, body: String) -> Response {
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
        ui_cookie: context.ui_cookie(),
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

/// `GET /notes/graph`: the notes face with the graph in the main column. No
/// note is open, so a stray `path` or concept never reaches the page.
pub(super) async fn note_graph_page(
    context: Context,
    Query(mut params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    for key in ["path", "concept", "concept_name", "mode"] {
        params.remove(key);
    }
    params.insert("view".to_string(), "graph".to_string());
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

/// Follow a wikilink the editor drew: resolve `target` through the notes store and
/// open the note it names, else stay on `from` with the banner. The target
/// never builds a path; only a path the notes store resolved is opened.
pub(super) async fn note_open(
    context: Context,
    Query(mut params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    let target = field(&params, "target").to_string();
    let from = field(&params, "from").to_string();
    params.remove("path");
    match context.notes().resolve(&target) {
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

/// A project or label page: `kind` and `name` from the query pick a name the
/// board holds. A label has a page of its own; a project opens its entry note.
/// An unknown name draws the notes face with the error banner and its way back.
pub(super) async fn note_concept(
    context: Context,
    Query(mut params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    let kind = field(&params, "kind").to_string();
    let name = field(&params, "name").to_string();
    params.remove("path");
    params.insert("concept".to_string(), kind);
    params.insert("concept_name".to_string(), name);
    params.insert("mode".to_string(), "edit".to_string());
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
    let vault = context.notes();

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
    let vault = context.notes();
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

/// The first line of a new daily note: `About: [[project:<name>]]` for a project
/// the board holds, spelled as the board spells it. `None` with no focus, an
/// unknown project, or a name that could not sit inside a link.
fn about_line(context: &Context, focus: &str) -> Option<String> {
    let focus = focus.trim();
    if focus.is_empty() {
        return None;
    }
    let names = crate::rpc::vault::try_names(&context.board()).ok()?;
    let name = &names.project(focus)?.name;
    if name.contains(['[', ']', '|', '#', '\n']) {
        return None;
    }
    Some(format!("About: [[project:{name}]]\n"))
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
    let vault = context.notes();
    // An existing page is the common case and not an error: open it.
    let opened = match vault.create(&path) {
        Ok(read) => {
            // A new page under a focused project starts with its About line;
            // an existing page is never touched.
            if let Some(line) = about_line(&context, field(&params, "project"))
                && let Err(error) = vault.write(&read.value, &line, None)
            {
                tracing::warn!(error = ?error, "the daily note's About line was not written");
            }
            read.value
        }
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
    match context.notes().rename(&path, &target) {
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
    let error = match context.notes().delete(&path) {
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
    match context.notes().restore(&trash) {
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
    let vault = context.notes();
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
    let vault = context.notes();
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
