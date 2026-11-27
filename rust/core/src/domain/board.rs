//! The board: the ticket aggregate over the store adapter.
//!
//! These are the operations both faces use, and they define the shared semantics so
//! the binary and the app cannot drift. The domain holds no SQL, no JSON and no argv:
//! it reads one [`Contents`], changes it, and hands back the difference for
//! [`Store::write`] to commit under one transaction and one [`Actor`].
//!
//! **Ordering.** The stored order is authoritative: a ticket's place in the ticket
//! vector is its place in the list, and `add` puts a new ticket on **top** of its
//! label by default. Order is never recomputed from `created_at`. Labels group the
//! list, and the list is kept grouped by label (default first, then each declared
//! label) so a reader can walk it straight through.
//!
//! Every id-taking operation resolves its handle through [`Contents::require_ticket`],
//! the store's one rule: a UUID, or the ref (`TAN-42`, case-insensitive on the key).

use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use crate::store::board_rows::TicketKey;
use crate::store::{Actor, NewComment, Store};
use crate::store::{decode, encode, unix_now};

use super::comment::{Comment, CommentRead};
use super::contents::Contents;
use super::description::{self, DescriptionBody};
use super::edit::Edit;
use super::error::DomainError;
use super::query::{Filter, HUMAN, TicketQuery};
use super::ticket::{Status, Ticket};
use super::ticket_label::{
    Label, LabelDeletion, Project, ProjectKey, ProjectWithLabels, TicketRef,
};

/// The fields a new ticket may be given, over the title it must have.
#[derive(Debug, Clone)]
pub struct AddTicket<'a> {
    /// The project the ticket lives in. A `label` alone implies its project.
    pub project: Option<&'a str>,
    pub label: Option<&'a str>,
    pub status: Status,
    pub assignee: Option<&'a str>,
    pub description: Option<&'a str>,
    /// Who filed it. Written once and never changed (ADR 0020).
    pub origin: &'a str,
}

impl Default for AddTicket<'_> {
    fn default() -> Self {
        Self {
            project: None,
            label: None,
            status: Status::Backlog,
            assignee: None,
            description: None,
            origin: HUMAN,
        }
    }
}

