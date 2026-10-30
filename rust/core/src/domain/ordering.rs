//! Pure ordering arithmetic for drag-and-drop (`Ordering.swift`): the view names
//! the row a ticket was dropped on, and this turns that into the index `move`
//! wants.
//!
//! Keeping it here rather than in the view buys two things a view cannot give
//! itself: it is provable against the real `move` in the suite, and the two subtle
//! rules live in one place, next to the operation they feed:
//!
//! - the anchor is resolved **by id**, never by its position in a visible list, so
//!   a filter that hides rows cannot land the drop somewhere else;
//! - one is subtracted when the moving ticket already sat before the drop point,
//!   because `move` removes the ticket before it inserts.

use uuid::Uuid;

use super::ticket::Ticket;

/// The drag arithmetic.
pub struct TicketOrdering;

impl TicketOrdering {
    /// The index to hand to `move_ticket` so the ticket `moving` lands immediately
    /// before (`after == false`) or after the anchor, **within the anchor's own
    /// lane**. `None` when either ticket is missing, or when the row was dropped on
    /// itself.
    pub fn drop_index(
        tickets: &[Ticket],
        moving: Uuid,
        relative_to: Uuid,
        after: bool,
    ) -> Option<usize> {
        if moving == relative_to {
            return None;
        }
        let anchor = tickets.iter().find(|ticket| ticket.id == relative_to)?;
        let members: Vec<&Ticket> = tickets
            .iter()
            .filter(|ticket| ticket.lane == anchor.lane)
            .collect();
        let anchor_index = members.iter().position(|ticket| ticket.id == relative_to)?;
        let mut index = anchor_index + usize::from(after);
        if let Some(moving_index) = members.iter().position(|ticket| ticket.id == moving)
            && moving_index < index
        {
            index -= 1;
        }
        Some(index)
    }
}
