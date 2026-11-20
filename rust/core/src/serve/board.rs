//! The board surface: the filter bar, the columns and the card writes, and the
//! shell each of them draws in.

use super::*;
use crate::domain::DomainError;

/// The board's visible set: every non-archived ticket, narrowed by the project
/// focus, the label, the assignee and the title search.
fn visible<'a>(
    tickets: &'a [Ticket],
    projects: &[crate::domain::ProjectWithLabels],
    filters: &Filters,
) -> Option<Vec<&'a Ticket>> {
    let focus: Option<&str> = if filters.show_all || filters.project.is_empty() {
        None
    } else {
        projects
            .iter()
            .find(|project| project.name == filters.project)
            .map(|project| project.name.as_str())
    };
    let label = filters.label_axis();
    let assignee = filters.assignee_axis();
    let needle = filters.text.trim();
    Some(
        tickets
            .iter()
            .filter(|ticket| {
                if ticket.archived {
                    return false;
                }
                if let Some(project) = focus
                    && ticket.project != project
                {
                    return false;
                }
                match &label {
                    LabelAxis::Any => {}
                    LabelAxis::Unnamed => {
                        if ticket.label.is_some() {
                            return false;
                        }
                    }
                    LabelAxis::Named(name) => {
                        if ticket.label.as_deref() != Some(name.as_str()) {
                            return false;
                        }
                    }
                }
                match &assignee {
                    AssigneeAxis::Any => {}
                    AssigneeAxis::Unassigned => {
                        if ticket.assignee.is_some() {
                            return false;
                        }
                    }
                    AssigneeAxis::Named(name) => {
                        if ticket.assignee.as_deref() != Some(name.as_str()) {
                            return false;
                        }
                    }
                }
                needle.is_empty() || TextMatch::contains(needle, &ticket.title)
            })
            .collect(),
    )
}
// MARK: - The templates

#[derive(Template)]
#[template(path = "board.html")]
struct BoardView {
    revision: i64,
    self_url: String,
    dismiss_url: String,
    /// The filters as a query string, so a card's link to its sheet carries
    /// them and the sheet opens over the board the human was looking at.
    board_query: String,
    notice: Option<String>,
    form: FiltersView,
    project_options: Vec<ProjectOption>,
    label_options: Vec<LabelOption>,
    assignee_options: Vec<AssigneeOption>,
    label_rows: Vec<LabelRow>,
    /// The New form's Project select: every project, the focused one chosen.
    new_projects: Vec<NewProjectOption>,
    /// The New form's Label select: every label with its project, so the page
    /// script can show only the chosen project's.
    new_labels: Vec<NewLabelOption>,
    /// Whether the New form's Project select starts on a project.
    project_chosen: bool,
    /// An empty home: there is no project to file into.
    no_projects: bool,
    show_all: bool,
    empty: Option<EmptyView>,
    columns: Vec<ColumnView>,
    /// The ticket sheet, drawn inside the board root so an htmx swap of the
    /// root carries both. `None` on the plain board.
    sheet: Option<askama::filters::Safe<String>>,
}

/// One label the board may rename.
struct LabelRow {
    name: String,
    encoded: String,
}

pub(super) struct FiltersView {
    pub(super) project: String,
    pub(super) show_all: bool,
    pub(super) label: String,
    pub(super) assignee: String,
    pub(super) text: String,
}

pub(super) struct ProjectOption {
    pub(super) value: String,
    pub(super) label: String,
    pub(super) selected: bool,
}

pub(super) struct LabelOption {
    pub(super) value: String,
    pub(super) label: String,
    pub(super) selected: bool,
}

struct NewProjectOption {
    name: String,
    selected: bool,
}

struct NewLabelOption {
    name: String,
    project: String,
    selected: bool,
    /// Not the preset project's: hidden until the script shows it, so a page
    /// without the script still draws one project's labels.
    hidden: bool,
}

struct AssigneeOption {
    value: String,
    label: String,
    selected: bool,
}

struct EmptyView {
    title: String,
    subtitle: String,
    hero: bool,
}

struct ColumnView {
    status: &'static str,
    label: &'static str,
    count: usize,
    cards: Vec<CardView>,
}

