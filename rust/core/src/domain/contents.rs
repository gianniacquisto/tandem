//! The board in memory: the lanes and the tickets, in stored order
//! (`StoreContents` in `Lanes.swift`).
//!
//! **Stored order is the order.** The ticket vector's position is authoritative
//! (new tickets land on top by default); it is never recomputed from `created_at`.
//! It is kept **grouped by lane**: the default (unnamed) area first, then each
//! declared lane in its declared order, so a reader can walk it once and a
//! hand-edited store stays legible.
//!
//! The dependency edges are the one thing a ticket cannot derive alone, so this
//! value keeps them as they were read (`edges`, dependent then blocker, in the
//! order they were set) and fills `blocked_by`/`blocks` from them.

use std::collections::HashMap;

use uuid::Uuid;

use crate::store::board_rows::{BoardRows, EdgeRow, LaneRow, TicketRow};
use crate::store::timestamp;

use super::error::DomainError;
use super::lane::{Lane, LaneKey, TicketRef};
use super::ticket::{Status, Ticket, TicketBlock};

/// The store's contents: the ordered lanes and the tickets, in stored order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Contents {
    pub lanes: Vec<Lane>,
    pub tickets: Vec<Ticket>,
    /// Every dependency edge: (dependent, blocker), in the order it was set.
    pub edges: Vec<(Uuid, Uuid)>,
}

impl Contents {
    /// The board as the store holds it, with the refs, the lane keys and the
    /// dependency halves made true: a read runs the same preparation a write does,
    /// and still writes nothing.
    pub(crate) fn from_rows(rows: &BoardRows) -> Self {
        let mut lanes: Vec<Lane> = rows
            .lanes
            .iter()
            .map(|lane| Lane {
                name: lane.name.clone(),
                key: lane.key.clone(),
                last_number: lane.last_number,
            })
            .collect();
        let tickets: Vec<Ticket> = rows.tickets.iter().map(ticket_from_row).collect();
        let edges: Vec<(Uuid, Uuid)> = rows
            .edges
            .iter()
            .filter_map(|edge| {
                Some((
                    Uuid::parse_str(&edge.ticket_id).ok()?,
                    Uuid::parse_str(&edge.depends_on_id).ok()?,
                ))
            })
            .collect();
        // A lane a ticket names but the lane list does not carry (a hand edit) is
        // declared so it can be keyed and numbered, in first-appearance order.
        for ticket in &tickets {
            let Some(name) = &ticket.lane else { continue };
            if !lanes.iter().any(|lane| &lane.name == name) {
                lanes.push(Lane::new(name.clone(), ""));
            }
        }
        let mut contents = Self {
            lanes,
            tickets,
            edges,
        };
        contents.normalize();
        contents.prepare();
        contents
    }

    /// The board as rows, in stored order, ready for the adapter's diff.
    pub(crate) fn to_rows(&self) -> BoardRows {
        BoardRows {
            lanes: self
                .lanes
                .iter()
                .enumerate()
                .map(|(position, lane)| LaneRow {
                    name: lane.name.clone(),
                    key: lane.key.clone(),
                    last_number: lane.last_number,
                    position: position as i64,
                })
                .collect(),
            tickets: self
                .tickets
                .iter()
                .enumerate()
                .map(|(position, ticket)| ticket_to_row(ticket, position as i64))
                .collect(),
            edges: self
                .edges
                .iter()
                .map(|(dependent, blocker)| EdgeRow {
                    ticket_id: dependent.to_string(),
                    depends_on_id: blocker.to_string(),
                })
                .collect(),
        }
    }

    /// Every lane key in render order: the default area, the declared lanes, then
    /// any name that appears only on a ticket (a hand edit), in first-appearance
    /// order, so a hand-edited store loses no grouping.
    pub fn ordered_keys(&self) -> Vec<Option<String>> {
        let mut keys: Vec<Option<String>> = vec![None];
        for lane in &self.lanes {
            if !keys.contains(&Some(lane.name.clone())) {
                keys.push(Some(lane.name.clone()));
            }
        }
        for ticket in &self.tickets {
            if let Some(lane) = &ticket.lane
                && !keys.contains(&Some(lane.clone()))
            {
                keys.push(Some(lane.clone()));
            }
        }
        keys
    }

    /// Every name that is safe to file into: each declared lane plus any dangling
    /// name already on a ticket.
    pub fn known_lanes(&self) -> Vec<String> {
        self.ordered_keys().into_iter().flatten().collect()
    }

    /// The index at which `key`'s block begins: the cumulative length of every
    /// preceding group. For a key that is neither declared nor present, the end of
    /// the list.
    pub fn block_start(&self, key: Option<&str>) -> usize {
        let mut start = 0;
        for candidate in self.ordered_keys() {
            if candidate.as_deref() == key {
                return start;
            }
            start += self
                .tickets
                .iter()
                .filter(|ticket| ticket.lane.as_deref() == candidate.as_deref())
                .count();
        }
        start
    }

