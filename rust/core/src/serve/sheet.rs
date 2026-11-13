//! The ticket sheet: one ticket drawn over the board it was opened from, the
//! writes its forms send, and the lane rows a board write redraws.

use super::board::{FiltersView, LaneOption, board_with_sheet};
use super::*;

// MARK: - The ticket sheet

/// One status choice in the sheet's metadata row.
struct SheetStatusOption {
    value: &'static str,
    label: &'static str,
    selected: bool,
}

/// One lane the sheet may rename.
struct LaneRow {
    name: String,
    encoded: String,
}

/// One comment, its markdown already drawn.
struct SheetComment {
    author: String,
    time: String,
    html: askama::filters::Safe<String>,
}

/// One dependency edge, drawn as a link to the other ticket.
struct EdgeView {
    id: String,
    reference: String,
    title: String,
    status: &'static str,
}

#[derive(Template)]
#[template(path = "sheet.html")]
struct SheetView {
    revision: i64,
    id: String,
    /// The filters the board behind the sheet was drawn with, as a query
    /// string: every write form carries them, so an answer redraws that board
    /// and the close link lands on it.
    board_query: String,
    /// The same filters as fields, for the hidden inputs every sheet form
    /// carries back.
    filters: FiltersView,
    handle: String,
    origin: String,
    link: Option<String>,
    title: String,
    status: &'static str,
    assignee: String,
    status_options: Vec<SheetStatusOption>,
    lane_options: Vec<LaneOption>,
    lane_rows: Vec<LaneRow>,
    description_empty: bool,
    description_html: askama::filters::Safe<String>,
    description_source: String,
    description_refusal: Option<String>,
    comment_refusal: Option<String>,
    notice: Option<String>,
    comments: Vec<SheetComment>,
    blocked_by: Vec<EdgeView>,
    blocks: Vec<EdgeView>,
    blocked_reason: Option<String>,
}

/// The UTC stamp a comment header draws: whole seconds since the epoch, as the
/// date and minute the store wrote, never an invented zone.
fn utc_stamp(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        rest / 3_600,
        (rest % 3_600) / 60
    )
}

/// Howard Hinnant's `civil_from_days`: the date a day count from 1970-01-01
/// names, proleptic Gregorian.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn comment_author(author: &str) -> String {
    let author = author.trim();
    if author.is_empty() {
        "anonymous".to_string()
    } else if author == "human" {
        "human".to_string()
    } else {
        format!("@{author}")
    }
}

fn edge_view(edge: &crate::domain::TicketBlock) -> EdgeView {
    EdgeView {
        id: edge.id.to_string(),
        reference: edge
            .reference
            .clone()
            .unwrap_or_else(|| "no ref".to_string()),
        title: edge.title.clone(),
        status: edge.status.display_name(),
    }
}

pub(super) fn sheet_handle(ticket: &Ticket) -> String {
    if let Some(reference) = &ticket.reference {
        return reference.clone();
    }
    match ticket.number {
        Some(number) => format!("#{number}"),
        None => "#?".to_string(),
    }
}

fn sheet_lane_options(ticket: &Ticket, lanes: &[String]) -> Vec<LaneOption> {
    let current = ticket.lane.as_deref();
    let mut options = vec![LaneOption {
        value: String::new(),
        label: "No lane".to_string(),
        selected: current.is_none(),
    }];
    for lane in lanes {
        options.push(LaneOption {
            value: lane.clone(),
            label: lane.clone(),
            selected: current == Some(lane.as_str()),
        });
    }
    if let Some(current) = current
        && !lanes.iter().any(|lane| lane == current)
    {
        options.push(LaneOption {
            value: current.to_string(),
            label: current.to_string(),
            selected: true,
        });
    }
    options
}

