//! The board's geometry: which rows to draw and what sits in each cell
//! (`Board.swift`).
//!
//! The rule is the list's rule, because a lane must mean the same thing in both
//! faces: the default area first, then each declared lane, then any lane that
//! appears only on a ticket (a hand edit), so a row cannot be hidden by deleting a
//! lane name from the lane list. Rows are never dropped for being empty; that is
//! what makes a cell a drop target.
//!
//! Grouping is a pure function of the tickets it is handed. The caller decides what
//! is in view (a query's result), so "the board" and "the list" cannot disagree
//! about what a lane holds.

use std::collections::BTreeMap;

use super::ticket::{Status, Ticket};

/// One lane row of the board, cut into its status columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoardLane {
    /// The lane name; `None` is the default (ungrouped) area.
    pub key: Option<String>,
    /// The tickets in each column. A status with no tickets is absent, not empty.
    pub cells: BTreeMap<Status, Vec<Ticket>>,
}

impl BoardLane {
    /// The tickets in one column, in stored order.
    pub fn tickets(&self, status: Status) -> &[Ticket] {
        self.cells.get(&status).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Whether this is the default (ungrouped) area.
    pub fn is_default(&self) -> bool {
        self.key.is_none()
    }

    /// Every ticket in the row, whatever its column.
    pub fn count(&self) -> usize {
        self.cells.values().map(Vec::len).sum()
    }
}

/// The board's geometry, in the core.
pub struct TicketBoard;

impl TicketBoard {
    /// The board's rows: the tickets grouped by lane, each row split by status.
    pub fn rows(tickets: &[Ticket], lanes: &[String]) -> Vec<BoardLane> {
        Self::ordered_keys(lanes, tickets)
            .into_iter()
            .map(|key| {
                let mut cells: BTreeMap<Status, Vec<Ticket>> = BTreeMap::new();
                for ticket in tickets.iter().filter(|ticket| ticket.lane == key) {
                    cells.entry(ticket.status).or_default().push(ticket.clone());
                }
                BoardLane { key, cells }
            })
            .collect()
    }

    /// Every lane key in render order: the default area, the declared lanes, then
    /// any name only a ticket carries. Shared with the list so the two faces group
    /// identically.
    pub fn ordered_keys(declared: &[String], tickets: &[Ticket]) -> Vec<Option<String>> {
        let mut keys: Vec<Option<String>> = vec![None];
        for name in declared {
            let key = Some(name.clone());
            if !keys.contains(&key) {
                keys.push(key);
            }
        }
        for ticket in tickets {
            if let Some(lane) = &ticket.lane {
                let key = Some(lane.clone());
                if !keys.contains(&key) {
                    keys.push(key);
                }
            }
        }
        keys
    }
}