    /// Re-establish the grouping invariant, stably: groups in `ordered_keys` order,
    /// each keeping its relative order. Run before every read-modify-write body and
    /// again before the write, so the invariant holds for every operation.
    pub fn normalize(&mut self) {
        let keys = self.ordered_keys();
        let mut grouped: Vec<Ticket> = Vec::with_capacity(self.tickets.len());
        for key in keys {
            grouped.extend(
                self.tickets
                    .iter()
                    .filter(|ticket| ticket.lane == key)
                    .cloned(),
            );
        }
        self.tickets = grouped;
        self.refresh_refs();
        self.refresh_blocked();
    }

    /// Make the ref scheme true for contents that predate it, idempotently: every
    /// ticket's lane is declared, every lane has a key, and a ticket in a lane
    /// without a number gets the lane's next one in `created_at` order (ties by
    /// id).
    pub fn prepare(&mut self) {
        self.assign_keys();
        self.number_unnumbered();
        self.refresh_refs();
        self.refresh_blocked();
    }

    /// Every lane without a key gets one, in render order, among the keys already
    /// taken.
    fn assign_keys(&mut self) {
        let mut taken: Vec<String> = self
            .lanes
            .iter()
            .map(|lane| lane.key.to_uppercase())
            .filter(|key| !key.is_empty())
            .collect();
        for lane in &mut self.lanes {
            if lane.key.is_empty() {
                let key = LaneKey::generate(&lane.name, &taken);
                taken.push(key.clone());
                lane.key = key;
            }
        }
    }

    /// A ticket in a lane without a number takes the lane's next one, oldest
    /// first: the backfill an installed board needs once.
    fn number_unnumbered(&mut self) {
        for i in 0..self.lanes.len() {
            let name = self.lanes[i].name.clone();
            let mut highest = self.lanes[i].last_number;
            let mut missing: Vec<usize> = self
                .tickets
                .iter()
                .enumerate()
                .filter(|(_, ticket)| {
                    ticket.lane.as_deref() == Some(name.as_str()) && ticket.number.is_none()
                })
                .map(|(index, _)| index)
                .collect();
            missing.sort_by(|left, right| {
                let (left, right) = (&self.tickets[*left], &self.tickets[*right]);
                left.created_at
                    .cmp(&right.created_at)
                    .then_with(|| left.id.cmp(&right.id))
            });
            for index in missing {
                highest += 1;
                self.tickets[index].number = Some(highest);
            }
            self.lanes[i].last_number = self.lanes[i].last_number.max(highest);
        }
    }

    /// The derived half: a ticket's `reference` is its lane's key and its own
    /// number, or `None` when either is missing (the default area has neither).
    pub fn refresh_refs(&mut self) {
        let mut keys: HashMap<&str, &str> = HashMap::new();
        for lane in &self.lanes {
            if !lane.key.is_empty() {
                keys.insert(lane.name.as_str(), lane.key.as_str());
            }
        }
        let rendered: Vec<Option<String>> = self
            .tickets
            .iter()
            .map(|ticket| {
                let name = ticket.lane.as_deref()?;
                let key = keys.get(name)?;
                let number = ticket.number?;
                Some(TicketRef::render(key, number))
            })
            .collect();
        for (ticket, reference) in self.tickets.iter_mut().zip(rendered) {
            ticket.reference = reference;
        }
    }

    /// The dependency half: each ticket's `blocked_by` names the other ticket as it
    /// is now (its ref, its title, its status), and `blocks` is the same edges read
    /// from the far end, in stored order.
    pub fn refresh_blocked(&mut self) {
        let shown: HashMap<Uuid, TicketBlock> = self
            .tickets
            .iter()
            .map(|ticket| {
                (
                    ticket.id,
                    TicketBlock {
                        id: ticket.id,
                        reference: ticket.reference.clone(),
                        title: ticket.title.clone(),
                        status: ticket.status,
                    },
                )
            })
            .collect();
        let edges = self.edges.clone();
        let mut near: HashMap<Uuid, Vec<TicketBlock>> = HashMap::new();
        let mut far: HashMap<Uuid, Vec<TicketBlock>> = HashMap::new();
        // The near side keeps the order the edges were set in.
        for (dependent, blocker) in &edges {
            if let Some(blocker_block) = shown.get(blocker) {
                near.entry(*dependent)
                    .or_default()
                    .push(blocker_block.clone());
            }
        }
        // The far side has no order of its own to store, so it is the board's:
        // the same order every read already follows (ADR 0006).
        for ticket in &self.tickets {
            let Some(self_block) = shown.get(&ticket.id) else {
                continue;
            };
            for (dependent, blocker) in &edges {
                if *dependent == ticket.id {
                    far.entry(*blocker).or_default().push(self_block.clone());
                }
            }
        }
        for ticket in &mut self.tickets {
            ticket.blocked_by = near.get(&ticket.id).cloned().unwrap_or_default();
            ticket.blocks = far.get(&ticket.id).cloned().unwrap_or_default();
        }
    }