struct CardView {
    id: String,
    title: String,
    reference: Option<String>,
    label: Option<LabelView>,
    assignee: Option<AssigneeView>,
    blocked: Option<String>,
    selected: bool,
}

struct LabelView {
    name: String,
    tone: &'static str,
}

struct AssigneeView {
    label: String,
    human: bool,
}

/// The palette tokens a label chip may draw, in the renderer's fixed order.
const LABEL_TONES: [&str; 7] = ["moss", "ember", "accent", "sun", "skin", "hill", "sky"];

/// The tone a label's chip draws: the label name hashed over the palette tokens,
/// exactly as the renderer's board module did, so a label keeps its colour and
/// no colour is stored.
fn label_tone(name: &str) -> &'static str {
    let mut hash: u32 = 0;
    for character in name.chars() {
        hash = hash.wrapping_mul(31).wrapping_add(character as u32);
    }
    LABEL_TONES[(hash % LABEL_TONES.len() as u32) as usize]
}

/// The block's one-line mark: `Blocked, TAN-42` when exactly one open blocker
/// has a ref to name, `Blocked` when it has none or there are several.
fn block_label(ticket: &Ticket) -> Option<String> {
    if !ticket.blocked() {
        return None;
    }
    let open: Vec<_> = ticket
        .blocked_by
        .iter()
        .filter(|block| block.status != Status::Done)
        .collect();
    if open.len() != 1 {
        return Some("Blocked".to_string());
    }
    match open[0].reference.as_deref() {
        Some(reference) => Some(format!("Blocked, {reference}")),
        None => Some("Blocked".to_string()),
    }
}

fn assignee_view(assignee: Option<&str>) -> Option<AssigneeView> {
    assignee.map(|name| AssigneeView {
        label: if name == "human" {
            "human".to_string()
        } else {
            format!("@{name}")
        },
        human: name == "human",
    })
}

/// The one message the board draws when there is nothing to draw.
fn empty_view(
    tickets: &[Ticket],
    visible: usize,
    filters: &Filters,
    projects: &[crate::domain::ProjectWithLabels],
) -> Option<EmptyView> {
    if tickets.is_empty() {
        return Some(EmptyView {
            title: "Nothing on the board".to_string(),
            subtitle: "Add a card from a column's foot, or from the CLI.".to_string(),
            hero: true,
        });
    }
    if visible != 0 {
        return None;
    }
    if !filters.text.trim().is_empty() {
        return Some(EmptyView {
            title: "No matches".to_string(),
            subtitle: "No ticket title matches your search.".to_string(),
            hero: false,
        });
    }
    if !filters.label.is_empty() || !filters.assignee.is_empty() {
        return Some(EmptyView {
            title: "No tickets match these filters".to_string(),
            subtitle: "Clear a filter to see more.".to_string(),
            hero: false,
        });
    }
    if !filters.project.is_empty()
        && projects
            .iter()
            .any(|project| project.name == filters.project)
    {
        return Some(EmptyView {
            title: format!("No tickets in {}", filters.project),
            subtitle: "Add a card from a column's foot, or show every project.".to_string(),
            hero: false,
        });
    }
    Some(EmptyView {
        title: "The board is empty".to_string(),
        subtitle: "Add a card from a column's foot.".to_string(),
        hero: false,
    })
}

