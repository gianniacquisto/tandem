//! The board: the ticket aggregate over the store adapter (`Operations.swift`).
//!
//! These are the operations both faces use, and they define the shared semantics so
//! the binary and the app cannot drift. The domain holds no SQL, no JSON and no argv:
//! it reads one [`Contents`], changes it, and hands back the difference for
//! [`Store::write`] to commit under one transaction and one [`Actor`].
//!
//! **Ordering.** The stored order is authoritative: a ticket's place in the ticket
//! vector is its place in the list, and `add` puts a new ticket on **top** of its
//! lane by default. Order is never recomputed from `created_at`. Lanes group the
//! list, and the list is kept grouped by lane (default first, then each declared
//! lane) so a reader can walk it straight through.
//!
//! Every id-taking operation resolves its handle through [`Contents::require_ticket`],
//! the store's one rule: a UUID, or the ref (`TAN-42`, case-insensitive on the key).
//! A second copy of that rule would be a second answer.

use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use crate::store::timestamp;
use crate::store::{Actor, NewComment, Store};

use super::comment::{Comment, CommentRead};
use super::contents::Contents;
use super::description::{self, DescriptionBody};
use super::edit::Edit;
use super::error::DomainError;
use super::lane::{Lane, LaneDeletion};
use super::query::{Filter, HUMAN, TicketQuery};
use super::ticket::{Status, Ticket};
use super::undo::CompletionUndo;

/// The fields a new ticket may be given, over the title it must have.
#[derive(Debug, Clone, Copy)]
pub struct AddTicket<'a> {
    pub lane: Option<&'a str>,
    pub status: Status,
    pub assignee: Option<&'a str>,
    pub description: Option<&'a str>,
    /// Who filed it. Written once and never changed (ADR 0020).
    pub origin: &'a str,
}

impl Default for AddTicket<'_> {
    fn default() -> Self {
        Self {
            lane: None,
            status: Status::Backlog,
            assignee: None,
            description: None,
            origin: HUMAN,
        }
    }
}

/// The fields `set` may change. Each is an [`Edit`], so "leave it alone" and
/// "clear it" are different instructions.
#[derive(Debug, Clone, Copy, Default)]
pub struct SetFields<'a> {
    pub title: Edit<&'a str>,
    pub status: Edit<Status>,
    pub assignee: Edit<Option<&'a str>>,
    pub description: Edit<Option<&'a str>>,
    pub archived: Edit<bool>,
}

/// The board, rooted at one store, writing as one actor.
#[derive(Debug, Clone)]
pub struct Board {
    store: Store,
    actor: Actor,
}

impl Board {
    /// A board that writes as the process's actor (`TANDEM_AGENT`, else the human).
    pub fn new(store: Store) -> Self {
        Self {
            store,
            actor: Actor::current(),
        }
    }

