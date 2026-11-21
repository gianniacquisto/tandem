//! The dispatch behind `tandem serve`'s `POST /mcp` (ADR 0047): one typed
//! [`Method`] in, a [`Reply`] or a [`WireError`] out.
//!
//! [`execute`] is the one dispatch (ADR 0009): every arm is the CLI verb's
//! domain call with the same defaults, and the MCP door ([`crate::mcp`]) reads a
//! tool call's arguments into a [`Method`] and calls it directly. No frame is
//! built or parsed here: the transport and the JSON-RPC envelope are the MCP
//! door's.

use crate::bikeshed::{Bikeshed, BoardNames, BoardProject};
use crate::diagnostics;
use crate::domain::{
    AddTicket, Board, DescriptionBody, DomainError, Edit, Status, TicketQuery, resolve_assignee,
    resolve_comment_author, resolve_origin,
};
use crate::store::{Actor, Store};
pub(crate) mod vault;

pub use self::vault::reconcile as reconcile_vault;

use crate::wire::rpc::{
    ArchivedReply, CommentListReply, CommentReply, DoctorReply, LabelDeletionReply, LabelListReply,
    Method, NoteBacklinksReply, NoteHealthReply, NoteMoveReply, NoteReadReply, NoteSearchReply,
    NoteTreeReply, NoteWriteReply, ProjectEntry, ProjectListReply, Reply, TicketListReply,
    TicketReply, WireError,
};
use crate::wire::{
    BacklinkView, CommentView, NoteContentView, NoteMoveView, NoteNodeView, NotePathView,
    NoteWriteView, SearchHitView, TicketDetail, TicketSummary,
};

/// The most a request body may carry. A description is text a human wrote; a
/// megabyte is far past any of them, and the cap keeps an untrusted writer from
/// growing this process's heap one line at a time. `tandem serve` applies it as
/// the body limit on every route.
pub const MAX_FRAME: usize = 4 * 1024 * 1024;

// MARK: - Dispatch