/// The board the current store and filters draw, or the read failure that kept
/// it from being drawn.
fn render_board(
    context: &Context,
    filters: &Filters,
    notice: Option<String>,
) -> Result<BoardView, String> {
    let board = context.board();
    // A store newer than this build cannot be read: draw an empty board with
    // the upgrade words as the page notice, not a bare 500. Any other failure
    // stays the 500 it was.
    let (tickets, revision, notice) = match board.list_snapshot(&TicketQuery::default()) {
        Ok((tickets, revision)) => (tickets, revision, notice),
        Err(error @ DomainError::StoreNewer(_)) => (Vec::new(), 0, Some(error.message())),
        Err(error) => return Err(error.message()),
    };
    let refused = notice.is_some() && revision == 0 && tickets.is_empty();
    let (projects, _) = match board.projects_snapshot() {
        Err(DomainError::StoreNewer(_)) if refused => (Vec::new(), 0),
        other => other.map_err(|error| error.message())?,
    };
    let (labels, _) = match board.labels_snapshot() {
        Err(DomainError::StoreNewer(_)) if refused => (Vec::new(), 0),
        other => other.map_err(|error| error.message())?,
    };
    let shown = visible(&tickets, &projects, filters);
    let shown = shown.unwrap_or_default();

    let project_options = {
        let mut options = vec![ProjectOption {
            value: String::new(),
            label: "All projects".to_string(),
            selected: filters.project.is_empty()
                || !projects
                    .iter()
                    .any(|project| project.name == filters.project),
        }];
        for project in &projects {
            options.push(ProjectOption {
                value: project.name.clone(),
                label: project.name.clone(),
                selected: project.name == filters.project,
            });
        }
        options
    };

    let label_options = {
        let mut options = vec![
            LabelOption {
                value: String::new(),
                label: "All labels".to_string(),
                selected: filters.label.is_empty(),
            },
            LabelOption {
                value: " ".to_string(),
                label: "No label".to_string(),
                selected: filters.label == " ",
            },
        ];
        for label in &labels {
            // With a project in focus the filter offers that project's labels.
            if !filters.show_all
                && let Some(focused) = projects
                    .iter()
                    .find(|project| project.name == filters.project)
                && !focused.labels.contains(label)
            {
                continue;
            }
            options.push(LabelOption {
                value: label.clone(),
                label: label.clone(),
                selected: filters.label == *label,
            });
        }
        options
    };

    // The New form files into its own Project and Label selects. The Project
    // starts on the focused project (zero extra clicks); with "All projects" in
    // focus it starts empty and `required` stops a submit. The Label starts on
    // the filter's label when it belongs to that project, else "No label".
    let preset: Option<&str> = if filters.show_all {
        None
    } else {
        projects
            .iter()
            .find(|project| project.name == filters.project)
            .map(|project| project.name.as_str())
    };
    let new_projects: Vec<NewProjectOption> = projects
        .iter()
        .map(|project| NewProjectOption {
            name: project.name.clone(),
            selected: Some(project.name.as_str()) == preset,
        })
        .collect();
    let new_labels: Vec<NewLabelOption> = projects
        .iter()
        .flat_map(|project| {
            project.labels.iter().map(|label| NewLabelOption {
                name: label.clone(),
                project: project.name.clone(),
                selected: Some(project.name.as_str()) == preset && filters.label == *label,
                hidden: Some(project.name.as_str()) != preset,
            })
        })
        .collect();

    let assignee_options = {
        let mut names: Vec<String> = Vec::new();
        for ticket in &tickets {
            let Some(name) = &ticket.assignee else {
                continue;
            };
            let name = name.trim().to_string();
            if name.is_empty() || names.contains(&name) {
                continue;
            }
            names.push(name);
        }
        let mut options = vec![
            AssigneeOption {
                value: String::new(),
                label: "Anyone".to_string(),
                selected: filters.assignee.is_empty(),
            },
            AssigneeOption {
                value: " ".to_string(),
                label: "Unassigned".to_string(),
                selected: filters.assignee == " ",
            },
        ];
        for name in names {
            options.push(AssigneeOption {
                value: name.clone(),
                label: if name == "human" {
                    "human".to_string()
                } else {
                    format!("@{name}")
                },
                selected: filters.assignee == name,
            });
        }
        if !filters.assignee.is_empty()
            && filters.assignee != " "
            && !options
                .iter()
                .any(|option| option.value == filters.assignee)
        {
            options.push(AssigneeOption {
                value: filters.assignee.clone(),
                label: format!("@{}", filters.assignee),
                selected: true,
            });
        }
        options
    };

    // The columns are the six stages `Status` owns, and that is the whole set a
    // ticket can hold: the workflow tables that once recorded a seventh stage
    // retired in schema v11 (CUT-9), and a stored key outside the six reads back
    // as Backlog (`Status::stored`). No visible ticket is left out of a column.
    let columns = Status::WORKFLOW
        .iter()
        .map(|status| ColumnView {
            status: status.as_str(),
            label: status.display_name(),
            count: shown
                .iter()
                .filter(|ticket| ticket.status == *status)
                .count(),
            cards: shown
                .iter()
                .filter(|ticket| ticket.status == *status)
                .map(|ticket| CardView {
                    id: ticket.id.to_string(),
                    title: ticket.title.clone(),
                    reference: ticket.reference.clone(),
                    label: ticket.label.as_deref().map(|name| LabelView {
                        name: name.to_string(),
                        tone: label_tone(name),
                    }),
                    assignee: assignee_view(ticket.assignee.as_deref()),
                    blocked: block_label(ticket),
                    selected: filters.selected.as_deref() == Some(ticket.id.to_string().as_str()),
                })
                .collect(),
        })
        .collect();

    let empty = empty_view(&tickets, shown.len(), filters, &projects);
    let url = filters.url();
    Ok(BoardView {
        revision,
        self_url: url.clone(),
        dismiss_url: url,
        board_query: filters.query(),
        notice,
        form: FiltersView {
            project: filters.project.clone(),
            show_all: filters.show_all,
            label: filters.label.clone(),
            assignee: filters.assignee.clone(),
            text: filters.text.clone(),
        },
        project_options,
        label_options,
        assignee_options,
        new_projects,
        new_labels,
        project_chosen: preset.is_some(),
        no_projects: projects.is_empty(),
        label_rows: labels
            .iter()
            .map(|label| LabelRow {
                name: label.clone(),
                encoded: super::sheet::encode_component(label),
            })
            .collect(),
        show_all: filters.show_all,
        empty,
        columns,
        sheet: None,
    })
}

