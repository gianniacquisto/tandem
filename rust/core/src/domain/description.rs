//! The body of a `ticket set --description -` write, and the two sections the
//! board reads out of a description (`DescriptionBody.swift`).
//!
//! The store's own rule is that an empty description is a legitimate value: the
//! text is trimmed at its edges and an empty result means "no description". The
//! stdin path is a different thing: an empty pipe, a failed heredoc and an unset
//! variable all arrive as the same empty body, and writing it would erase the brief
//! the session-ticket protocol exists to preserve (ADR 0024). So a named body must
//! carry text, on stdin and on the wire alike: the CLI's `ticket add --description`
//! and the rpc's `ticket.add` refuse the same empty value with the same error.

use super::error::DomainError;

/// The description rules the ticket body reads by.
pub struct DescriptionBody;

impl DescriptionBody {
    /// The body to write: the text as it arrived, trimmed at its edges by the
    /// store's own description rule. Fails when the body carries no text.
    pub fn required(body: &str) -> Result<String, DomainError> {
        match normalize(Some(body)) {
            Some(text) => Ok(text),
            None => Err(DomainError::EmptyDescriptionBody),
        }
    }

    /// Whether the description carries an answer block: a line whose trimmed start
    /// is `## Answer`, case-insensitively.
    ///
    /// **Line-anchored on purpose.** The block is how the sprint and the interview
    /// record a decision, so the test is the heading, not the words: a ticket that
    /// mentions the block inline is still asking its question.
    pub fn answered(description: Option<&str>) -> bool {
        let Some(description) = description else {
            return false;
        };
        description
            .lines()
            .any(|line| line.trim().to_lowercase().starts_with("## answer"))
    }

    /// Why a ticket waits: the text under a line whose trimmed start is
    /// `## Blocked on`, up to the next `## ` heading, trimmed, `None` when the body
    /// carries no such line or the section is empty (TAN-232). Line-anchored, for
    /// `answered`'s reason.
    pub fn blocked_reason(description: Option<&str>) -> Option<String> {
        let description = description?;
        let lines: Vec<&str> = description.lines().collect();
        let heading = lines
            .iter()
            .position(|line| line.trim().to_lowercase().starts_with("## blocked on"))?;
        let mut body: Vec<&str> = Vec::new();
        for line in &lines[heading + 1..] {
            if line.trim().starts_with("## ") {
                break;
            }
            body.push(line);
        }
        let text = body.join("\n");
        let text = text.trim();
        if text.is_empty() {
            None
        } else {
            Some(text.to_string())
        }
    }
}

/// A description is trimmed at its edges and an empty result means `None`.
pub(crate) fn normalize(raw: Option<&str>) -> Option<String> {
    let raw = raw?;
    let text = raw.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.to_string())
    }
}