    /// A board that writes as `actor`, which is how the suite pins authorship
    /// without touching the environment.
    pub fn with_actor(store: Store, actor: Actor) -> Self {
        Self { store, actor }
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn actor(&self) -> &Actor {
        &self.actor
    }

    // MARK: - Reads

    /// `list(query)`: the tickets in view, in stored order.
    pub fn list(&self, query: &TicketQuery) -> Result<Vec<Ticket>, DomainError> {
        Ok(query.apply(&self.load()?.tickets))
    }

    /// `list(filter)`: the shipped three-value filter, sugar over [`TicketQuery`].
    pub fn list_filter(&self, filter: Filter) -> Result<Vec<Ticket>, DomainError> {
        self.list(&filter.query())
    }

    /// `list(query)` and the revision its rows are a snapshot of, read in one
    /// transaction: a listing cannot pair rows with a number a writer moved
    /// between the two statements (PRT-11).
    pub fn list_snapshot(&self, query: &TicketQuery) -> Result<(Vec<Ticket>, i64), DomainError> {
        let Some((rows, revision)) = self.store.board_snapshot()? else {
            return Ok((Vec::new(), 0));
        };
        let contents = Contents::from_rows(&rows)?;
        Ok((query.apply(&contents.tickets), revision))
    }

    /// The lane names in render order (the default area is implicit and unnamed).
    /// Includes any name that appears only on a ticket, so a face can draw it.
    pub fn lanes(&self) -> Result<Vec<String>, DomainError> {
        Ok(self.load()?.known_lanes())
    }

    /// `ticket(id)`: the ticket a handle names, or the failure every id-taking
    /// operation raises. A lock-free read.
    pub fn ticket(&self, handle: &str) -> Result<Ticket, DomainError> {
        let contents = self.load()?;
        let (_, index) = contents.require_ticket(handle)?;
        Ok(contents.tickets[index].clone())
    }

    /// `comments(id)`: the ticket's comments in append order, or an empty list.
    /// The ticket must exist; an unknown handle is the same failure `append`
    /// raises, with an empty list the answer only for a ticket that has none.
    pub fn comments(&self, handle: &str) -> Result<Vec<Comment>, DomainError> {
        let contents = self.load()?;
        let (id, _) = contents.require_ticket(handle)?;
        self.comment_rows(id)
    }

    /// The app's read path: the log, or why it could not be read. The two failures
    /// get their own words, because they ask the human for different things: one
    /// asks for a retry, the other for a repair.
    pub fn comment_read(&self, handle: &str) -> CommentRead {
        match self.comments(handle) {
            Ok(comments) => CommentRead::List(comments),
            Err(DomainError::Locked) => CommentRead::Busy(DomainError::Locked.message()),
            Err(error) => CommentRead::Failed(error.message()),
        }
    }

    // MARK: - Lanes

    /// `add_lane(name, key)`: append a new, empty lane. The name is trimmed; an
    /// empty or already-used name is rejected. An absent key is the first three
    /// alphanumerics of the name; a present one is uppercased and must be
    /// alphanumerics. Returns the lane list after the write.
    pub fn add_lane(&self, name: &str, key: Option<&str>) -> Result<Vec<String>, DomainError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(DomainError::EmptyLaneName);
        }
        let requested = key.map(super::lane::LaneKey::normalize).transpose()?;
        self.mutate(|contents| {
            if contents.known_lanes().iter().any(|known| known == name) {
                return Err(DomainError::DuplicateLane(name.to_string()));
            }
            let taken: Vec<String> = contents.lanes.iter().map(|lane| lane.key.clone()).collect();
            let key = match requested {
                Some(key) => key,
                None => super::lane::LaneKey::generate(name, &taken),
            };
            let key = unique_key(&key, contents)?;
            contents.lanes.push(Lane::new(name, key));
            Ok((contents.known_lanes(), true))
        })
    }

    /// `rename_lane(from, to, key)`: rename a lane in place, the lane list and
    /// every ticket that belongs to it, in one write. The key survives a rename by
    /// default; a key may move it only while the lane holds no numbered ticket.
    pub fn rename_lane(
        &self,
        from: &str,
        to: &str,
        key: Option<&str>,
    ) -> Result<Vec<String>, DomainError> {
        let from = from.trim();
        let to = to.trim();
        if to.is_empty() {
            return Err(DomainError::EmptyLaneName);
        }
        let requested = key.map(super::lane::LaneKey::normalize).transpose()?;
        self.mutate(|contents| {
            let Some(i) = contents.lanes.iter().position(|lane| lane.name == from) else {
                return Err(DomainError::UnknownLane(from.to_string()));
            };
            let current = contents.lanes[i].clone();
            let new_key = match requested {
                Some(requested) if requested != current.key => {
                    let refs: Vec<String> = contents
                        .tickets
                        .iter()
                        .filter(|ticket| ticket.lane.as_deref() == Some(from))
                        .filter(|ticket| ticket.number.is_some())
                        .filter_map(|ticket| ticket.reference.clone())
                        .collect();
                    if !refs.is_empty() {
                        return Err(DomainError::FrozenLaneKey {
                            key: current.key.clone(),
                            lane: from.to_string(),
                            refs,
                        });
                    }
                    Some(unique_key(&requested, contents)?)
                }
                _ => None,
            };
            if from == to {
                let Some(new_key) = new_key else {
                    return Ok((contents.known_lanes(), false));
                };
                contents.lanes[i].key = new_key;
                contents.normalize();
                return Ok((contents.known_lanes(), true));
            }
            if contents.known_lanes().iter().any(|known| known == to) {
                return Err(DomainError::DuplicateLane(to.to_string()));
            }
            contents.lanes[i].name = to.to_string();
            if let Some(new_key) = new_key {
                contents.lanes[i].key = new_key;
            }
            for ticket in &mut contents.tickets {
                if ticket.lane.as_deref() == Some(from) {
                    ticket.lane = Some(to.to_string());
                }
            }
            contents.normalize();
            Ok((contents.known_lanes(), true))
        })
    }

    /// `delete_lane(name)`: remove a lane and move its tickets to the default area.
    /// A lane is never a way to delete tickets; the moved tickets lose their
    /// numbers with the lane, because the default area carries no ref.
    pub fn delete_lane(&self, name: &str) -> Result<LaneDeletion, DomainError> {
        let name = name.trim();
        self.mutate(|contents| {
            let Some(i) = contents.lanes.iter().position(|lane| lane.name == name) else {
                return Err(DomainError::UnknownLane(name.to_string()));
            };
            contents.lanes.remove(i);
            let mut moved = 0;
            for ticket in &mut contents.tickets {
                if ticket.lane.as_deref() == Some(name) {
                    ticket.lane = None;
                    ticket.number = None;
                    ticket.reference = None;
                    moved += 1;
                }
            }
            contents.normalize();
            Ok((
                LaneDeletion {
                    lanes: contents.known_lanes(),
                    moved,
                },
                true,
            ))
        })
    }

    /// `move_lane(name, to)`: reorder the declared lanes. `index` is 0-based in the
    /// lane list and is clamped, so `usize::MAX` moves it to the end. Only the lane
    /// list changes: the groups travel whole.
    pub fn move_lane(&self, name: &str, to: usize) -> Result<Vec<String>, DomainError> {
        let name = name.trim();
        self.mutate(|contents| {
            let Some(from) = contents.lanes.iter().position(|lane| lane.name == name) else {
                return Err(DomainError::UnknownLane(name.to_string()));
            };
            let lane = contents.lanes.remove(from);
            let to = to.min(contents.lanes.len());
            contents.lanes.insert(to, lane);
            Ok((
                contents
                    .lanes
                    .iter()
                    .map(|lane| lane.name.clone())
                    .collect(),
                to != from,
            ))
        })
    }

    // MARK: - Add

    /// `add(title, ...)`: the title is trimmed; empty is rejected. The new ticket
    /// lands on the **top of its lane** (`None` is the default area); an unknown
    /// lane is rejected. The filer is stamped here and never touched again.
    pub fn add(&self, title: &str, fields: AddTicket<'_>) -> Result<Ticket, DomainError> {
        let title = title.trim();
        if title.is_empty() {
            return Err(DomainError::EmptyTitle);
        }
        let assignee = normalize_assignee(fields.assignee);
        let origin = normalize_origin(Some(fields.origin));
        let description = description::normalize(fields.description);
        self.mutate(|contents| {
            contents.validate_lane(fields.lane)?;
            let now = timestamp::unix_now();
            let mut ticket = Ticket::new(title);
            ticket.description = description.clone();
            ticket.assignee = assignee.clone();
            ticket.origin = origin.clone();
            ticket.lane = fields.lane.map(str::to_string);
            ticket.set_status(fields.status, now);
            insert_at_top(&mut ticket, contents);
            Ok((ticket, true))
        })
    }

    /// `add_from_note(selection, note_path, ...)`: file a selection from a note as a
    /// ticket, linked to that note.
    ///
    /// The title is the selection's first non-blank line, trimmed, with a leading
    /// task-list marker removed; the description is the whole selection. **The note
    /// is never touched**: the operation writes the store and nothing else.
    pub fn add_from_note(
        &self,
        selection: &str,
        note_path: &str,
        fields: AddTicket<'_>,
    ) -> Result<Ticket, DomainError> {
        let selection = selection.trim();
        let title = ticket_title(selection);
        if title.is_empty() {
            return Err(DomainError::EmptySelection);
        }
        let path = note_path.trim();
        if !is_note_link_path(path) {
            return Err(DomainError::InvalidNotePath(note_path.to_string()));
        }
        let assignee = normalize_assignee(fields.assignee);
        let origin = normalize_origin(Some(fields.origin));
        let description = description::normalize(Some(selection));
        self.mutate(|contents| {
            contents.validate_lane(fields.lane)?;
            let now = timestamp::unix_now();
            let mut ticket = Ticket::new(title.clone());
            ticket.description = description.clone();
            ticket.assignee = assignee.clone();
            ticket.origin = origin.clone();
            ticket.lane = fields.lane.map(str::to_string);
            ticket.link = Some(path.to_string());
            ticket.set_status(fields.status, now);
            // The same placement `add` gives it: the top of its lane.
            let start = contents.block_start(fields.lane);
            contents.tickets.insert(start, ticket.clone());
            Ok((ticket, true))
        })
    }

    // MARK: - Move and place

    /// `move(id, to_lane, at)`: place a ticket at a 0-based index within `to_lane`
    /// (`None` is the default area), moving it between lanes when needed. `index`
    /// is clamped into range, so `usize::MAX` appends.
    pub fn move_ticket(
        &self,
        handle: &str,
        to_lane: Option<&str>,
        at: usize,
    ) -> Result<Ticket, DomainError> {
        self.mutate(|contents| {
            let from = contents.require_ticket(handle)?.1;
            contents.validate_lane(to_lane)?;
            let mut moved = contents.tickets.remove(from);
            insert(&mut moved, to_lane, contents, at);
            Ok((moved, true))
        })
    }

    /// `place(id, lane, status, at)`: the board's drag. **One write** that sets the
    /// lane and the status together and puts the ticket at a 0-based index within
    /// the lane. A drop that lands the ticket exactly where it already was writes
    /// nothing.
    pub fn place(
        &self,
        handle: &str,
        lane: Option<&str>,
        status: Status,
        at: usize,
    ) -> Result<Ticket, DomainError> {
        self.mutate(|contents| {
            let from = contents.require_ticket(handle)?.1;
            contents.validate_lane(lane)?;
            let before = contents.tickets.clone();
            let mut moved = contents.tickets.remove(from);
            moved.set_status(status, timestamp::unix_now());
            insert(&mut moved, lane, contents, at);
            let changed = contents.tickets != before;
            Ok((moved, changed))
        })
    }

    // MARK: - Complete, reopen, undo

    /// `complete(id)`: idempotent. On a transition to done, `completed_at` is
    /// stamped. The stored order is never changed.
    pub fn complete(&self, handle: &str) -> Result<Ticket, DomainError> {
        self.mutate(|contents| {
            let index = contents.require_ticket(handle)?.1;
            let mut changed = false;
            if !contents.tickets[index].done() {
                contents.tickets[index].set_status(Status::Done, timestamp::unix_now());
                changed = true;
            }
            Ok((contents.tickets[index].clone(), changed))
        })
    }

    /// `reopen(id)`: the inverse of `complete`, idempotent. **A reopen lands in
    /// backlog**, because completing erased the stage it came from.
    pub fn reopen(&self, handle: &str) -> Result<Ticket, DomainError> {
        self.mutate(|contents| {
            let index = contents.require_ticket(handle)?.1;
            let mut changed = false;
            if contents.tickets[index].done() {
                contents.tickets[index].set_status(Status::Backlog, timestamp::unix_now());
                changed = true;
            }
            Ok((contents.tickets[index].clone(), changed))
        })
    }

    /// `undo(offer)`: take a completion back. This is `reopen` under the name the
    /// interaction has.
    pub fn undo(&self, offer: &CompletionUndo) -> Result<Ticket, DomainError> {
        self.reopen(&offer.id.to_string())
    }

    // MARK: - Set

    /// `set(id, ...)`: change the named fields in one write, leaving the rest
    /// untouched. A call that changes nothing writes nothing.
    pub fn set(&self, handle: &str, fields: SetFields<'_>) -> Result<Ticket, DomainError> {
        let new_title = match fields.title {
            Edit::Set(raw) => {
                let trimmed = raw.trim();
                if trimmed.is_empty() {
                    return Err(DomainError::EmptyTitle);
                }
                Some(trimmed.to_string())
            }
            Edit::Unchanged => None,
        };
        let new_assignee = fields.assignee.is_set().then(|| match fields.assignee {
            Edit::Set(value) => normalize_assignee(value),
            Edit::Unchanged => None,
        });
        let new_description = fields
            .description
            .is_set()
            .then(|| match fields.description {
                Edit::Set(value) => description::normalize(value),
                Edit::Unchanged => None,
            });
        self.mutate(|contents| {
            let index = contents.require_ticket(handle)?.1;
            let mut ticket = contents.tickets[index].clone();
            let mut changed = false;
            if let Some(new_title) = &new_title
                && &ticket.title != new_title
            {
                ticket.title = new_title.clone();
                changed = true;
            }
            if let Edit::Set(new_status) = fields.status
                && ticket.status != new_status
            {
                ticket.set_status(new_status, timestamp::unix_now());
                changed = true;
            }
            if let Some(new_assignee) = &new_assignee
                && &ticket.assignee != new_assignee
            {
                ticket.assignee = new_assignee.clone();
                changed = true;
            }
            if let Some(new_description) = &new_description
                && &ticket.description != new_description
            {
                ticket.description = new_description.clone();
                changed = true;
            }
            if let Edit::Set(archived) = fields.archived
                && ticket.archived != archived
            {
                ticket.archived = archived;
                changed = true;
            }
            contents.tickets[index] = ticket.clone();
            Ok((ticket, changed))
        })
    }

    /// `update(id, title)`: the GUI's inline rename, the same instruction as
    /// `set(id, title)`.
    pub fn update(&self, handle: &str, title: &str) -> Result<Ticket, DomainError> {
        self.set(
            handle,
            SetFields {
                title: Edit::Set(title),
                ..SetFields::default()
            },
        )
    }

    /// `answer(id, status, assignee, body)`: the human's answer recorded and the
    /// hold closed, a `## Answer` block appended to the description, the status the
    /// answer opens and the next assignee, all in **one** write.
    ///
    /// The write cannot half-land: recording the answer in a second call is what let
    /// answered tickets sit in the inbox wearing `human`. `assignee` is the next
    /// owner; `None` clears it, because the question is settled.
    pub fn answer(
        &self,
        handle: &str,
        status: Status,
        assignee: Option<&str>,
        body: &str,
        now: i64,
    ) -> Result<Ticket, DomainError> {
        let body = DescriptionBody::required(body)?;
        let who = normalize_assignee(assignee);
        let block = answer_block(&body, now);
        self.mutate(|contents| {
            let index = contents.require_ticket(handle)?.1;
            let mut ticket = contents.tickets[index].clone();
            ticket.description = Some(appending(&block, ticket.description.as_deref()));
            ticket.set_status(status, now);
            ticket.assignee = who.clone();
            contents.tickets[index] = ticket.clone();
            Ok((ticket, true))
        })
    }

    /// The promotion's link write: point a ticket at the note that was just
    /// written, **refusing a ticket that acquired a link since the promotion's own
    /// read**. The Swift promotion re-checks inside its transaction
    /// (`TicketToNote.swift:59-68`) because the note and the link are two writes
    /// with a store write between them: another face may have linked the ticket in
    /// that window, and that note is the human's prose now.
    pub(crate) fn link_from_note(&self, handle: &str, path: &str) -> Result<Ticket, DomainError> {
        self.mutate(|contents| {
            let index = contents.require_ticket(handle)?.1;
            if let Some(existing) = &contents.tickets[index].link {
                return Err(DomainError::AlreadyLinked {
                    path: existing.clone(),
                });
            }
            contents.tickets[index].link = Some(path.to_string());
            Ok((contents.tickets[index].clone(), true))
        })
    }

    /// `set_link(id, to)`: point a ticket at a bikeshed note, or clear the link.
    /// The link is trimmed; an empty value unlinks. An unchanged link writes
    /// nothing.
    pub fn set_link(&self, handle: &str, to: Option<&str>) -> Result<Ticket, DomainError> {
        let trimmed = to.map(str::trim);
        let link = match trimmed {
            Some(link) if !link.is_empty() => Some(link.to_string()),
            _ => None,
        };
        self.mutate(|contents| {
            let index = contents.require_ticket(handle)?.1;
            let changed = contents.tickets[index].link != link;
            if changed {
                contents.tickets[index].link = link.clone();
            }
            Ok((contents.tickets[index].clone(), changed))
        })
    }

    // MARK: - Block and unblock (TAN-232)

    /// `block(id, on)`: make one ticket wait on another, one edge. The keys are the
    /// two UUIDs, so a lane move, a lane delete or a key move changes the displayed
    /// ref and never the edge.
    ///
    /// **The refusals are the store's, and they are all at write time**: a ticket
    /// cannot wait on itself, on a done ticket (the block would lift on write), or
    /// on an archived one (the block could never be seen to lift), and an edge that
    /// would close a cycle names the loop it would make. An edge that is already
    /// there is a **no-op, not an error**.
    pub fn block(&self, handle: &str, on: &str) -> Result<Ticket, DomainError> {
        self.mutate(|contents| {
            let dependent = contents.require_ticket(handle)?;
            let blocker = contents.require_ticket(on)?;
            let dependent_name = display_name(&contents.tickets[dependent.1]);
            let blocker_name = display_name(&contents.tickets[blocker.1]);

            if dependent.0 == blocker.0 {
                return Err(DomainError::TicketBlocksItself {
                    reference: dependent_name,
                });
            }
            if contents.edges.contains(&(dependent.0, blocker.0)) {
                return Ok((contents.tickets[dependent.1].clone(), false));
            }
            let blocker_ticket = contents.tickets[blocker.1].clone();
            if blocker_ticket.done() {
                return Err(DomainError::TicketBlockerDone {
                    reference: blocker_name,
                });
            }
            if blocker_ticket.archived {
                return Err(DomainError::TicketBlockerArchived {
                    reference: blocker_name,
                });
            }
            if let Some(path) = cycle_path(contents, blocker.0, dependent.0) {
                let mut loop_names = vec![dependent_name];
                loop_names.extend(
                    path.iter()
                        .map(|index| display_name(&contents.tickets[*index])),
                );
                return Err(DomainError::TicketCycle { path: loop_names });
            }

            // The key, appended last: the order the edges were set is the order read.
            contents.edges.push((dependent.0, blocker.0));
            contents.refresh_blocked();
            Ok((contents.tickets[dependent.1].clone(), true))
        })
    }

    /// `unblock(id, on)`: release a ticket, one edge when a blocker is named, every
    /// edge when it is not. A missing edge is a no-op.
    pub fn unblock(&self, handle: &str, on: Option<&str>) -> Result<Ticket, DomainError> {
        self.mutate(|contents| {
            let dependent = contents.require_ticket(handle)?;
            let before = contents.edges.len();
            match on {
                Some(blocker) => {
                    let blocker = contents.require_ticket(blocker)?;
                    contents
                        .edges
                        .retain(|edge| *edge != (dependent.0, blocker.0));
                }
                None => contents.edges.retain(|edge| edge.0 != dependent.0),
            }
            let changed = contents.edges.len() != before;
            if changed {
                contents.refresh_blocked();
            }
            Ok((contents.tickets[dependent.1].clone(), changed))
        })
    }

    // MARK: - Archive and delete

    /// `archive_done()`: archive every done ticket in one write and return how many
    /// it archived. Already-archived ones are left alone. **Nothing is deleted.**
    pub fn archive_done(&self) -> Result<usize, DomainError> {
        self.mutate(|contents| {
            let mut archived = 0;
            for ticket in &mut contents.tickets {
                if ticket.done() && !ticket.archived {
                    ticket.archived = true;
                    archived += 1;
                }
            }
            Ok((archived, archived > 0))
        })
    }

    /// `archive(id)`: set one ticket aside. Idempotent. **Nothing is deleted.**
    pub fn archive(&self, handle: &str) -> Result<Ticket, DomainError> {
        self.mutate(|contents| {
            let index = contents.require_ticket(handle)?.1;
            let mut changed = false;
            if !contents.tickets[index].archived {
                contents.tickets[index].archived = true;
                changed = true;
            }
            Ok((contents.tickets[index].clone(), changed))
        })
    }

    /// `delete(id)`: remove a single ticket, and its edges on both ends in the same
    /// write. GUI-only: the agent face has no delete verb.
    pub fn delete(&self, handle: &str) -> Result<(), DomainError> {
        self.mutate(|contents| {
            let index = contents.require_ticket(handle)?.1;
            let removed = contents.tickets.remove(index);
            contents
                .edges
                .retain(|edge| edge.0 != removed.id && edge.1 != removed.id);
            contents.refresh_blocked();
            Ok(((), true))
        })
    }

    // MARK: - Comments

    /// `append(id, author, body)`: append one comment. The ticket must exist, and
    /// the existence check and the insert are **one transaction**. The body is
    /// trimmed; an empty result is rejected before anything is written. The author
    /// is trimmed; an empty result is the unnamed author `""`.
    pub fn append_comment(
        &self,
        handle: &str,
        author: &str,
        body: &str,
    ) -> Result<Comment, DomainError> {
        let text = body.trim();
        if text.is_empty() {
            return Err(DomainError::EmptyCommentBody);
        }
        let comment = Comment {
            author: author.trim().to_string(),
            at: timestamp::unix_now(),
            body: text.to_string(),
        };
        let handle = handle.to_string();
        let record = comment.clone();
        // A domain failure inside the transaction is not the store's to carry, so
        // it is parked here and handed back once the transaction has rolled back.
        let refused: std::cell::RefCell<Option<DomainError>> = std::cell::RefCell::new(None);
        let outcome = self.store.write(&self.actor, |write| {
            let rows = write.board_rows()?;
            let contents = match Contents::from_rows(&rows) {
                Ok(contents) => contents,
                Err(error) => {
                    *refused.borrow_mut() = Some(error);
                    return Err(crate::store::StoreError::Io(
                        "the board holds a row this build cannot read".to_string(),
                    ));
                }
            };
            let id = match contents.require_ticket(&handle) {
                Ok((id, _)) => id,
                Err(error) => {
                    *refused.borrow_mut() = Some(error);
                    return Err(crate::store::StoreError::Io(
                        "the ticket left the board before the comment landed".to_string(),
                    ));
                }
            };
            let ticket_id = id.to_string();
            write.insert_comment_authored(
                &record.author,
                NewComment {
                    ticket_id: &ticket_id,
                    at: &timestamp::encode(record.at),
                    body: &record.body,
                },
            )
        });
        if let Some(error) = refused.into_inner() {
            return Err(error);
        }
        outcome.map_err(map_store_error)?;
        Ok(comment)
    }

    /// `comment_write(id, body)`: the app's write path. The author is the human:
    /// the app is the human's face.
    pub fn comment_write(&self, handle: &str, body: &str) -> Result<Comment, DomainError> {
        self.append_comment(handle, HUMAN, body)
    }

    fn comment_rows(&self, id: Uuid) -> Result<Vec<Comment>, DomainError> {
        let rows = self
            .store
            .comment_rows(&id.to_string())?
            .unwrap_or_default();
        rows.into_iter()
            .map(|row| {
                let at = timestamp::decode(&row.at).ok_or_else(|| {
                    DomainError::Io(format!(
                        "a comment row carries an unreadable timestamp \"{}\"",
                        row.at
                    ))
                })?;
                Ok(Comment {
                    author: row.author,
                    at,
                    body: row.body,
                })
            })
            .collect()
    }

    // MARK: - The read-modify-write

    /// The board as one value, with the refs and the edges made true.
    fn load(&self) -> Result<Contents, DomainError> {
        let rows = self.store.board_rows()?.unwrap_or_default();
        Contents::from_rows(&rows)
    }

    /// One transaction: read the board **inside** it, let `body` change it, and
    /// write the difference when it says something changed. A no-op body writes no
    /// row, so a store's bytes are untouched by a call that changed nothing.
    ///
    /// **The read is inside the transaction**, the way the Swift store's `mutate`
    /// is (`Store.swift:130-175`): the write lock is held before the rows are read,
    /// so two writers over one home serialise their read-modify-write. A snapshot
    /// taken before the lock would hand the same lane number to both writers and a
    /// stale diff would write an untouched row back over a concurrent edit.
    ///
    /// A domain failure is not the store's to carry, so `body`'s failure is parked
    /// and handed back once the transaction has rolled back.
    fn mutate<T>(
        &self,
        body: impl FnOnce(&mut Contents) -> Result<(T, bool), DomainError>,
    ) -> Result<T, DomainError> {
        let refused: std::cell::RefCell<Option<DomainError>> = std::cell::RefCell::new(None);
        let outcome = self.store.write(&self.actor, |write| {
            let before = write.board_rows()?;
            let mut contents = match Contents::from_rows(&before) {
                Ok(contents) => contents,
                Err(error) => {
                    *refused.borrow_mut() = Some(error);
                    return Err(crate::store::StoreError::Io(
                        "the board holds a row this build cannot read".to_string(),
                    ));
                }
            };
            let (value, changed) = match body(&mut contents) {
                Ok(step) => step,
                Err(error) => {
                    *refused.borrow_mut() = Some(error);
                    return Err(crate::store::StoreError::Io(
                        "the board refused the write".to_string(),
                    ));
                }
            };
            if changed {
                contents.normalize();
                write.save_board(&before, &contents.to_rows())?;
            }
            Ok(value)
        });
        if let Some(error) = refused.into_inner() {
            return Err(error);
        }
        outcome.map_err(map_store_error)
    }
}

