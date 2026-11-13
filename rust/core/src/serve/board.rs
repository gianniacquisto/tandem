//! The board surface: the filter bar, the columns and the card writes, and the
//! shell each of them draws in.

use super::*;

/// The board's visible set: every non-archived ticket, narrowed by the project
/// focus, the lane, the assignee and the title search.
fn visible<'a>(
    tickets: &'a [Ticket],
    projects: &[crate::domain::ProjectWithLanes],
    filters: &Filters,
) -> Option<Vec<&'a Ticket>> {
    let focus: Option<Vec<String>> = if filters.show_all || filters.project.is_empty() {
        None
    } else {
        projects
            .iter()
            .find(|project| project.name == filters.project)
            .map(|project| project.lanes.clone())
    };
    let lane = filters.lane_axis();
    let assignee = filters.assignee_axis();
    let needle = filters.text.trim();
    Some(
        tickets
            .iter()
            .filter(|ticket| {
                if ticket.archived {
                    return false;
                }
                if let Some(lanes) = &focus
                    && !ticket
                        .lane
                        .as_ref()
                        .is_some_and(|lane| lanes.contains(lane))
                {
                    return false;
                }
                match &lane {
                    LaneAxis::Any => {}
                    LaneAxis::Unnamed => {
                        if ticket.lane.is_some() {
                            return false;
                        }
                    }
                    LaneAxis::Named(name) => {
                        if ticket.lane.as_deref() != Some(name.as_str()) {
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
    lane_options: Vec<LaneOption>,
    assignee_options: Vec<AssigneeOption>,
    show_all: bool,
    empty: Option<EmptyView>,
    columns: Vec<ColumnView>,
    /// The ticket sheet, drawn inside the board root so an htmx swap of the
    /// root carries both. `None` on the plain board.
    sheet: Option<askama::filters::Safe<String>>,
}

pub(super) struct FiltersView {
    pub(super) project: String,
    pub(super) show_all: bool,
    pub(super) lane: String,
    pub(super) assignee: String,
    pub(super) text: String,
}

pub(super) struct ProjectOption {
    pub(super) value: String,
    pub(super) label: String,
    pub(super) selected: bool,
}

pub(super) struct LaneOption {
    pub(super) value: String,
    pub(super) label: String,
    pub(super) selected: bool,
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
    lane: Option<LaneView>,
    assignee: Option<AssigneeView>,
    blocked: Option<String>,
    selected: bool,
}

struct LaneView {
    name: String,
    tone: &'static str,
}

struct AssigneeView {
    label: String,
    human: bool,
}

/// The palette tokens a lane chip may draw, in the renderer's fixed order.
const LANE_TONES: [&str; 7] = ["moss", "ember", "accent", "sun", "skin", "hill", "sky"];

/// The tone a lane's chip draws: the lane name hashed over the palette tokens,
/// exactly as the renderer's board module did, so a lane keeps its colour and
/// no colour is stored.
fn lane_tone(name: &str) -> &'static str {
    let mut hash: u32 = 0;
    for character in name.chars() {
        hash = hash.wrapping_mul(31).wrapping_add(character as u32);
    }
    LANE_TONES[(hash % LANE_TONES.len() as u32) as usize]
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
    projects: &[crate::domain::ProjectWithLanes],
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
    if !filters.lane.is_empty() || !filters.assignee.is_empty() {
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
    context: &SessionContext,
    filters: &Filters,
    notice: Option<String>,
) -> Result<BoardView, String> {
    let board = context.board();
    let (tickets, revision) = board
        .list_snapshot(&TicketQuery::default())
        .map_err(|error| error.message())?;
    let (projects, _) = board.projects_snapshot().map_err(|error| error.message())?;
    let (lanes, _) = board.lanes_snapshot().map_err(|error| error.message())?;
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

    let lane_options = {
        let mut options = vec![
            LaneOption {
                value: String::new(),
                label: "All lanes".to_string(),
                selected: filters.lane.is_empty(),
            },
            LaneOption {
                value: " ".to_string(),
                label: "No lane".to_string(),
                selected: filters.lane == " ",
            },
        ];
        for lane in &lanes {
            options.push(LaneOption {
                value: lane.clone(),
                label: lane.clone(),
                selected: filters.lane == *lane,
            });
        }
        options
    };

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
                    lane: ticket.lane.as_deref().map(|name| LaneView {
                        name: name.to_string(),
                        tone: lane_tone(name),
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
            lane: filters.lane.clone(),
            assignee: filters.assignee.clone(),
            text: filters.text.clone(),
        },
        project_options,
        lane_options,
        assignee_options,
        show_all: filters.show_all,
        empty,
        columns,
        sheet: None,
    })
}

/// The board with a sheet drawn inside its root, so one htmx swap carries both.
pub(super) fn board_with_sheet(
    context: &SessionContext,
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
        body: askama::filters::Safe(board),
        stream: false,
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
    context: &SessionContext,
    filters: &Filters,
    notice: Option<String>,
    headers: &HeaderMap,
    status: StatusCode,
) -> Response {
    let view = match render_board(context, filters, notice) {
        Ok(view) => view,
        Err(error) => {
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
        body: askama::filters::Safe(board),
        stream: true,
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
    let lane = filters.add_lane();
    let title = field(&params, "title");
    match context.board().add(
        title,
        AddTicket {
            lane: lane.as_deref(),
            status,
            assignee: None,
            description: None,
            origin: context.actor.as_str(),
        },
    ) {
        Ok(ticket) => {
            let mut filters = filters;
            filters.selected = Some(ticket.id.to_string());
            render_response(&context, &filters, None, &headers, StatusCode::OK)
        }
        Err(error) => bad_request(&error.message()),
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
    if ticket.status != status
        && let Err(error) = board.set(
            &id,
            SetFields {
                status: Edit::Set(status),
                ..SetFields::default()
            },
        )
    {
        return bad_request(&error.message());
    }
    render_response(&context, &filters, None, &headers, StatusCode::OK)
}