/// The board with a sheet drawn inside its root, so one htmx swap carries both.
pub(super) fn board_with_sheet(
    context: &Context,
    filters: &Filters,
    sheet: String,
    self_url: String,
    headers: &HeaderMap,
    status: StatusCode,
) -> Response {
    let view = match render_board(context, filters, None) {
        Ok(view) => view,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("the board could not be read: {error}\n"),
            )
                .into_response();
        }
    };
    let mut view = view;
    view.self_url = self_url;
    view.sheet = Some(askama::filters::Safe(sheet));
    let board = match view.render() {
        Ok(board) => board,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("the board could not be drawn: {error}\n"),
            )
                .into_response();
        }
    };
    if is_partial(headers) {
        return html_response(board, status);
    }
    match (PageView {
        assets: assets::urls(),
        body: askama::filters::Safe(board),
        stream: false,
        sign_out: context.access.is_token(),
        export: context.may_export(),
        tokens: context.shows_tokens(),
        people: context.shows_people(),
    })
    .render()
    {
        Ok(html) => html_response(html, status),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("the shell could not be drawn: {error}\n"),
        )
            .into_response(),
    }
}
fn render_response(
    context: &Context,
    filters: &Filters,
    notice: Option<String>,
    headers: &HeaderMap,
    status: StatusCode,
) -> Response {
    if let Some(notice) = &notice {
        tracing::warn!(error = ?notice, "board refused");
    }
    let view = match render_board(context, filters, notice) {
        Ok(view) => view,
        Err(error) => {
            tracing::error!(error = ?error, "the board could not be read");
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("the board could not be read: {error}\n"),
            )
                .into_response();
        }
    };
    let board = match view.render() {
        Ok(board) => board,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("the board could not be drawn: {error}\n"),
            )
                .into_response();
        }
    };
    if is_partial(headers) {
        return html_response(board, status);
    }
    match (PageView {
        assets: assets::urls(),
        body: askama::filters::Safe(board),
        stream: true,
        sign_out: context.access.is_token(),
        export: context.may_export(),
        tokens: context.shows_tokens(),
        people: context.shows_people(),
    })
    .render()
    {
        Ok(html) => html_response(html, status),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("the shell could not be drawn: {error}\n"),
        )
            .into_response(),
    }
}
pub(super) async fn board_page(
    context: Context,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    let filters = Filters::parse(&params);
    render_response(&context, &filters, None, &headers, StatusCode::OK)
}
pub(super) async fn add_ticket(
    context: Context,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    let Ok(Form(params)) = form else {
        return bad_request("the form could not be read");
    };
    let filters = Filters::parse(&params);
    let expected = match expected_revision(&params) {
        Ok(revision) => revision,
        Err(message) => return bad_request(&message),
    };
    if expected != context.revision() {
        return render_response(
            &context,
            &filters,
            Some(CONFLICT_NOTICE.to_string()),
            &headers,
            StatusCode::OK,
        );
    }
    let status = match Status::parse(field(&params, "status")) {
        Ok(status) => status,
        Err(error) => return bad_request(&error.message()),
    };
    // The New form's own selects name the ticket's project and label; the hidden
    // filter fields only carry the view for the re-render.
    let project = field(&params, "ticket_project");
    let label = field(&params, "ticket_label");
    let title = field(&params, "title");
    let refused = |error: DomainError| {
        render_response(
            &context,
            &filters,
            Some(error.message()),
            &headers,
            StatusCode::OK,
        )
    };
    match context.board().add(
        title,
        AddTicket {
            project: Some(project),
            label: Some(label),
            status,
            assignee: None,
            description: None,
            origin: context.actor.as_str(),
        },
    ) {
        Ok(ticket) => {
            tracing::info!(
                actor = ?context.actor.as_str(),
                id = ?ticket.id,
                reference = ?ticket.reference,
                label = ?ticket.label,
                status = ticket.status.as_str(),
                revision = context.revision(),
                "ticket added"
            );
            let mut filters = filters;
            filters.selected = Some(ticket.id.to_string());
            render_response(&context, &filters, None, &headers, StatusCode::OK)
        }
        Err(error) => refused(error),
    }
}