/// A store failure as the domain's two words.
fn map_store_error(error: crate::store::StoreError) -> DomainError {
    error.into()
}

/// The key `requested` must not be another lane's, compared uppercased.
fn unique_key(requested: &str, contents: &Contents) -> Result<String, DomainError> {
    let key = requested.to_uppercase();
    if contents
        .lanes
        .iter()
        .any(|lane| lane.key.eq_ignore_ascii_case(&key))
    {
        return Err(DomainError::DuplicateLaneKey(key));
    }
    Ok(key)
}

/// Insert a new ticket on top of its lane: it takes the lane's next number and its
/// ref, and the ticket comes back as the store will read it.
fn insert_at_top(ticket: &mut Ticket, contents: &mut Contents) {
    if let Some(lane) = ticket.lane.clone() {
        ticket.number = Some(contents.issue_number(&lane));
        let start = contents.block_start(Some(&lane));
        contents.tickets.insert(start, ticket.clone());
        contents.refresh_refs();
        ticket.reference = contents
            .tickets
            .iter()
            .find(|candidate| candidate.id == ticket.id)
            .and_then(|candidate| candidate.reference.clone());
    } else {
        let start = contents.block_start(None);
        contents.tickets.insert(start, ticket.clone());
    }
}

/// The one place a lane insertion's arithmetic lives: `index` is 0-based within
/// `lane` and clamped into range, so `usize::MAX` appends.
///
/// The ticket's **number** moves with its lane: a move into another lane takes the
/// target lane's next number, while a reorder inside the same lane keeps it. The
/// default area has none, so a move there clears it.
fn insert(ticket: &mut Ticket, lane: Option<&str>, contents: &mut Contents, at: usize) {
    let from = ticket.lane.clone();
    ticket.lane = lane.map(str::to_string);
    match lane {
        Some(lane) => {
            if from.as_deref() != Some(lane) || ticket.number.is_none() {
                ticket.number = Some(contents.issue_number(lane));
            }
        }
        None => {
            ticket.number = None;
            ticket.reference = None;
        }
    }
    let start = contents.block_start(lane);
    let count = contents
        .tickets
        .iter()
        .filter(|candidate| candidate.lane.as_deref() == lane)
        .count();
    contents
        .tickets
        .insert(start + at.min(count), ticket.clone());
    contents.refresh_refs();
    ticket.reference = contents
        .tickets
        .iter()
        .find(|candidate| candidate.id == ticket.id)
        .and_then(|candidate| candidate.reference.clone());
}

