//! The ticket sheet: one ticket drawn over the board it was opened from, the
//! per-field autosave write it posts, and the comment form.

use super::board::{FiltersView, LabelOption, board_with_sheet};
use super::*;

// MARK: - The ticket sheet

/// One status choice in the sheet's metadata row.
struct SheetStatusOption {
    value: &'static str,
    label: &'static str,
    selected: bool,
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
    /// The ticket's label, or empty: the baseline its select was drawn from.
    label: String,
    status_options: Vec<SheetStatusOption>,
    label_options: Vec<LabelOption>,
    description: String,
    /// The description drawn by the comments' renderer; none when empty.
    description_html: Option<askama::filters::Safe<String>>,
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
pub(super) fn civil_from_days(days: i64) -> (i64, i64, i64) {
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

fn sheet_label_options(ticket: &Ticket, labels: &[String]) -> Vec<LabelOption> {
    let current = ticket.label.as_deref();
    let mut options = vec![LabelOption {
        value: String::new(),
        label: "No label".to_string(),
        selected: current.is_none(),
    }];
    for label in labels {
        options.push(LabelOption {
            value: label.clone(),
            label: label.clone(),
            selected: current == Some(label.as_str()),
        });
    }
    if let Some(current) = current
        && !labels.iter().any(|label| label == current)
    {
        options.push(LabelOption {
            value: current.to_string(),
            label: current.to_string(),
            selected: true,
        });
    }
    options
}

fn render_sheet(
    context: &Context,
    id: &str,
    filters: &Filters,
    notice: Option<String>,
    comment_refusal: Option<String>,
) -> Result<SheetView, String> {
    let board = context.board();
    let (ticket, _) = board.ticket_snapshot(id).map_err(|error| error.message())?;
    let (comments, _) = board
        .comments_snapshot(id)
        .map_err(|error| error.message())?;
    // A ticket never changes project, so the label select offers its own
    // project's labels only.
    let (projects, _) = board.projects_snapshot().map_err(|error| error.message())?;
    let labels: Vec<String> = projects
        .iter()
        .find(|project| project.name == ticket.project)
        .map(|project| project.labels.clone())
        .unwrap_or_default();

    let description = ticket.description.clone().unwrap_or_default();
    let description_html = if description.trim().is_empty() {
        None
    } else {
        Some(askama::filters::Safe(markdown::document(
            &TicketMarkdown::blocks(Some(&description)),
        )))
    };

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
        id: ticket.id.to_string(),
        board_query: filters.query(),
        filters: FiltersView {
            project: filters.project.clone(),
            labels: filters.labels.clone(),
            assignee: filters.assignee.clone(),
            text: filters.text.clone(),
        },
        handle: sheet_handle(&ticket),
        origin: ticket.origin.clone(),
        link: ticket.link.clone(),
        title: ticket.title.clone(),
        status: ticket.status.as_str(),
        assignee: ticket.assignee.clone().unwrap_or_default(),
        label: ticket.label.clone().unwrap_or_default(),
        status_options,
        label_options: sheet_label_options(&ticket, &labels),
        description,
        description_html,
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
    context: &Context,
    id: &str,
    filters: &Filters,
    notice: Option<String>,
    comment_refusal: Option<String>,
    headers: &HeaderMap,
) -> Response {
    if let Some(refusal) = notice.as_ref().or(comment_refusal.as_ref()) {
        tracing::warn!(error = ?refusal, "sheet refused");
    }
    let view = match render_sheet(context, id, filters, notice, comment_refusal) {
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

/// One percent-encoded path segment, for a label name in a route.
pub(super) fn encode_component(value: &str) -> String {
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
    Query(pairs): Query<Pairs>,
    headers: HeaderMap,
) -> Response {
    let mut filters = Filters::parse(&pairs);
    // A link that names no project (a bookmark, an agent's URL) opens over the
    // ticket's own project, so the header's switcher names what is behind it.
    if filters.project.is_empty()
        && let Ok((ticket, _)) = context.board().ticket_snapshot(&id)
    {
        filters.project = ticket.project;
    }
    sheet_response(&context, &id, &filters, None, None, &headers)
}

// MARK: - The field autosave

/// The fields the sheet saves one at a time.
#[derive(Clone, Copy)]
enum SheetField {
    Title,
    Status,
    Assignee,
    Label,
    Description,
}

impl SheetField {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "title" => Some(Self::Title),
            "status" => Some(Self::Status),
            "assignee" => Some(Self::Assignee),
            "label" => Some(Self::Label),
            "description" => Some(Self::Description),
            _ => None,
        }
    }

    /// The value as the write would store it, so a `was` and the stored value
    /// compare equal when the write would have changed nothing between them.
    fn normalise(self, raw: &str) -> String {
        match self {
            Self::Title => raw
                .replace("\r\n", " ")
                .replace(['\r', '\n'], " ")
                .trim()
                .to_string(),
            Self::Status => Status::parse(raw)
                .map(|status| status.as_str().to_string())
                .unwrap_or_else(|_| raw.trim().to_string()),
            Self::Assignee => {
                crate::domain::board::normalize_assignee(Some(raw)).unwrap_or_default()
            }
            Self::Label => raw.trim().to_string(),
            Self::Description => raw.replace("\r\n", "\n").trim().to_string(),
        }
    }

    /// What the ticket holds for this field, normalised.
    fn stored(self, ticket: &Ticket) -> String {
        match self {
            Self::Title => self.normalise(&ticket.title),
            Self::Status => ticket.status.as_str().to_string(),
            Self::Assignee => self.normalise(ticket.assignee.as_deref().unwrap_or_default()),
            Self::Label => self.normalise(ticket.label.as_deref().unwrap_or_default()),
            Self::Description => self.normalise(ticket.description.as_deref().unwrap_or_default()),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Status => "status",
            Self::Assignee => "assignee",
            Self::Label => "label",
            Self::Description => "description",
        }
    }
}

/// The answer to one field write: a single element the script reads, never the
/// board, so nothing is swapped under the caret.
#[derive(Template)]
#[template(path = "field_result.html")]
struct FieldResult {
    field: &'static str,
    state: &'static str,
    was: String,
    theirs: Option<String>,
    reason: Option<String>,
}

fn field_answer(result: FieldResult) -> Response {
    match result.render() {
        Ok(html) => Html(html).into_response(),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("the answer could not be drawn: {error}\n"),
        )
            .into_response(),
    }
}

/// One field of one ticket, written if the field still holds what the sheet
/// last showed (`was`). The compare is per field: a write to another field or
/// another ticket never makes this one conflict.
pub(super) async fn save_field(
    context: Context,
    Path(id): Path<String>,
    form: Result<Form<Pairs>, FormRejection>,
) -> Response {
    let Ok(Form(pairs)) = form else {
        return bad_request("the form could not be read");
    };
    let params = pairs_map(&pairs);
    let Some(kind) = SheetField::parse(field(&params, "field")) else {
        return bad_request("the field is not one the sheet saves");
    };
    let Some(was) = params.get("was") else {
        return bad_request("was is missing");
    };
    let value = field(&params, "value");
    let board = context.board();
    let ticket = match board.ticket_snapshot(&id) {
        Ok((ticket, _)) => ticket,
        Err(error) => return bad_request(&error.message()),
    };
    let stored = kind.stored(&ticket);
    if kind.normalise(was) != stored {
        tracing::info!(
            actor = ?context.actor.as_str(),
            id = ?id,
            field = kind.name(),
            revision = context.revision(),
            "field save conflicted"
        );
        return field_answer(FieldResult {
            field: kind.name(),
            state: "conflict",
            was: was.clone(),
            theirs: Some(stored),
            reason: None,
        });
    }
    let outcome = match kind {
        SheetField::Title => {
            let title = kind.normalise(value);
            board.set(
                &id,
                SetFields {
                    title: Edit::Set(&title),
                    ..SetFields::default()
                },
            )
        }
        SheetField::Status => Status::parse(value).and_then(|status| {
            board.set(
                &id,
                SetFields {
                    status: Edit::Set(status),
                    ..SetFields::default()
                },
            )
        }),
        SheetField::Assignee => board.set(
            &id,
            SetFields {
                assignee: Edit::Set(Some(value)),
                ..SetFields::default()
            },
        ),
        SheetField::Description => board.set(
            &id,
            SetFields {
                description: Edit::Set(Some(value)),
                ..SetFields::default()
            },
        ),
        SheetField::Label => {
            let label = kind.normalise(value);
            let label = (!label.is_empty()).then_some(label);
            board.move_ticket(&id, label.as_deref(), usize::MAX)
        }
    };
    match outcome {
        Ok(saved) => {
            tracing::info!(
                actor = ?context.actor.as_str(),
                id = ?id,
                field = kind.name(),
                revision = context.revision(),
                "field saved"
            );
            field_answer(FieldResult {
                field: kind.name(),
                state: "saved",
                was: kind.stored(&saved),
                theirs: None,
                reason: None,
            })
        }
        Err(error) => {
            tracing::warn!(
                actor = ?context.actor.as_str(),
                id = ?id,
                field = kind.name(),
                "field save refused"
            );
            field_answer(FieldResult {
                field: kind.name(),
                state: "refused",
                was: was.clone(),
                theirs: None,
                reason: Some(error.message()),
            })
        }
    }
}

pub(super) async fn add_comment(
    context: Context,
    Path(id): Path<String>,
    headers: HeaderMap,
    form: Result<Form<Pairs>, FormRejection>,
) -> Response {
    let Ok(Form(pairs)) = form else {
        return bad_request("the form could not be read");
    };
    let params = pairs_map(&pairs);
    let filters = Filters::parse_board_fields(&pairs);
    let body = field(&params, "body");
    if body.trim().is_empty() {
        return sheet_response(
            &context,
            &id,
            &filters,
            None,
            Some(crate::domain::DomainError::EmptyCommentBody.message()),
            &headers,
        );
    }
    match context.board().comment_write(&id, body) {
        Ok(_) => {
            tracing::info!(
                actor = ?context.actor.as_str(),
                id = ?id,
                revision = context.revision(),
                "comment added"
            );
            sheet_response(&context, &id, &filters, None, None, &headers)
        }
        Err(error) => sheet_response(
            &context,
            &id,
            &filters,
            Some(error.message()),
            None,
            &headers,
        ),
    }
}