/// The fields `set` may change. Each is an [`Edit`], so "leave it alone" and
/// "clear it" are different instructions.
#[derive(Debug, Clone, Default)]
pub struct SetFields<'a> {
    pub title: Edit<&'a str>,
    pub status: Edit<Status>,
    pub assignee: Edit<Option<&'a str>>,
    pub description: Edit<Option<&'a str>>,
    /// The text to append after the existing body, separated by one blank
    /// line. Bare text onto an empty body. Naming this with `description` is
    /// refused: a replace and an append in one call cannot both win.
    pub append_description: Edit<&'a str>,
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
    /// between the two statements.
    pub fn list_snapshot(&self, query: &TicketQuery) -> Result<(Vec<Ticket>, i64), DomainError> {
        let Some((rows, revision)) = self.store.listing_snapshot(&query.scope())? else {
            return Ok((Vec::new(), 0));
        };
        let contents = Contents::from_rows(&rows)?;
        Ok((query.apply(&contents.tickets), revision))
    }

    /// The label names in render order (the default area is implicit and unnamed).
    /// Includes any name that appears only on a ticket, so a face can draw it.
    pub fn labels(&self) -> Result<Vec<String>, DomainError> {
        Ok(self.load()?.known_labels())
    }

    /// [`labels`](Self::labels) and the revision its names are a snapshot of, read
    /// in one transaction.
    pub fn labels_snapshot(&self) -> Result<(Vec<String>, i64), DomainError> {
        let Some((rows, revision)) = self.store.board_snapshot()? else {
            return Ok((Vec::new(), 0));
        };
        let contents = Contents::from_rows(&rows)?;
        Ok((contents.known_labels(), revision))
    }

    /// `ticket(id)`: the ticket a handle names, or the failure every id-taking
    /// operation raises. A lock-free read.
    pub fn ticket(&self, handle: &str) -> Result<Ticket, DomainError> {
        let contents = self.load()?;
        let (_, index) = contents.require_ticket(handle)?;
        Ok(contents.tickets[index].clone())
    }

    /// [`ticket`](Self::ticket) and the revision its row is a snapshot of, read
    /// in one transaction.
    pub fn ticket_snapshot(&self, handle: &str) -> Result<(Ticket, i64), DomainError> {
        let key = match uuid::Uuid::parse_str(handle) {
            Ok(id) => Some(TicketKey::Id(id.to_string())),
            Err(_) => TicketRef::parse(handle).map(|(key, number)| TicketKey::Ref { key, number }),
        };
        let snapshot = match key {
            Some(key) => self.store.one_snapshot(&key)?,
            None => self.store.board_snapshot()?,
        };
        let Some((rows, revision)) = snapshot else {
            return Err(DomainError::UnknownId {
                id: handle.to_string(),
                open: Vec::new(),
            });
        };
        let contents = Contents::from_rows(&rows)?;
        let (_, index) = contents.require_ticket(handle)?;
        Ok((contents.tickets[index].clone(), revision))
    }

    /// `comments(id)`: the ticket's comments in append order, or an empty list.
    /// The ticket must exist; an unknown handle is the same failure `append`
    /// raises, with an empty list the result only for a ticket that has none.
    pub fn comments(&self, handle: &str) -> Result<Vec<Comment>, DomainError> {
        let contents = self.load()?;
        let (id, _) = contents.require_ticket(handle)?;
        self.comment_rows(id)
    }

    /// [`comments`](Self::comments) and the revision they are a snapshot of,
    /// read in one transaction: the handle is resolved first, then the log and
    /// its number come from one read.
    pub fn comments_snapshot(&self, handle: &str) -> Result<(Vec<Comment>, i64), DomainError> {
        let contents = self.load()?;
        let (id, _) = contents.require_ticket(handle)?;
        let Some((rows, revision)) = self.store.comment_snapshot(&id.to_string())? else {
            return Ok((Vec::new(), 0));
        };
        let comments = rows
            .into_iter()
            .map(comment_from_row)
            .collect::<Result<Vec<_>, _>>()?;
        Ok((comments, revision))
    }

    /// The app's read path: the log, or why it could not be read. The two failures
    /// get their own words, because they ask the human for different things: one
    /// asks for a retry, the other for a repair.
    pub fn comment_read(&self, handle: &str) -> CommentRead {
        match self.comments(handle) {
            Ok(comments) => CommentRead::List(comments),
            Err(error @ DomainError::Locked(_)) => CommentRead::Busy(error.message()),
            Err(error) => CommentRead::Failed(error.message()),
        }
    }

    // MARK: - Labels

    /// `add_label(name, project)`: append a new, empty label inside a project. The
    /// name is trimmed; an empty or already-used name is rejected. The project is
    /// required and must be a known one: a refusal lists them. Returns the label
    /// list after the write.
    pub fn add_label(&self, name: &str, project: Option<&str>) -> Result<Vec<String>, DomainError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(DomainError::EmptyLabelName);
        }
        let project = project.map(str::trim).filter(|raw| !raw.is_empty());
        self.mutate(|contents| {
            if let Some(known) = contents
                .known_labels()
                .iter()
                .find(|known| known.to_lowercase() == name.to_lowercase())
            {
                return Err(DomainError::DuplicateLabel(known.clone()));
            }
            let known = contents.known_projects();
            let Some(parent) = project else {
                return Err(DomainError::MissingProject {
                    projects: known,
                    for_label: true,
                });
            };
            if !known.iter().any(|known| known == parent) {
                return Err(DomainError::UnknownProjectName {
                    name: parent.to_string(),
                    projects: known,
                });
            }
            contents.labels.push(Label::new(name, parent));
            Ok((contents.known_labels(), true))
        })
    }

    /// `rename_label(from, to, project)`: rename a label in place, the label list
    /// and every ticket that belongs to it, in one write. The project survives by
    /// default; a present project re-parents the label, must name a known project,
    /// and is refused while the label holds any ticket (a ticket never changes
    /// project).
    pub fn rename_label(
        &self,
        from: &str,
        to: &str,
        project: Option<&str>,
    ) -> Result<Vec<String>, DomainError> {
        let from = from.trim();
        let to = to.trim();
        if to.is_empty() {
            return Err(DomainError::EmptyLabelName);
        }
        let project = project.map(str::trim);
        self.mutate(|contents| {
            let Some(i) = contents.labels.iter().position(|label| label.name == from) else {
                return Err(DomainError::UnknownLabel(from.to_string()));
            };
            let new_parent = match project {
                Some(raw) if !raw.is_empty() => {
                    let known = contents.known_projects();
                    if !known.iter().any(|known| known == raw) {
                        return Err(DomainError::UnknownProjectName {
                            name: raw.to_string(),
                            projects: known,
                        });
                    }
                    Some(raw.to_string())
                }
                _ => None,
            };
            if let Some(parent) = &new_parent
                && parent != &contents.labels[i].project
            {
                let held = contents
                    .tickets
                    .iter()
                    .filter(|ticket| ticket.label.as_deref() == Some(from))
                    .count();
                if held > 0 {
                    return Err(DomainError::LabelHoldsTickets {
                        label: from.to_string(),
                        tickets: held,
                        from: contents.labels[i].project.clone(),
                        to: parent.clone(),
                    });
                }
            }
            if from == to {
                let mut changed = false;
                if let Some(parent) = new_parent
                    && parent != contents.labels[i].project
                {
                    contents.labels[i].project = parent;
                    changed = true;
                }
                if !changed {
                    return Ok((contents.known_labels(), false));
                }
                contents.normalize();
                return Ok((contents.known_labels(), true));
            }
            if let Some(known) = contents
                .known_labels()
                .iter()
                .find(|known| known.as_str() != from && known.to_lowercase() == to.to_lowercase())
            {
                return Err(DomainError::DuplicateLabel(known.clone()));
            }
            contents.labels[i].name = to.to_string();
            if let Some(parent) = new_parent {
                contents.labels[i].project = parent;
            }
            for ticket in &mut contents.tickets {
                if ticket.label.as_deref() == Some(from) {
                    ticket.label = Some(to.to_string());
                }
            }
            contents.normalize();
            Ok((contents.known_labels(), true))
        })
    }

    /// `delete_label(name)`: remove a label and unlabel its tickets. They stay in
    /// their project, with their numbers and refs; a label is never a way to
    /// delete tickets.
    pub fn delete_label(&self, name: &str) -> Result<LabelDeletion, DomainError> {
        let name = name.trim();
        self.mutate(|contents| {
            let Some(i) = contents.labels.iter().position(|label| label.name == name) else {
                return Err(DomainError::UnknownLabel(name.to_string()));
            };
            contents.labels.remove(i);
            let mut moved = 0;
            for ticket in &mut contents.tickets {
                if ticket.label.as_deref() == Some(name) {
                    ticket.label = None;
                    moved += 1;
                }
            }
            contents.normalize();
            Ok((
                LabelDeletion {
                    labels: contents.known_labels(),
                    moved,
                },
                true,
            ))
        })
    }

    /// `move_label(name, to)`: reorder the declared labels. `index` is 0-based in the
    /// label list and is clamped, so `usize::MAX` moves it to the end. Only the label
    /// list changes: the groups travel whole.
    pub fn move_label(&self, name: &str, to: usize) -> Result<Vec<String>, DomainError> {
        let name = name.trim();
        self.mutate(|contents| {
            let Some(from) = contents.labels.iter().position(|label| label.name == name) else {
                return Err(DomainError::UnknownLabel(name.to_string()));
            };
            let label = contents.labels.remove(from);
            let to = to.min(contents.labels.len());
            contents.labels.insert(to, label);
            Ok((
                contents
                    .labels
                    .iter()
                    .map(|label| label.name.clone())
                    .collect(),
                to != from,
            ))
        })
    }

    // MARK: - Projects

    /// `projects()`: every project in render order, each with its labels in label
    /// order. Empty projects list truthfully; the board draws only projects
    /// with labels.
    pub fn projects(&self) -> Result<Vec<ProjectWithLabels>, DomainError> {
        Ok(self.load()?.projects_with_labels())
    }

    /// [`projects`](Self::projects) and the revision its rows are a snapshot of.
    pub fn projects_snapshot(&self) -> Result<(Vec<ProjectWithLabels>, i64), DomainError> {
        let Some((rows, revision)) = self.store.board_snapshot()? else {
            return Ok((Vec::new(), 0));
        };
        let contents = Contents::from_rows(&rows)?;
        Ok((contents.projects_with_labels(), revision))
    }

    /// `add_project(name, abbr)`: append a new, empty project. The name is
    /// trimmed; an empty or already-used name is rejected. An absent key is the
    /// first three alphanumerics of the name; a present one is uppercased and
    /// must be alphanumerics. Either must differ from every project key and every
    /// retired ref's prefix. Returns the project list after the write.
    pub fn add_project(
        &self,
        name: &str,
        abbr: Option<&str>,
    ) -> Result<Vec<ProjectWithLabels>, DomainError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(DomainError::EmptyProjectName);
        }
        let requested = abbr.map(ProjectKey::normalize).transpose()?;
        self.mutate(|contents| {
            if let Some(known) = contents
                .projects
                .iter()
                .find(|p| p.name.to_lowercase() == name.to_lowercase())
            {
                return Err(DomainError::DuplicateProject(known.name.clone()));
            }
            let key = match requested {
                Some(key) => unique_key(&key, contents, None)?,
                None => ProjectKey::generate(name, &taken_keys(contents, None)),
            };
            contents.projects.push(Project::new(name, key));
            Ok((contents.projects_with_labels(), true))
        })
    }

    /// `rename_project(from, to, abbr)`: rename a project in place, moving its
    /// labels and its tickets with it. Numbers and refs are unchanged. A present
    /// key moves the prefix, and is refused once the project has issued a number.
    /// An empty or already-used name is rejected; an unknown project names the
    /// known projects.
    pub fn rename_project(
        &self,
        from: &str,
        to: &str,
        abbr: Option<&str>,
    ) -> Result<Vec<ProjectWithLabels>, DomainError> {
        let from = from.trim();
        let to = to.trim();
        if to.is_empty() {
            return Err(DomainError::EmptyProjectName);
        }
        let requested = abbr.map(ProjectKey::normalize).transpose()?;
        self.mutate(|contents| {
            let Some(i) = contents.projects.iter().position(|p| p.name == from) else {
                return Err(DomainError::UnknownProjectName {
                    name: from.to_string(),
                    projects: contents.known_projects(),
                });
            };
            let mut changed = false;
            if let Some(requested) = requested
                && requested != contents.projects[i].key
            {
                if contents.projects[i].last_number > 0 {
                    return Err(DomainError::FrozenProjectKey {
                        key: contents.projects[i].key.clone(),
                        project: from.to_string(),
                    });
                }
                let key = unique_key(&requested, contents, Some(from))?;
                contents.projects[i].key = key;
                changed = true;
            }
            if from != to {
                if let Some(known) = contents
                    .projects
                    .iter()
                    .enumerate()
                    .find(|(at, p)| *at != i && p.name.to_lowercase() == to.to_lowercase())
                    .map(|(_, p)| p)
                {
                    return Err(DomainError::DuplicateProject(known.name.clone()));
                }
                contents.projects[i].name = to.to_string();
                for label in &mut contents.labels {
                    if label.project == from {
                        label.project = to.to_string();
                    }
                }
                for ticket in &mut contents.tickets {
                    if ticket.project == from {
                        ticket.project = to.to_string();
                    }
                }
                changed = true;
            }
            Ok((contents.projects_with_labels(), changed))
        })
    }

    /// `delete_project(name)`: remove an empty project. A project that still holds
    /// labels or tickets is refused, naming them: a delete can never strand a
    /// ticket.
    pub fn delete_project(&self, name: &str) -> Result<Vec<ProjectWithLabels>, DomainError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(DomainError::EmptyProjectName);
        }
        self.mutate(|contents| {
            let Some(i) = contents.projects.iter().position(|p| p.name == name) else {
                return Err(DomainError::UnknownProjectName {
                    name: name.to_string(),
                    projects: contents.known_projects(),
                });
            };
            let labels: Vec<String> = contents
                .labels
                .iter()
                .filter(|label| label.project == name)
                .map(|label| label.name.clone())
                .collect();
            let tickets = contents
                .tickets
                .iter()
                .filter(|ticket| ticket.project == name)
                .count();
            if !labels.is_empty() || tickets > 0 {
                return Err(DomainError::ProjectNotEmpty {
                    project: name.to_string(),
                    labels,
                    tickets,
                });
            }
            contents.projects.remove(i);
            Ok((contents.projects_with_labels(), true))
        })
    }

    // MARK: - Add

    /// `add(title, ...)`: the title is trimmed; empty is rejected. A ticket always
    /// lives in a project: the `project` argument names it, or a `label` alone
    /// implies the label's project, and with neither the refusal lists the
    /// projects. A project and a label that disagree are refused. The new ticket
    /// lands on the **top of its label** (or of the unlabelled group) and takes the
    /// project's next number. The filer is stamped here and never touched again.
    pub fn add(&self, title: &str, fields: AddTicket<'_>) -> Result<Ticket, DomainError> {
        let title = title.trim();
        if title.is_empty() {
            return Err(DomainError::EmptyTitle);
        }
        let assignee = normalize_assignee(fields.assignee);
        let origin = normalize_origin(Some(fields.origin));
        let description = description::normalize(fields.description);
        let label = fields.label.map(str::trim).filter(|raw| !raw.is_empty());
        let project = fields.project.map(str::trim).filter(|raw| !raw.is_empty());
        self.mutate(|contents| {
            contents.validate_label(label)?;
            let known = contents.known_projects();
            let label_project = label.and_then(|name| contents.label_project(name));
            let chosen = match (project, label_project) {
                (Some(project), Some(parent)) if project != parent => {
                    return Err(DomainError::LabelProjectMismatch {
                        label: label.unwrap_or_default().to_string(),
                        label_project: parent.to_string(),
                        project: project.to_string(),
                    });
                }
                (Some(project), _) => {
                    if !known.iter().any(|name| name == project) {
                        return Err(DomainError::UnknownProjectName {
                            name: project.to_string(),
                            projects: known,
                        });
                    }
                    project.to_string()
                }
                (None, Some(parent)) => parent.to_string(),
                (None, None) => {
                    return Err(DomainError::MissingProject {
                        projects: known,
                        for_label: false,
                    });
                }
            };
            let now = unix_now();
            let mut ticket = Ticket::new(title);
            ticket.description = description.clone();
            ticket.assignee = assignee.clone();
            ticket.origin = origin.clone();
            ticket.project = chosen;
            ticket.label = label.map(str::to_string);
            ticket.set_status(fields.status, now);
            insert_at_top(&mut ticket, contents);
            Ok((ticket, true))
        })
    }

    // MARK: - Move and place

    /// `move(id, to_label, at)`: place a ticket at a 0-based index within
    /// `to_label` (`None` is no label), inside its own project. A label of
    /// another project is refused: a ticket never changes project, so its number
    /// and ref never change. `index` is clamped into range, so `usize::MAX`
    /// appends.
    pub fn move_ticket(
        &self,
        handle: &str,
        to_label: Option<&str>,
        at: usize,
    ) -> Result<Ticket, DomainError> {
        self.mutate(|contents| {
            let from = contents.require_ticket(handle)?.1;
            contents.validate_label(to_label)?;
            refuse_cross_project(contents, &contents.tickets[from], to_label)?;
            let mut moved = contents.tickets.remove(from);
            insert(&mut moved, to_label, contents, at);
            Ok((moved, true))
        })
    }

    /// `place(id, label, status, at)`: the board's drag. **One write** that sets the
    /// label and the status together and puts the ticket at a 0-based index within
    /// the label. A label of another project is refused. A drop that lands the
    /// ticket exactly where it already was writes nothing.
    pub fn place(
        &self,
        handle: &str,
        label: Option<&str>,
        status: Status,
        at: usize,
    ) -> Result<Ticket, DomainError> {
        self.mutate(|contents| {
            let from = contents.require_ticket(handle)?.1;
            contents.validate_label(label)?;
            refuse_cross_project(contents, &contents.tickets[from], label)?;
            let before = contents.tickets.clone();
            let mut moved = contents.tickets.remove(from);
            moved.set_status(status, unix_now());
            insert(&mut moved, label, contents, at);
            let changed = contents.tickets != before;
            Ok((moved, changed))
        })
    }

    // MARK: - Complete and reopen

    /// `complete(id)`: idempotent. Moves the ticket to Done and stamps
    /// `completed_at` on the transition. The stored order is never changed.
    pub fn complete(&self, handle: &str) -> Result<Ticket, DomainError> {
        self.mutate(|contents| {
            let index = contents.require_ticket(handle)?.1;
            let mut changed = false;
            if !contents.tickets[index].done() {
                contents.tickets[index].set_status(Status::Done, unix_now());
                changed = true;
            }
            Ok((contents.tickets[index].clone(), changed))
        })
    }

    /// `reopen(id)`: the inverse of `complete`, idempotent. A reopen lands in
    /// Backlog, because completing erased the stage it came from.
    pub fn reopen(&self, handle: &str) -> Result<Ticket, DomainError> {
        self.mutate(|contents| {
            let index = contents.require_ticket(handle)?.1;
            let mut changed = false;
            if contents.tickets[index].done() {
                contents.tickets[index].set_status(Status::Backlog, unix_now());
                changed = true;
            }
            Ok((contents.tickets[index].clone(), changed))
        })
    }

    // MARK: - Set

    /// `set(id, ...)`: change the named fields in one write, leaving the rest
    /// untouched. A call that changes nothing writes nothing.
    pub fn set(&self, handle: &str, fields: SetFields<'_>) -> Result<Ticket, DomainError> {
        if fields.description.is_set() && fields.append_description.is_set() {
            return Err(DomainError::ConflictingDescriptionEdit);
        }
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
        let new_append = match fields.append_description {
            Edit::Set(raw) => Some(DescriptionBody::required(raw)?),
            Edit::Unchanged => None,
        };
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
                ticket.set_status(new_status, unix_now());
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
            if let Some(appended) = &new_append {
                ticket.description = Some(appending(appended, ticket.description.as_deref()));
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

    /// `set_link(id, to)`: point a ticket at a notes note, or clear the link.
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
    /// two UUIDs, so a label move, a label delete or a key move changes the displayed
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
            at: unix_now(),
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
                    at: &encode(record.at),
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
        rows.into_iter().map(comment_from_row).collect()
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
    /// **The read is inside the transaction**, and the write lock is held before the rows are read,
    /// so two writers over one home serialise their read-modify-write. A snapshot
    /// taken before the lock would hand the same label number to both writers and a
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

/// One comment row as the domain reads it: its stamp decoded or the row named
/// as unreadable. The read path and the snapshot path share this conversion.
fn comment_from_row(row: crate::store::board_rows::CommentRow) -> Result<Comment, DomainError> {
    let at = decode(&row.at).ok_or_else(|| {
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
}

/// Every key a new project key must avoid: each project's (but `except`'s own)
/// and each retired ref's prefix.
fn taken_keys(contents: &Contents, except: Option<&str>) -> Vec<String> {
    let mut taken: Vec<String> = contents
        .projects
        .iter()
        .filter(|project| Some(project.name.as_str()) != except)
        .map(|project| project.key.to_uppercase())
        .filter(|key| !key.is_empty())
        .collect();
    taken.extend(contents.alias_keys());
    taken
}

/// The key `requested` must not be another project's or a retired ref's prefix,
/// compared uppercased.
fn unique_key(
    requested: &str,
    contents: &Contents,
    except: Option<&str>,
) -> Result<String, DomainError> {
    let key = requested.to_uppercase();
    if taken_keys(contents, except)
        .iter()
        .any(|taken| taken.eq_ignore_ascii_case(&key))
    {
        return Err(DomainError::DuplicateProjectKey(key));
    }
    Ok(key)
}

/// A label of another project is refused for a ticket that has a project.
fn refuse_cross_project(
    contents: &Contents,
    ticket: &Ticket,
    label: Option<&str>,
) -> Result<(), DomainError> {
    let Some(label) = label else { return Ok(()) };
    let Some(parent) = contents.label_project(label) else {
        return Ok(());
    };
    if parent != ticket.project {
        return Err(DomainError::CrossProjectMove {
            reference: display_name(ticket),
            from: ticket.project.clone(),
            label: label.to_string(),
            to: parent.to_string(),
        });
    }
    Ok(())
}

/// Insert a new ticket on top of its label: it takes the project's next number and
/// its ref, and the ticket comes back as the store will read it.
fn insert_at_top(ticket: &mut Ticket, contents: &mut Contents) {
    ticket.number = Some(contents.issue_number(&ticket.project));
    let start = contents.block_start(ticket.label.as_deref());
    contents.tickets.insert(start, ticket.clone());
    contents.refresh_refs();
    ticket.reference = contents
        .tickets
        .iter()
        .find(|candidate| candidate.id == ticket.id)
        .and_then(|candidate| candidate.reference.clone());
}

/// The one place a label insertion's arithmetic lives: `index` is 0-based within
/// `label` and clamped into range, so `usize::MAX` appends.
///
/// The ticket's **number** never moves: it belongs to the project, and a move
/// stays inside the project.
fn insert(ticket: &mut Ticket, label: Option<&str>, contents: &mut Contents, at: usize) {
    ticket.label = label.map(str::to_string);
    let start = contents.block_start(label);
    let count = contents
        .tickets
        .iter()
        .filter(|candidate| candidate.label.as_deref() == label)
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

/// A ticket as a failure names it: its ref, or its title when it has none.
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

/// A comment's author: the one given, else the named actor the write runs as
/// (a member's credential, `X-Tandem-Agent`, `TANDEM_AGENT`), so a known
/// caller's comment is not left unsigned. Blank counts as absent. The plain
/// human actor keeps the unnamed author `""`. One rule for every door.
pub fn resolve_comment_author<'a>(raw: Option<&'a str>, actor: &'a Actor) -> &'a str {
    match raw.map(str::trim) {
        Some(given) if !given.is_empty() => given,
        _ => match actor {
            Actor::Agent(name) => name.as_str(),
            Actor::Human => "",
        },
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