/// A ticket as a failure names it: its ref, or its title in the default area,
/// which carries neither a number nor a ref.
fn display_name(ticket: &Ticket) -> String {
    ticket
        .reference
        .clone()
        .unwrap_or_else(|| ticket.title.clone())
}

/// The chain of edges from `start` forward to `target`, as indices into
/// `contents.tickets`: the loop an edge into `target` would close, with `start`
/// first and `target` last. `None` when `target` is not reachable.
fn cycle_path(contents: &Contents, start: Uuid, target: Uuid) -> Option<Vec<usize>> {
    let mut next: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for (dependent, blocker) in &contents.edges {
        next.entry(*dependent).or_default().push(*blocker);
    }
    let mut visited: HashSet<Uuid> = HashSet::new();
    walk(contents, &next, start, target, &mut visited)
}

fn walk(
    contents: &Contents,
    next: &HashMap<Uuid, Vec<Uuid>>,
    current: Uuid,
    target: Uuid,
    visited: &mut HashSet<Uuid>,
) -> Option<Vec<usize>> {
    let index = contents
        .tickets
        .iter()
        .position(|ticket| ticket.id == current)?;
    if current == target {
        return Some(vec![index]);
    }
    if !visited.insert(current) {
        return None;
    }
    for edge in next.get(&current).into_iter().flatten() {
        if let Some(tail) = walk(contents, next, *edge, target, visited) {
            let mut path = vec![index];
            path.extend(tail);
            return Some(path);
        }
    }
    None
}

