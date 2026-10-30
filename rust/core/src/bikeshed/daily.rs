//! Daily notes (`DailyNote.swift`, ticket EE991EF2): the human's own folder at the
//! vault root.
//!
//! The human writes one file per day, `daily/YYYY-MM-DD.md`, from the app's
//! **Today's note** action. The consolidation pass reads them and folds what
//! matters into the curated notes; it never edits, moves or archives one, and
//! there is no agent-facing write path for it.
//!
//! The deterministic half of the contract lives here: the harvest list, the day's
//! path, the fold marker's wording and parse, and the pending set (the files
//! dated after the marker, found by name and never by mtime).

use std::fs;

use crate::domain::error::{DomainError, DomainResult};
use crate::domain::operation::Operation;
use crate::store::timestamp;

use super::Bikeshed;
use super::stats::files;

/// A civil date, the only shape a daily note's name takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Day {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl Day {
    pub fn new(year: i32, month: u32, day: u32) -> Self {
        Self { year, month, day }
    }

    /// `YYYY-MM-DD`, zero-padded.
    pub fn text(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    /// The day the text names, or `None` when it is not exactly `YYYY-MM-DD`.
    pub fn parse(text: &str) -> Option<Self> {
        if !is_day(text) {
            return None;
        }
        Some(Self {
            year: text[0..4].parse().ok()?,
            month: text[5..7].parse().ok()?,
            day: text[8..10].parse().ok()?,
        })
    }

    /// The UTC day `seconds` since the epoch falls on.
    pub fn utc(seconds: i64) -> Self {
        Self::parse(&timestamp::encode(seconds)[..10]).expect("the encoder names a day")
    }

    /// The local day now: the calendar the human reads, the same one the app's
    /// Today's note uses.
    pub fn today() -> Self {
        let local = crate::store::local_day(&timestamp::now())
            .ok()
            .and_then(|day| Self::parse(&day));
        local.unwrap_or_else(|| Self::utc(timestamp::unix_now()))
    }
}

/// The daily-note contract's deterministic half.
pub struct DailyNote;

impl DailyNote {
    /// The human-owned folders at the vault root, read-only to every agent.
    pub fn harvest_folders() -> &'static [&'static str] {
        &["daily"]
    }

    /// The fixed wording of the fold position in `Index.md`.
    pub const MARKER_PREFIX: &'static str = "Human notes folded through";

    /// `daily/<YYYY-MM-DD>.md` for `day`.
    pub fn path(day: Day) -> String {
        format!("{}/{}.md", Self::harvest_folders()[0], day.text())
    }

    /// The day the fold marker names, or `None` when there is no marker or the
    /// line names no day.
    pub fn folded_through(index_text: &str) -> Option<String> {
        for line in index_text.split('\n') {
            let Some(marker) = line.find(Self::MARKER_PREFIX) else {
                continue;
            };
            let day = line[marker + Self::MARKER_PREFIX.len()..].trim_matches(|character| {
                character == ' ' || character == '\t' || character == '*' || character == '.'
            });
            if is_day(day) {
                return Some(day.to_string());
            }
        }
        None
    }

    /// The day a daily note's file name carries, `None` for anything else.
    pub fn day_of_file(name: &str) -> Option<String> {
        if !name.to_lowercase().ends_with(".md") {
            return None;
        }
        let day = &name[..name.len() - 3];
        if is_day(day) {
            Some(day.to_string())
        } else {
            None
        }
    }
}

impl Bikeshed {
    /// Open-or-create the day's note. An existing file is returned untouched; a
    /// missing one is created empty.
    pub fn open_daily_note(&self, day: Day) -> DomainResult<Operation<String>> {
        let path = DailyNote::path(day);
        let Some(url) = self.file_url(&path) else {
            return Err(DomainError::InvalidName(path));
        };
        match fs::metadata(&url) {
            Ok(metadata) if metadata.is_dir() => Err(DomainError::BikeshedIo(format!(
                "path \"{path}\" is a folder"
            ))),
            Ok(_) => Ok(Operation {
                value: path,
                note: None,
            }),
            Err(_) => self.create(&path),
        }
    }

    /// The daily notes the consolidation pass has not folded yet: every file in
    /// the harvest list whose day is after `Index.md`'s marker, sorted.
    pub fn pending_daily_notes(&self) -> DomainResult<Operation<Vec<String>>> {
        let index = fs::read_to_string(self.directory.join("Index.md")).unwrap_or_default();
        let marker = DailyNote::folded_through(&index);
        let mut pending = Vec::new();
        for folder in DailyNote::harvest_folders() {
            let directory = self.directory.join(folder);
            for url in files(&directory, false, true) {
                let name = url
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default();
                let Some(day) = DailyNote::day_of_file(&name) else {
                    continue;
                };
                if marker
                    .as_deref()
                    .is_some_and(|marker| day.as_str() <= marker)
                {
                    continue;
                }
                pending.push(format!("{folder}/{name}"));
            }
        }
        pending.sort();
        Ok(Operation {
            value: pending,
            note: None,
        })
    }
}

/// Whether the text is exactly `YYYY-MM-DD`.
fn is_day(text: &str) -> bool {
    let characters: Vec<char> = text.chars().collect();
    if characters.len() != 10 {
        return false;
    }
    for (index, character) in characters.iter().enumerate() {
        if index == 4 || index == 7 {
            if *character != '-' {
                return false;
            }
        } else if !character.is_ascii_digit() {
            return false;
        }
    }
    true
}