fn render_sheet(
    context: &SessionContext,
    id: &str,
    filters: &Filters,
    notice: Option<String>,
    description_refusal: Option<String>,
    comment_refusal: Option<String>,
) -> Result<SheetView, String> {
    let board = context.board();
    let (ticket, revision) = board.ticket_snapshot(id).map_err(|error| error.message())?;
    let (comments, _) = board
        .comments_snapshot(id)
        .map_err(|error| error.message())?;
    let (lanes, _) = board.lanes_snapshot().map_err(|error| error.message())?;

    let description_source = ticket.description.clone().unwrap_or_default();
    let description_blocks = TicketMarkdown::blocks(ticket.description.as_deref());
    let description_html = markdown::document(&description_blocks);

    let comments = comments
        .into_iter()
        .map(|comment| SheetComment {
            author: comment_author(&comment.author),
            time: utc_stamp(comment.at),
            html: askama::filters::Safe(markdown::document(&TicketMarkdown::blocks(Some(
                &comment.body,
            )))),
        })
        .collect();

    let status_options = Status::WORKFLOW
        .iter()
        .map(|status| SheetStatusOption {
            value: status.as_str(),
            label: status.display_name(),
            selected: *status == ticket.status,
        })
        .collect();

    Ok(SheetView {
        revision,
        id: ticket.id.to_string(),
        board_query: filters.query(),
        filters: FiltersView {
            project: filters.project.clone(),
            show_all: filters.show_all,
            lane: filters.lane.clone(),
            assignee: filters.assignee.clone(),
            text: filters.text.clone(),
        },
        handle: sheet_handle(&ticket),
        origin: ticket.origin.clone(),
        link: ticket.link.clone(),
        title: ticket.title.clone(),
        status: ticket.status.as_str(),
        assignee: ticket.assignee.clone().unwrap_or_default(),
        status_options,
        lane_options: sheet_lane_options(&ticket, &lanes),
        lane_rows: lanes
            .iter()
            .map(|lane| LaneRow {
                name: lane.clone(),
                encoded: encode_component(lane),
            })
            .collect(),
        description_empty: description_blocks.is_empty(),
        description_html: askama::filters::Safe(description_html),
        description_source,
        description_refusal,
        comment_refusal,
        notice,
        comments,
        blocked_by: ticket.blocked_by.iter().map(edge_view).collect(),
        blocks: ticket.blocks.iter().map(edge_view).collect(),
        blocked_reason: ticket.blocked_reason(),
    })
}

/// The sheet as an htmx partial or as a full page: the board is always behind
/// it, so the human can dismiss back to the face they came from. The filters
/// ride the sheet's own request (the query on a GET, the hidden fields on a
/// write form), so the board behind is the one the sheet was opened over.
fn sheet_response(
    context: &SessionContext,
    id: &str,
    filters: &Filters,
    notice: Option<String>,
    description_refusal: Option<String>,
    comment_refusal: Option<String>,
    headers: &HeaderMap,
) -> Response {
    let view = match render_sheet(
        context,
        id,
        filters,
        notice,
        description_refusal,
        comment_refusal,
    ) {
        Ok(view) => view,
        Err(error) => return bad_request(&error),
    };
    let html = match view.render() {
        Ok(html) => html,
        Err(error) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("the sheet could not be drawn: {error}\n"),
            )
                .into_response();
        }
    };
    board_with_sheet(
        context,
        filters,
        html,
        filters.url(),
        headers,
        StatusCode::OK,
    )
}

/// One percent-encoded path segment, for a lane name in a route.
fn encode_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