/// The answer block: `## Answer` and the day the answer was recorded (UTC), a
/// blank line, then the body. The heading is what `DescriptionBody::answered` reads
/// back, so the written form and the read agree by construction.
fn answer_block(body: &str, at: i64) -> String {
    let day = timestamp::encode(at);
    format!("## Answer \u{2014} {}\n\n{}", &day[..10], body)
}

/// A section appended to a description, separated by one blank line: a description
/// is written once and appended to, never rewritten.
fn appending(section: &str, description: Option<&str>) -> String {
    match description::normalize(description) {
        Some(existing) => format!("{existing}\n\n{section}"),
        None => section.to_string(),
    }
}

/// The name `self` stands for: the agent running the call, never the human.
fn self_name(actor: &Actor) -> Result<String, DomainError> {
    match actor {
        Actor::Agent(name) => Ok(name.clone()),
        Actor::Human => Err(DomainError::AgentNameUnset),
    }
}

/// `--assignee self` is the agent running this command, on the CLI and on the
/// wire alike: one rule, so the same value cannot mean two things. Any other
/// value passes through, including `""`, which selects the unowned tickets.
pub fn resolve_assignee(raw: Option<&str>, actor: &Actor) -> Result<Option<String>, DomainError> {
    match raw {
        Some("self") => Ok(Some(self_name(actor)?)),
        other => Ok(other.map(str::to_string)),
    }
}