    /// Issue the lane's next number: the counter rises and never goes back, so a
    /// number a moved-away ticket used is not handed out again.
    pub fn issue_number(&mut self, lane: &str) -> i64 {
        let Some(i) = self
            .lanes
            .iter()
            .position(|candidate| candidate.name == lane)
        else {
            return 0;
        };
        self.lanes[i].last_number += 1;
        self.lanes[i].last_number
    }

    /// The key a lane files under, or `None` when the lane is not declared.
    pub fn key_of(&self, lane: &str) -> Option<&str> {
        self.lanes
            .iter()
            .find(|candidate| candidate.name == lane)
            .map(|candidate| candidate.key.as_str())
    }

    /// Guards a lane argument: `None` (the default area) is always valid; any
    /// other name must be a known lane.
    pub fn validate_lane(&self, lane: Option<&str>) -> Result<(), DomainError> {
        let Some(lane) = lane else { return Ok(()) };
        if self.known_lanes().iter().any(|name| name == lane) {
            Ok(())
        } else {
            Err(DomainError::UnknownLane(lane.to_string()))
        }
    }

    /// The store's one rule for "which ticket is this id?": a full UUID, or the
    /// ticket's ref (`TAN-42`, case-insensitive on the key), and its index.
    pub fn resolve_ticket(&self, raw: &str) -> Option<(Uuid, usize)> {
        if let Ok(id) = Uuid::parse_str(raw)
            && let Some(index) = self.tickets.iter().position(|ticket| ticket.id == id)
        {
            return Some((id, index));
        }
        let (key, number) = TicketRef::parse(raw)?;
        let index = self.tickets.iter().position(|ticket| {
            ticket.number == Some(number)
                && ticket
                    .lane
                    .as_deref()
                    .and_then(|lane| self.key_of(lane))
                    .is_some_and(|lane_key| lane_key.eq_ignore_ascii_case(&key))
        })?;
        Some((self.tickets[index].id, index))
    }

    /// The ticket a handle names, or the failure every id-taking operation raises.
    /// A handle that looks like a ref but names nothing gets the ref-specific
    /// failure: it names the lane's real range, so the caller recovers in one turn.
    pub fn require_ticket(&self, raw: &str) -> Result<(Uuid, usize), DomainError> {
        if let Some(found) = self.resolve_ticket(raw) {
            return Ok(found);
        }
        if let Some((key, _)) = TicketRef::parse(raw) {
            return Err(DomainError::UnknownRef {
                reference: raw.to_string(),
                hint: self.ref_hint(&key),
            });
        }
        Err(DomainError::UnknownId {
            id: raw.to_string(),
            open: self
                .tickets
                .iter()
                .filter(|ticket| !ticket.done())
                .cloned()
                .collect(),
        })
    }

    /// What the unknown-ref failure says next: whether the key exists at all, and
    /// the range the lane has issued.
    fn ref_hint(&self, key: &str) -> String {
        let Some(lane) = self
            .lanes
            .iter()
            .find(|lane| lane.key.eq_ignore_ascii_case(key))
        else {
            let keys: Vec<&str> = self
                .lanes
                .iter()
                .map(|lane| lane.key.as_str())
                .filter(|key| !key.is_empty())
                .collect();
            if keys.is_empty() {
                return "No cycle lane has that key, and no lane carries a key yet.".to_string();
            }
            return format!(
                "No cycle lane has the key \"{key}\". The keys in use: {}.",
                keys.join(", ")
            );
        };
        if lane.last_number <= 0 {
            return format!(
                "Lane \"{}\" ({}) has issued no numbers yet.",
                lane.name, lane.key
            );
        }
        format!(
            "Lane \"{}\" ({}) has issued {}-1 to {}-{}.",
            lane.name, lane.key, lane.key, lane.key, lane.last_number
        )
    }
}

fn ticket_from_row(row: &TicketRow) -> Ticket {
    Ticket {
        id: Uuid::parse_str(&row.id).unwrap_or_else(|_| Uuid::nil()),
        title: row.title.clone(),
        status: Status::stored(&row.status),
        archived: row.archived,
        description: row.description.clone(),
        assignee: row.assignee.clone(),
        origin: row.origin.clone(),
        lane: row.lane.clone(),
        number: row.number,
        reference: None,
        link: row.link.clone(),
        blocked_by: Vec::new(),
        blocks: Vec::new(),
        created_at: timestamp::decode(&row.created_at).unwrap_or(0),
        completed_at: row.completed_at.as_deref().and_then(timestamp::decode),
    }
}

fn ticket_to_row(ticket: &Ticket, position: i64) -> TicketRow {
    TicketRow {
        id: ticket.id.to_string(),
        title: ticket.title.clone(),
        description: ticket.description.clone(),
        status: ticket.status.as_str().to_string(),
        archived: ticket.archived,
        lane: ticket.lane.clone(),
        number: ticket.number,
        origin: ticket.origin.clone(),
        assignee: ticket.assignee.clone(),
        link: ticket.link.clone(),
        position,
        created_at: timestamp::encode(ticket.created_at),
        completed_at: ticket.completed_at.map(timestamp::encode),
    }
}
