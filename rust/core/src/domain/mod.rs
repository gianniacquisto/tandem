//! The board's domain: tickets, lanes, comments, order and the dependency edge
//! (PRT-5, ported from `Sources/TandemCore/`).
//!
//! The domain is pure over the store adapter: no argv, no JSON, no file I/O. It
//! reads one [`Contents`], changes it, and hands the difference to
//! [`Store::write`](crate::store::Store::write), which owns the transaction, the
//! actor on every row and the rolling backup.
//!
//! Three axes stay independent, because they are decisions and not implementation
//! detail (the brief's `architecture` lens):
//!
//! - **status** is a value with free-form transitions: [`Status::parse`] validates
//!   the word, nothing validates the move (`Ticket::set_status` only keeps
//!   `completed_at` truthful);
//! - **archived** is a flag the human sets, and a done ticket is still not archived
//!   until something asks;
//! - **the cycle lane** is a grouping with its own issued-number counter, and a
//!   ticket's ref is derived from the lane's key and that number on every read.
//!
//! The dependency edge is the one relation the Swift tests were written to pin
//! (ADR 0035): it is stored once, keyed by both tickets' UUIDs, and read from both
//! ends, so a lane move or a key move changes what a ticket is *called* and never
//! what it waits on.

pub mod board;
pub mod comment;
pub mod contents;
pub mod description;
pub mod edit;
pub mod error;
pub mod label;
pub mod lane;
pub mod layout;
pub mod markdown;
pub mod ordering;
pub mod query;
pub mod text;
pub mod ticket;
pub mod ticket_to_note;
pub mod undo;

pub use board::{AddTicket, Board, SetFields};
pub use comment::{Comment, CommentRead};
pub use contents::Contents;
pub use description::DescriptionBody;
pub use edit::Edit;
pub use error::DomainError;
pub use label::AssigneeLabel;
pub use lane::{Lane, LaneDeletion, LaneKey, TicketRef};
pub use layout::{BoardLane, TicketBoard};
pub use markdown::{InlineStyle, MarkdownBlock, MarkdownInline, TicketMarkdown};
pub use ordering::TicketOrdering;
pub use query::{Archived, AssigneeAxis, Filter, HUMAN, LaneAxis, TicketQuery};
pub use ticket::{Status, Ticket, TicketBlock};
pub use ticket_to_note::{Notes, PromotedNote, TicketToNote};
pub use undo::CompletionUndo;