/// `--origin self` is the agent; an absent value is the human. Any other value
/// passes through to the domain's spelling rule.
pub fn resolve_origin(raw: Option<&str>, actor: &Actor) -> Result<String, DomainError> {
    match raw {
        Some("self") => self_name(actor),
        Some(other) => Ok(other.to_string()),
        None => Ok(HUMAN.to_string()),
    }
}

/// The assignee's spelling rule: trimmed; empty means unassigned; the reserved word
/// `human` is stored lowercase so it is one name; anything else is an agent's name
/// exactly as given.
pub fn normalize_assignee(raw: Option<&str>) -> Option<String> {
    let name = raw?.trim();
    if name.is_empty() {
        return None;
    }
    if name.eq_ignore_ascii_case(HUMAN) {
        Some(HUMAN.to_string())
    } else {
        Some(name.to_string())
    }
}

/// The origin's spelling rule, the assignee's except there is no absent value: a
/// filer always exists, so `None` and empty both mean the human.
pub fn normalize_origin(raw: Option<&str>) -> String {
    let Some(name) = raw.map(str::trim).filter(|name| !name.is_empty()) else {
        return HUMAN.to_string();
    };
    if name.eq_ignore_ascii_case(HUMAN) {
        HUMAN.to_string()
    } else {
        name.to_string()
    }
}