/// One method through the domain. Every arm is the CLI verb's call, with the
/// same defaults: the wire adds no rule of its own.
pub fn execute(method: Method, store: &Store, actor: &Actor) -> Result<Reply, WireError> {
    let board = Board::with_actor(store.clone(), actor.clone());
    match method {
        Method::TicketList(params) => {
            let assignee = resolve_assignee(params.assignee.as_deref(), actor).map_err(domain)?;
            let query = TicketQuery::parse(
                params.filter.as_deref(),
                params.text.as_deref(),
                params.status.as_deref(),
                assignee.as_deref(),
                params.project.as_deref(),
                params.label.as_deref(),
                params.archived,
                params.blocked,
            )
            .map_err(domain)?;
            let (tickets, revision) = board.list_snapshot(&query).map_err(domain)?;
            let matched = tickets.len();
            let cut = params
                .limit
                .map(|limit| limit.max(0) as usize)
                .filter(|limit| *limit < matched);
            Ok(Reply::TicketList(TicketListReply {
                revision,
                tickets: tickets
                    .iter()
                    .take(cut.unwrap_or(matched))
                    .map(TicketSummary::new)
                    .collect(),
                total: cut.map(|_| matched),
            }))
        }
        Method::TicketShow(params) => {
            let (ticket, revision) = board.ticket_snapshot(named(&params.id)).map_err(domain)?;
            Ok(Reply::Ticket(TicketReply {
                revision,
                ticket: TicketDetail::new(&ticket),
            }))
        }
        Method::TicketAdd(params) => {
            let status = match params.status.as_deref() {
                Some(raw) => Status::parse(raw).map_err(domain)?,
                None => Status::Backlog,
            };
            let assignee = resolve_assignee(params.assignee.as_deref(), actor).map_err(domain)?;
            let origin = resolve_origin(params.origin.as_deref(), actor).map_err(domain)?;
            let description = match params.description.as_deref() {
                None => None,
                Some(raw) => Some(DescriptionBody::required(raw).map_err(domain)?),
            };
            let ticket = board
                .add(
                    params.title.as_deref().unwrap_or(""),
                    AddTicket {
                        project: params.project.as_deref(),
                        label: params.label.as_deref(),
                        status,
                        assignee: assignee.as_deref(),
                        description: description.as_deref(),
                        origin: &origin,
                    },
                )
                .map_err(domain)?;
            Ok(ticket_receipt(&ticket, store))
        }
        Method::TicketSet(params) => {
            let status = match params.status.as_deref() {
                Some(raw) => Edit::Set(Status::parse(raw).map_err(domain)?),
                None => Edit::Unchanged,
            };
            let resolved_assignee =
                resolve_assignee(params.assignee.as_deref(), actor).map_err(domain)?;
            let assignee = match params.assignee.as_deref() {
                Some(_) => Edit::Set(resolved_assignee.as_deref()),
                None => Edit::Unchanged,
            };
            let description = match params.description.as_deref() {
                Some(raw) => Edit::Set(Some(raw)),
                None => Edit::Unchanged,
            };
            let append_description = match params.append_description.as_deref() {
                Some(raw) => Edit::Set(raw),
                None => Edit::Unchanged,
            };
            let ticket = board
                .set(
                    named(&params.id),
                    crate::domain::SetFields {
                        title: match params.title.as_deref() {
                            Some(title) => Edit::Set(title),
                            None => Edit::Unchanged,
                        },
                        status,
                        assignee,
                        description,
                        append_description,
                        archived: Edit::Unchanged,
                    },
                )
                .map_err(domain)?;
            Ok(ticket_receipt(&ticket, store))
        }
        Method::TicketMove(params) => {
            let at = match params.index {
                None => usize::MAX,
                Some(index) => index.max(0) as usize,
            };
            let ticket = board
                .move_ticket(named(&params.id), params.label.as_deref(), at)
                .map_err(domain)?;
            Ok(ticket_receipt(&ticket, store))
        }
        Method::TicketBlock(params) => {
            let ticket = board
                .block(named(&params.id), params.on.as_deref().unwrap_or(""))
                .map_err(domain)?;
            Ok(ticket_receipt(&ticket, store))
        }
        Method::TicketUnblock(params) => {
            let ticket = board
                .unblock(named(&params.id), params.on.as_deref())
                .map_err(domain)?;
            Ok(ticket_receipt(&ticket, store))
        }
        Method::TicketComplete(params) => {
            let ticket = board.complete(named(&params.id)).map_err(domain)?;
            Ok(ticket_receipt(&ticket, store))
        }
        Method::TicketArchive(params) => match params.id.as_deref() {
            Some(id) => {
                let ticket = board.archive(id).map_err(domain)?;
                Ok(ticket_receipt(&ticket, store))
            }
            None => {
                let archived = board.archive_done().map_err(domain)?;
                Ok(Reply::Archived(ArchivedReply {
                    revision: store.last_revision(),
                    archived,
                }))
            }
        },
        Method::TicketCommentAdd(params) => {
            let author = resolve_comment_author(params.author.as_deref(), actor);
            let comment = board
                .append_comment(
                    named(&params.id),
                    author,
                    params.body.as_deref().unwrap_or(""),
                )
                .map_err(domain)?;
            Ok(Reply::Comment(CommentReply {
                revision: store.last_revision(),
                comment: CommentView::new(&comment),
            }))
        }
        Method::TicketCommentList(params) => {
            let (comments, revision) =
                board.comments_snapshot(named(&params.id)).map_err(domain)?;
            Ok(Reply::CommentList(CommentListReply {
                revision,
                comments: comments.iter().map(CommentView::new).collect(),
            }))
        }
        Method::LabelList(_) => {
            let (labels, revision) = board.labels_snapshot().map_err(domain)?;
            Ok(Reply::LabelList(LabelListReply {
                revision,
                labels,
                note: None,
            }))
        }
        Method::LabelAdd(params) => {
            let name = params.name.as_deref().unwrap_or("");
            let labels = board
                .add_label(name, params.project.as_deref())
                .map_err(domain)?;
            let note = vault::added(store, &board, params.project.as_deref(), Some(name.trim()));
            Ok(Reply::LabelList(LabelListReply {
                revision: store.last_revision(),
                labels,
                note,
            }))
        }
        Method::LabelRename(params) => {
            let (from, to) = (
                params.from.as_deref().unwrap_or(""),
                params.to.as_deref().unwrap_or(""),
            );
            let before = vault::names(&board);
            let labels = board
                .rename_label(from, to, params.project.as_deref())
                .map_err(domain)?;
            let note = vault::label_renamed(store, &board, &before, from, to);
            Ok(Reply::LabelList(LabelListReply {
                revision: store.last_revision(),
                labels,
                note,
            }))
        }
        Method::LabelDelete(params) => {
            let deletion = board
                .delete_label(params.name.as_deref().unwrap_or(""))
                .map_err(domain)?;
            Ok(Reply::LabelDeletion(LabelDeletionReply {
                revision: store.last_revision(),
                labels: deletion.labels,
                moved: deletion.moved,
                note: None,
            }))
        }
        Method::LabelMove(params) => {
            let at = match params.index {
                Some(index) if index >= 0 => index as usize,
                Some(_) => 0,
                None => usize::MAX,
            };
            let labels = board
                .move_label(params.name.as_deref().unwrap_or(""), at)
                .map_err(domain)?;
            Ok(Reply::LabelList(LabelListReply {
                revision: store.last_revision(),
                labels,
                note: None,
            }))
        }
        Method::ProjectList(_) => {
            let (projects, revision) = board.projects_snapshot().map_err(domain)?;
            Ok(Reply::ProjectList(ProjectListReply {
                revision,
                projects: project_entries(projects),
                note: None,
            }))
        }
        Method::ProjectAdd(params) => {
            let name = params.name.as_deref().unwrap_or("");
            let projects = board
                .add_project(name, params.abbr.as_deref())
                .map_err(domain)?;
            let note = vault::added(store, &board, Some(name.trim()), None);
            Ok(Reply::ProjectList(ProjectListReply {
                revision: store.last_revision(),
                projects: project_entries(projects),
                note,
            }))
        }
        Method::ProjectRename(params) => {
            let (from, to) = (
                params.from.as_deref().unwrap_or(""),
                params.to.as_deref().unwrap_or(""),
            );
            let projects = board
                .rename_project(from, to, params.abbr.as_deref())
                .map_err(domain)?;
            let note = vault::project_renamed(store, &board, from, to);
            Ok(Reply::ProjectList(ProjectListReply {
                revision: store.last_revision(),
                projects: project_entries(projects),
                note,
            }))
        }
        Method::ProjectDelete(params) => {
            let projects = board
                .delete_project(params.name.as_deref().unwrap_or(""))
                .map_err(domain)?;
            Ok(Reply::ProjectList(ProjectListReply {
                revision: store.last_revision(),
                projects: project_entries(projects),
                note: None,
            }))
        }
        Method::DoctorReport(_) => Ok(Reply::Doctor(DoctorReply {
            report: diagnostics::run(store.home()),
        })),
        Method::NoteTree(_) => {
            let bikeshed = Bikeshed::new(bikeshed_directory(store));
            Ok(Reply::NoteTree(NoteTreeReply {
                nodes: bikeshed.tree().iter().map(NoteNodeView::new).collect(),
            }))
        }
        Method::NoteRead(params) => {
            let path = named(&params.path);
            let content = open_bikeshed(store).read(path).map_err(domain)?.value;
            Ok(Reply::NoteRead(NoteReadReply {
                note: NoteContentView::new(path, &content),
            }))
        }
        Method::NoteWrite(params) => {
            let path = named(&params.path);
            // An absent text is not an empty note: the client sent no document,
            // and writing "" would clear a human's note. `text: ""` is the
            // explicit clear; a missing field is the refusal.
            let Some(text) = params.text.as_deref() else {
                return Err(WireError::domain("the write carries no text"));
            };
            let record = open_bikeshed(store)
                .write(path, text, params.expected_hash.as_deref())
                .map_err(domain)?
                .value;
            Ok(Reply::NoteWrite(NoteWriteReply {
                write: NoteWriteView::new(&record),
            }))
        }
        Method::NoteMove(params) => {
            let outcome = open_bikeshed(store)
                .move_into(named(&params.path), named(&params.into))
                .map_err(domain)?
                .value;
            Ok(Reply::NoteMove(NoteMoveReply {
                moved: NoteMoveView::new(&outcome),
            }))
        }
        Method::NoteCreate(params) => {
            let created = open_bikeshed(store)
                .create(named(&params.path))
                .map_err(domain)?
                .value;
            Ok(Reply::NotePath(NotePathView {
                path: created,
                note: None,
            }))
        }
        Method::NoteCreateFolder(params) => {
            let created = open_bikeshed(store)
                .create_folder(named(&params.path))
                .map_err(domain)?
                .value;
            Ok(Reply::NotePath(NotePathView {
                path: created,
                note: None,
            }))
        }
        Method::NoteHealth(_) => {
            // The one join point (ADR 0054): names from the board, folders from
            // the vault.
            let (projects, _) = board.projects_snapshot().map_err(domain)?;
            let names = BoardNames {
                projects: projects
                    .into_iter()
                    .map(|entry| BoardProject {
                        name: entry.name,
                        labels: entry.labels,
                    })
                    .collect(),
            };
            Ok(Reply::NoteHealth(NoteHealthReply {
                health: open_bikeshed(store).health(&names).map_err(domain)?.value,
            }))
        }
        Method::NoteSearch(params) => {
            let Some(query) = params.query.as_deref() else {
                return Err(WireError::domain("the search carries no query"));
            };
            let hits = open_bikeshed(store).search(query).map_err(domain)?.value;
            let matched = hits.len();
            let cut = params.limit.unwrap_or(NOTE_SEARCH_LIMIT).max(0) as usize;
            let cut = Some(cut).filter(|cut| *cut < matched);
            Ok(Reply::NoteSearch(NoteSearchReply {
                hits: hits
                    .iter()
                    .take(cut.unwrap_or(matched))
                    .map(SearchHitView::new)
                    .collect(),
                total: cut.map(|_| matched),
            }))
        }
        Method::NoteEdit(params) => {
            let (Some(old), Some(new)) = (params.old_text.as_deref(), params.new_text.as_deref())
            else {
                return Err(WireError::domain("the edit carries no oldText or newText"));
            };
            let record = open_bikeshed(store)
                .edit(
                    named(&params.path),
                    old,
                    new,
                    params.expected_hash.as_deref(),
                )
                .map_err(domain)?
                .value;
            Ok(Reply::NoteWrite(NoteWriteReply {
                write: NoteWriteView::new(&record),
            }))
        }
        Method::NoteBacklinks(params) => {
            let path = named(&params.path);
            let rows = open_bikeshed(store).backlinks(path).map_err(domain)?.value;
            Ok(Reply::NoteBacklinks(NoteBacklinksReply {
                path: path.to_string(),
                backlinks: rows.iter().map(BacklinkView::new).collect(),
            }))
        }
    }
}