pub(super) async fn ticket_page(
    context: Context,
    Path(id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    let filters = Filters::parse(&params);
    sheet_response(&context, &id, &filters, None, None, None, &headers)
}

pub(super) async fn save_ticket(
    context: Context,
    Path(id): Path<String>,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    let Some(params) = form_params(form) else {
        return bad_request("the form could not be read");
    };
    let filters = Filters::parse_board_fields(&params);
    let expected = match expected_revision(&params) {
        Ok(revision) => revision,
        Err(message) => return bad_request(&message),
    };
    if expected != context.revision() {
        return sheet_response(
            &context,
            &id,
            &filters,
            Some(CONFLICT_NOTICE.to_string()),
            None,
            None,
            &headers,
        );
    }
    let title = field(&params, "title");
    if title.trim().is_empty() {
        return sheet_response(
            &context,
            &id,
            &filters,
            Some(crate::domain::DomainError::EmptyTitle.message()),
            None,
            None,
            &headers,
        );
    }
    let status = match Status::parse(field(&params, "status")) {
        Ok(status) => status,
        Err(error) => {
            return sheet_response(
                &context,
                &id,
                &filters,
                Some(error.message()),
                None,
                None,
                &headers,
            );
        }
    };
    let assignee = field(&params, "assignee");
    let outcome = context.board().set(
        &id,
        SetFields {
            title: Edit::Set(title),
            status: Edit::Set(status),
            assignee: Edit::Set(Some(assignee)),
            ..SetFields::default()
        },
    );
    match outcome {
        Ok(_) => sheet_response(&context, &id, &filters, None, None, None, &headers),
        Err(error) => sheet_response(
            &context,
            &id,
            &filters,
            Some(error.message()),
            None,
            None,
            &headers,
        ),
    }
}

pub(super) async fn append_description(
    context: Context,
    Path(id): Path<String>,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    let Some(params) = form_params(form) else {
        return bad_request("the form could not be read");
    };
    let filters = Filters::parse_board_fields(&params);
    let expected = match expected_revision(&params) {
        Ok(revision) => revision,
        Err(message) => return bad_request(&message),
    };
    if expected != context.revision() {
        return sheet_response(
            &context,
            &id,
            &filters,
            Some(CONFLICT_NOTICE.to_string()),
            None,
            None,
            &headers,
        );
    }
    let addition = field(&params, "addition");
    if addition.trim().is_empty() {
        return sheet_response(
            &context,
            &id,
            &filters,
            None,
            Some(crate::domain::DomainError::EmptyDescriptionBody.message()),
            None,
            &headers,
        );
    }
    match context.board().set(
        &id,
        SetFields {
            append_description: Edit::Set(addition),
            ..SetFields::default()
        },
    ) {
        Ok(_) => sheet_response(&context, &id, &filters, None, None, None, &headers),
        Err(error) => sheet_response(
            &context,
            &id,
            &filters,
            None,
            Some(error.message()),
            None,
            &headers,
        ),
    }
}

pub(super) async fn add_comment(
    context: Context,
    Path(id): Path<String>,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    let Some(params) = form_params(form) else {
        return bad_request("the form could not be read");
    };
    let filters = Filters::parse_board_fields(&params);
    let body = field(&params, "body");
    if body.trim().is_empty() {
        return sheet_response(
            &context,
            &id,
            &filters,
            None,
            None,
            Some(crate::domain::DomainError::EmptyCommentBody.message()),
            &headers,
        );
    }
    match context.board().comment_write(&id, body) {
        Ok(_) => sheet_response(&context, &id, &filters, None, None, None, &headers),
        Err(error) => sheet_response(
            &context,
            &id,
            &filters,
            Some(error.message()),
            None,
            None,
            &headers,
        ),
    }
}

pub(super) async fn move_to_lane(
    context: Context,
    Path(id): Path<String>,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    let Some(params) = form_params(form) else {
        return bad_request("the form could not be read");
    };
    let filters = Filters::parse_board_fields(&params);
    let expected = match expected_revision(&params) {
        Ok(revision) => revision,
        Err(message) => return bad_request(&message),
    };
    if expected != context.revision() {
        return sheet_response(
            &context,
            &id,
            &filters,
            Some(CONFLICT_NOTICE.to_string()),
            None,
            None,
            &headers,
        );
    }
    let lane = field(&params, "lane");
    let lane = if lane.trim().is_empty() {
        None
    } else {
        Some(lane.to_string())
    };
    match context
        .board()
        .move_ticket(&id, lane.as_deref(), usize::MAX)
    {
        Ok(_) => sheet_response(&context, &id, &filters, None, None, None, &headers),
        Err(error) => sheet_response(
            &context,
            &id,
            &filters,
            Some(error.message()),
            None,
            None,
            &headers,
        ),
    }
}

/// The ticket a lane write re-draws; a form without one has no sheet to answer.
fn lane_ticket(params: &HashMap<String, String>) -> String {
    field(params, "ticket").trim().to_string()
}

pub(super) async fn add_lane(
    context: Context,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    let Some(params) = form_params(form) else {
        return bad_request("the form could not be read");
    };
    let filters = Filters::parse_board_fields(&params);
    let id = lane_ticket(&params);
    if id.is_empty() {
        return bad_request("a lane write names the ticket it was drawn from");
    }
    let abbr = field(&params, "abbr").trim();
    let key = if abbr.is_empty() { None } else { Some(abbr) };
    let outcome = context.board().add_lane(field(&params, "name"), key, None);
    match outcome {
        Ok(_) => sheet_response(&context, &id, &filters, None, None, None, &headers),
        Err(error) => sheet_response(
            &context,
            &id,
            &filters,
            Some(error.message()),
            None,
            None,
            &headers,
        ),
    }
}

pub(super) async fn rename_lane(
    context: Context,
    Path(name): Path<String>,
    headers: HeaderMap,
    form: Result<Form<HashMap<String, String>>, FormRejection>,
) -> Response {
    let Some(params) = form_params(form) else {
        return bad_request("the form could not be read");
    };
    let filters = Filters::parse_board_fields(&params);
    let id = lane_ticket(&params);
    if id.is_empty() {
        return bad_request("a lane write names the ticket it was drawn from");
    }
    let abbr = field(&params, "abbr").trim();
    let key = if abbr.is_empty() { None } else { Some(abbr) };
    let outcome = context
        .board()
        .rename_lane(&name, field(&params, "to"), key, None);
    match outcome {
        Ok(_) => sheet_response(&context, &id, &filters, None, None, None, &headers),
        Err(error) => sheet_response(
            &context,
            &id,
            &filters,
            Some(error.message()),
            None,
            None,
            &headers,
        ),
    }
}