/// The ticket title a piece of note text makes: the **first non-blank line**,
/// trimmed, with a leading task-list marker removed.
pub fn ticket_title(text: &str) -> String {
    for line in text.split('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        return stripping_task_marker(trimmed);
    }
    String::new()
}

/// The task-list marker at the start of a line, removed: `- [ ] `, `* [x] `,
/// `1. [ ] ` (any bullet, any indentation). A checkbox line's *text* is the
/// ticket's title; the syntax is the note's business.
pub fn stripping_task_marker(line: &str) -> String {
    let mut rest = line.trim_start();
    let bullet = rest.chars().next();
    match bullet {
        Some('-') | Some('*') | Some('+') => {
            rest = &rest[1..];
        }
        Some(character) if character.is_ascii_digit() => {
            let digits = rest
                .char_indices()
                .find(|(_, character)| !character.is_ascii_digit())
                .map(|(index, _)| index)
                .unwrap_or(rest.len());
            rest = &rest[digits..];
            match rest.chars().next() {
                Some('.') | Some(')') => rest = &rest[1..],
                _ => return line.to_string(),
            }
        }
        _ => return line.to_string(),
    }
    if !(rest.starts_with(' ') || rest.starts_with('\t')) {
        return line.to_string();
    }
    rest = rest.trim_start();
    let Some(marker) = rest.strip_prefix('[') else {
        return line.to_string();
    };
    let mut characters = marker.chars();
    match characters.next() {
        Some(' ') | Some('x') | Some('X') => {}
        _ => return line.to_string(),
    }
    let Some(rest) = characters.as_str().strip_prefix(']') else {
        return line.to_string();
    };
    rest.trim().to_string()
}

/// Whether `path` is a legal bikeshed-root-relative path: not absolute, and no
/// empty, `.` or `..` component (ADR 0008's rule, without a root to resolve it
/// under).
pub fn is_note_link_path(path: &str) -> bool {
    if path.is_empty() || path.starts_with('/') {
        return false;
    }
    path.split('/')
        .all(|component| !component.is_empty() && component != "." && component != "..")
}