pub(super) async fn move_ticket(
    context: Context,
    Path(id): Path<String>,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    let Ok(Form(params)) = form else {
        return bad_request("the form could not be read");
    };
    let filters = Filters::parse(&params);
    let expected = match expected_revision(&params) {
        Ok(revision) => revision,
        Err(message) => return bad_request(&message),
    };
    if expected != context.revision() {
        return render_response(
            &context,
            &filters,
            Some(CONFLICT_NOTICE.to_string()),
            &headers,
            StatusCode::OK,
        );
    }
    let status = match Status::parse(field(&params, "status")) {
        Ok(status) => status,
        Err(error) => return bad_request(&error.message()),
    };
    let board = context.board();
    let ticket = match board.ticket(&id) {
        Ok(ticket) => ticket,
        Err(error) => return bad_request(&error.message()),
    };
    // The column the card already stands in has no order to write, so a drop
    // there writes nothing (the renderer's rule).
    if ticket.status != status {
        let to = status.as_str();
        match board.set(
            &id,
            SetFields {
                status: Edit::Set(status),
                ..SetFields::default()
            },
        ) {
            Ok(_) => tracing::info!(
                actor = ?context.actor.as_str(),
                id = ?id,
                from = ticket.status.as_str(),
                to,
                revision = context.revision(),
                "ticket moved"
            ),
            Err(error) => return bad_request(&error.message()),
        }
    }
    render_response(&context, &filters, None, &headers, StatusCode::OK)
}

/// Labels belong to the board, not to a ticket: the add and rename forms live in
/// the board's label menu and answer with the board.
pub(super) async fn add_label(
    context: Context,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    let Ok(Form(params)) = form else {
        return bad_request("the form could not be read");
    };
    let filters = Filters::parse(&params);
    // A label lives in a project: the board's project focus names it, and with
    // no focus the refusal lists the projects.
    let project = Some(filters.project.as_str()).filter(|name| !name.is_empty());
    let outcome = context.board().add_label(field(&params, "name"), project);
    if outcome.is_ok() {
        tracing::info!(
            actor = ?context.actor.as_str(),
            label = ?field(&params, "name"),
            revision = context.revision(),
            "label added"
        );
    }
    let notice = outcome.err().map(|error| error.message());
    render_response(&context, &filters, notice, &headers, StatusCode::OK)
}

pub(super) async fn rename_label(
    context: Context,
    Path(name): Path<String>,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    let Ok(Form(params)) = form else {
        return bad_request("the form could not be read");
    };
    let mut filters = Filters::parse(&params);
    let to = field(&params, "to");
    let outcome = context.board().rename_label(&name, to, None);
    if outcome.is_ok() {
        tracing::info!(
            actor = ?context.actor.as_str(),
            from = ?name,
            to = ?to,
            revision = context.revision(),
            "label renamed"
        );
    }
    if outcome.is_ok() && filters.label == name {
        filters.label = to.trim().to_string();
    }
    let notice = outcome.err().map(|error| error.message());
    render_response(&context, &filters, notice, &headers, StatusCode::OK)
}
