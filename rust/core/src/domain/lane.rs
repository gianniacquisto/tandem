//! Cycle lanes, their keys, and the ref a lane key plus a number renders
//! (`Lanes.swift`, ticket 6598E091).
//!
//! A lane is its name, its **key** (the `TAN` of `TAN-42`) and the last number it
//! issued. The key is uppercase alphanumerics, unique across lanes
//! case-insensitively, and frozen once any ticket in the lane holds a number.
//! `last_number` only ever rises, so a number a moved-away ticket used is never
//! handed out twice.

use super::error::DomainError;

/// One cycle lane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lane {
    pub name: String,
    pub key: String,
    pub last_number: i64,
}

impl Lane {
    pub fn new(name: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            key: key.into(),
            last_number: 0,
        }
    }
}

/// The outcome of deleting a lane: how many tickets fell back to the default
/// area, and the lane list after the write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaneDeletion {
    pub lanes: Vec<String>,
    pub moved: usize,
}

/// The lane key's rules and the ref's rendering, in one place, so the generator a
/// new lane uses and the parser every id-taking verb uses cannot disagree.
pub struct LaneKey;

impl LaneKey {
    /// Whether `raw` is a key: non-empty, uppercase ASCII letters and digits only.
    pub fn is_valid(raw: &str) -> bool {
        !raw.is_empty()
            && raw
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    }

    /// The key as stored: trimmed and uppercased, then validated. An empty or
    /// non-alphanumeric key is a domain failure, never silently repaired.
    pub fn normalize(raw: &str) -> Result<String, DomainError> {
        let key = raw.trim().to_uppercase();
        if !Self::is_valid(&key) {
            return Err(DomainError::InvalidLaneKey(raw.to_string()));
        }
        Ok(key)
    }

    /// The first three alphanumerics of `name`, separators stripped: `Tandem` is
    /// `TAN`, `code-rank` is `COD`. A collision extends the prefix (`TAND`, ...)
    /// and, past the whole name, numbers the three-letter stem (`TAN2`).
    /// Deterministic, so two readers of the same store agree.
    pub fn generate(name: &str, taken: &[String]) -> String {
        let mut base: String = name
            .to_uppercase()
            .chars()
            .filter(|character| character.is_ascii_alphanumeric())
            .collect();
        if base.is_empty() {
            base = "L".to_string();
        }
        let characters: Vec<char> = base.chars().collect();
        let mut length = 3.min(characters.len());
        while length <= characters.len() {
            let candidate: String = characters[..length].iter().collect();
            if !taken.iter().any(|key| key.eq_ignore_ascii_case(&candidate)) {
                return candidate;
            }
            length += 1;
        }
        let stem: String = characters[..3.min(characters.len())].iter().collect();
        let mut suffix = 2;
        loop {
            let candidate = format!("{stem}{suffix}");
            if !taken.iter().any(|key| key.eq_ignore_ascii_case(&candidate)) {
                return candidate;
            }
            suffix += 1;
        }
    }
}

/// A ticket reference: `KEY-N` (`TAN-42`), the human handle every id-taking verb
/// accepts beside the UUID.
pub struct TicketRef;

impl TicketRef {
    /// The ref as written: no zero padding.
    pub fn render(key: &str, number: i64) -> String {
        format!("{key}-{number}")
    }

    /// A raw handle split into the lane key and the number it names. The key is
    /// matched case-insensitively (`prb-2` is `PRB-2`); a token without a dash, or
    /// with anything but digits after it, is not a ref and reads as an unknown id.
    pub fn parse(raw: &str) -> Option<(String, i64)> {
        let trimmed = raw.trim();
        let parts: Vec<&str> = trimmed.split('-').collect();
        if parts.len() != 2 {
            return None;
        }
        let number: i64 = parts[1].parse().ok()?;
        if number <= 0 {
            return None;
        }
        let key = parts[0].to_uppercase();
        if !LaneKey::is_valid(&key) {
            return None;
        }
        Some((key, number))
    }
}