/// The project rows a reply carries.
fn project_entries(projects: Vec<crate::domain::ProjectWithLabels>) -> Vec<ProjectEntry> {
    projects
        .into_iter()
        .map(|entry| ProjectEntry {
            name: entry.name,
            key: entry.key,
            labels: entry.labels,
        })
        .collect()
}

/// How many search rows an absent `limit` returns.
const NOTE_SEARCH_LIMIT: i64 = 20;

/// The bikeshed a home keeps: `<home>/bikeshed`, the one rule the CLI's note
/// verbs and the app's reads share.
fn bikeshed_directory(store: &Store) -> std::path::PathBuf {
    store.home().directory().join("bikeshed")
}

/// The vault the note methods read and write.
fn open_bikeshed(store: &Store) -> Bikeshed {
    Bikeshed::new(bikeshed_directory(store))
}

/// A handle the caller named; an absent one is the empty string the CLI passes,
/// so the domain's unknown-id refusal is the same answer on both doors.
fn named(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("")
}

/// A ticket receipt: the ticket a write answered and the revision that write
/// produced, read inside its transaction (`Store::last_revision`). A concurrent
/// writer cannot pair the ticket with a later number.
fn ticket_receipt(ticket: &crate::domain::Ticket, store: &Store) -> Reply {
    Reply::Ticket(TicketReply {
        revision: store.last_revision(),
        ticket: TicketDetail::new(ticket),
    })
}

/// A domain refusal, in the words the CLI prints on stderr.
fn domain(error: DomainError) -> WireError {
    WireError::domain(error.message())
}
