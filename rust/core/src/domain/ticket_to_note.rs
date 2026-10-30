//! Promoting a ticket into a bikeshed note (`TicketToNote.swift`), the reverse of
//! `Board::add_from_note`.
//!
//! One operation, so the app and any future agent-facing surface behave
//! identically: the note is created from the ticket's title and description, the
//! ticket's `link` points at the new note, and nothing else about the ticket
//! changes. The board keeps the work item; the note is where the prose lives.
//!
//! The note lands at `projects/<lane>/<slug>.md` and is **created, never
//! overwritten**. A ticket that already links to a note is **refused**: that note is
//! the human's own prose now, and promoting again would orphan it.
//!
//! The two writes are not one transaction (the store and the bikeshed have their own
//! locks), so the order is fixed: the note first, the link second. A failed link
//! write takes the note back off the disk, so a promotion either lands whole or not
//! at all.
//!
//! [`Notes`] is the seam the bikeshed lands behind (PRT-7): the domain must not
//! touch a file, and this operation is the one place a note is written from the
//! board's side.

use super::board::Board;
use super::error::DomainError;
use super::ticket::Ticket;

/// A note writer: the bikeshed, behind the domain's one seam to it.
pub trait Notes {
    /// Write `text` to a **new** note at `path`, or at the first free `-2`, `-3`, ...
    /// variant of it, and return the path written. Never overwrites.
    fn write_new(&self, path: &str, text: &str) -> Result<String, String>;

    /// Remove the note at `path`, used to take back a note whose ticket refused the
    /// link.
    fn delete(&self, path: &str) -> Result<(), String>;
}

/// A ticket promoted to a note: the new note's path, and the ticket as it now
/// stands (linked to that path).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromotedNote {
    pub path: String,
    pub ticket: Ticket,
}

/// The promotion.
pub struct TicketToNote<'a> {
    board: &'a Board,
    notes: &'a dyn Notes,
}

impl<'a> TicketToNote<'a> {
    pub fn new(board: &'a Board, notes: &'a dyn Notes) -> Self {
        Self { board, notes }
    }

    /// Make a bikeshed note out of the ticket a handle names and link the two.
    ///
    /// The note is made of the ticket **as the store holds it**: the title is the
    /// note's heading, the description its body, the lane its folder. An unknown
    /// handle is the board's own failure; a ticket that already links to a note is
    /// [`DomainError::AlreadyLinked`].
    pub fn promote(&self, handle: &str) -> Result<PromotedNote, DomainError> {
        let ticket = self.board.ticket(handle)?;
        // Refuse before anything is written: a ticket that already links to a note
        // keeps that link, and that note.
        if let Some(existing) = &ticket.link {
            return Err(DomainError::AlreadyLinked {
                path: existing.clone(),
            });
        }

        let path = self
            .notes
            .write_new(
                &Self::path(ticket.lane.as_deref(), &ticket.title),
                &Self::text(&ticket.title, ticket.description.as_deref()),
            )
            .map_err(DomainError::Io)?;
        let linked = self
            .board
            .link_from_note(&ticket.id.to_string(), &path)
            .inspect_err(|_| {
                // The ticket did not take the link, so the note it was meant for is
                // not wanted either: take it back off the disk.
                let _ = self.notes.delete(&path);
            })?;
        Ok(PromotedNote {
            path,
            ticket: linked,
        })
    }

    /// The note's path: `projects/<lane>/<slug>.md`. The lane is the project's
    /// folder (a ticket in the unnamed default area lands in
    /// `projects/uncategorized/`); the slug is the title.
    pub fn path(lane: Option<&str>, title: &str) -> String {
        let folder = lane
            .and_then(Self::slug)
            .unwrap_or_else(|| "uncategorized".to_string());
        format!(
            "projects/{folder}/{}.md",
            Self::slug(title).unwrap_or_else(|| "untitled".to_string())
        )
    }

    /// The note's text: the title as the heading, the description as the body. A
    /// ticket with no description gets a note with the heading alone. The ticket's
    /// own state is not written into the note.
    pub fn text(title: &str, body: Option<&str>) -> String {
        let heading = format!("# {}", title.replace('\n', " "));
        match body {
            Some(body) if !body.is_empty() => format!("{heading}\n\n{body}\n"),
            _ => format!("{heading}\n"),
        }
    }

    /// A filename slug: lowercased, every run of characters that is not a letter or
    /// a digit collapsed to one `-`, trimmed of `-`, and capped at 60 characters.
    /// `None` when nothing is left.
    pub fn slug(text: &str) -> Option<String> {
        let mut slug = String::with_capacity(text.len());
        let mut last_was_dash = false;
        for character in text.to_lowercase().chars() {
            if character.is_ascii_alphanumeric() {
                slug.push(character);
                last_was_dash = false;
            } else if !last_was_dash {
                slug.push('-');
                last_was_dash = true;
            }
        }
        let trimmed = slug.trim_matches('-');
        if trimmed.is_empty() {
            return None;
        }
        let capped: String = trimmed.chars().take(60).collect();
        let trimmed = capped.trim_matches('-');
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }
}
