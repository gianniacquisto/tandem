//! Members and their credentials (ADR 0052).
//!
//! A **member** is a person: a unique name and a role. A **personal token**
//! (`tdm_`) is the credential a member gives an AI coding agent; it acts as
//! that member and reaches `/mcp` only. A **sign-in key** (`tdk_`) is what a
//! member pastes into the login form to get a browser session; it is a
//! different kind, so a token can never become a session and a key can never be
//! a bearer.
//!
//! Both are 32 random bytes from the OS generator, base64url, behind a fixed
//! prefix so a leaked one is greppable. Only the SHA-256 digest is stored: the
//! values are high-entropy random, so a plain hash is enough and a lookup by the
//! indexed digest leaks nothing usable. The value is shown once, by the call that
//! mints it.
//!
//! **A credential row is not board data.** Every write here goes through
//! [`Store::session_write`], so it never moves the store's revision and never
//! arms the rolling backup. All SQL lives under the store module (ADR 0009).

use std::fmt;
use std::sync::OnceLock;

use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};
use tokio::sync::broadcast;

use super::access_record::{AccessActor, append};
use super::schema::has_table;
use super::{Result, Store, StoreError, translate};

/// The fixed prefix of a personal token.
pub const TOKEN_PREFIX: &str = "tdm_";

/// The fixed prefix of a sign-in key.
pub const SIGNIN_KEY_PREFIX: &str = "tdk_";

/// 32 bytes, base64url without padding.
const BODY_LENGTH: usize = 43;

/// Names no member may take: the owner's and operator's words, and `count`, which the operator route `/members/count` already answers.
const RESERVED_NAMES: [&str; 4] = ["count", "human", "operator", "owner"];

/// The most live (unrevoked) personal tokens one member holds.
pub const MAX_LIVE_TOKENS: usize = 20;

/// The longest token label, in characters.
pub const MAX_LABEL_CHARS: usize = 60;

/// A token's last use is rewritten at most this often.
const TOKEN_TOUCH_SECONDS: i64 = 24 * 60 * 60;

/// The `store_meta` key that records team mode. Never unset.
const TEAM_MODE_KEY: &str = "team_mode";

/// Version 14: the members, their credentials, and the member a session names.
/// `removed_at` is a tombstone, so a name is never reused; `browser_sessions`
/// gains a nullable `member_id`, and an existing row (null) is the owner session.
pub(crate) const VERSION_FOURTEEN: &str = "
CREATE TABLE IF NOT EXISTS members (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    role TEXT NOT NULL CHECK (role IN ('admin', 'member')),
    created_at INTEGER NOT NULL,
    removed_at INTEGER
);
CREATE TABLE IF NOT EXISTS member_tokens (
    id INTEGER PRIMARY KEY,
    digest TEXT NOT NULL UNIQUE,
    member_id INTEGER NOT NULL,
    label TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    last_used_at INTEGER,
    revoked_at INTEGER
);
CREATE INDEX IF NOT EXISTS member_tokens_member ON member_tokens (member_id);
CREATE TABLE IF NOT EXISTS member_signin_keys (
    member_id INTEGER PRIMARY KEY,
    digest TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL
);
ALTER TABLE browser_sessions ADD COLUMN member_id INTEGER;
";

/// What a member may do. Part 2 (CUT-39) decides what each role reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Admin,
    Member,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Member => "member",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "admin" => Some(Self::Admin),
            "member" => Some(Self::Member),
            _ => None,
        }
    }
}

/// A live member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub id: i64,
    pub name: String,
    pub role: Role,
}

/// Which kind of credential a value is shaped like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialKind {
    /// A personal token (`tdm_`).
    Token,
    /// A sign-in key (`tdk_`).
    SigninKey,
}

/// A credential value, shown once. It never prints: `Debug` is redacted.
pub struct IssuedCredential {
    value: String,
}

impl IssuedCredential {
    /// The value, for the one place that shows it to its owner.
    pub fn reveal(&self) -> &str {
        &self.value
    }
}

impl fmt::Debug for IssuedCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("IssuedCredential(<redacted>)")
    }
}

/// One personal token as a list shows it: never the value or the digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenInfo {
    pub id: i64,
    pub label: String,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
    pub revoked_at: Option<i64>,
}

/// Why a member could not be added.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemberError {
    /// The name breaks the actor rule or is reserved; the rule it broke.
    Invalid(&'static str),
    /// The name is taken now, or was taken before a removal: never reused.
    Taken,
    /// The member is the workspace's last admin, who cannot be removed or
    /// demoted by any door, the operator's included (CUT-39).
    LastAdmin,
    /// The home has no live admin yet, so the first member must be one: a
    /// workspace is never left with members and no admin.
    NoAdmin,
    Store(StoreError),
}

impl fmt::Display for MemberError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(rule) => write!(formatter, "the member name {rule}"),
            Self::Taken => {
                formatter.write_str("that member name is taken, and a name is never reused")
            }
            Self::LastAdmin => formatter.write_str(
                "that member is the last admin, who cannot be removed or demoted: make another member an admin first",
            ),
            Self::NoAdmin => formatter.write_str(
                "the first member must be an admin: a workspace needs one admin before anyone else joins",
            ),
            Self::Store(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for MemberError {}

/// Why a personal token could not be made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenError {
    /// The label breaks a rule; the one it broke.
    Label(LabelRule),
    /// The member already holds [`MAX_LIVE_TOKENS`] live tokens.
    Limit,
    Store(StoreError),
}

impl fmt::Display for TokenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Label(rule) => write!(formatter, "the token name {}", rule.as_str()),
            Self::Limit => write!(
                formatter,
                "a member holds at most {MAX_LIVE_TOKENS} personal tokens: revoke one first"
            ),
            Self::Store(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for TokenError {}

impl From<StoreError> for TokenError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// The rule a token label can break.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelRule {
    /// Nothing, or only spaces.
    Empty,
    /// More than [`MAX_LABEL_CHARS`] characters.
    TooLong,
    /// A control character, a line or paragraph separator, or a character that
    /// hides or reorders text.
    NotPlain,
}

impl LabelRule {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "must not be empty",
            Self::TooLong => "must be 60 characters or fewer",
            Self::NotPlain => "must be one line of plain text",
        }
    }
}

/// The rule a token label breaks, or `None`: 1 to 60 characters of one line of
/// plain text. A control character (a newline included), a line or paragraph
/// separator and a character that reorders or hides text are refused, and a
/// label of spaces alone is empty. The label is stored as typed in the access
/// record and drawn back on the board's screens, so the one place that bounds it
/// is the function that creates the token, not a form.
pub fn token_label_rule(label: &str) -> Option<LabelRule> {
    if label.trim().is_empty() {
        return Some(LabelRule::Empty);
    }
    if label.chars().count() > MAX_LABEL_CHARS {
        return Some(LabelRule::TooLong);
    }
    let plain = label.chars().all(|character| {
        !character.is_control()
            && !matches!(
                character,
                '\u{2028}' | '\u{2029}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}'
                    | '\u{2066}'..='\u{2069}' | '\u{FEFF}'
            )
    });
    if !plain {
        return Some(LabelRule::NotPlain);
    }
    None
}

impl From<StoreError> for MemberError {
    fn from(error: StoreError) -> Self {
        Self::Store(error)
    }
}

/// The rule a member name breaks, or `None`: the actor rule (1 to 64 lowercase
/// letters, digits and hyphens), and never `count`, `human`, `operator` or `owner`.
pub fn member_name_rule(name: &str) -> Option<&'static str> {
    if !(1..=64).contains(&name.len())
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Some("must be 1 to 64 lowercase letters, digits or hyphens");
    }
    if RESERVED_NAMES.contains(&name) {
        return Some("is reserved");
    }
    None
}

/// What a value is shaped like, or `None`: the prefix, then exactly 43 base64url
/// characters. A value that is neither is refused before any lookup.
pub fn credential_kind(value: &str) -> Option<CredentialKind> {
    let (kind, body) = match value.strip_prefix(TOKEN_PREFIX) {
        Some(body) => (CredentialKind::Token, body),
        None => (
            CredentialKind::SigninKey,
            value.strip_prefix(SIGNIN_KEY_PREFIX)?,
        ),
    };
    let well_formed = body.len() == BODY_LENGTH
        && body
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_');
    well_formed.then_some(kind)
}

/// The digest a credential is stored under: lowercase hex SHA-256.
pub fn credential_digest(value: &str) -> String {
    Sha256::digest(value.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// A fresh credential: the prefix and 32 bytes from the OS generator.
fn mint(prefix: &str) -> Result<String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|_| StoreError::Io("the system random source is unavailable".to_string()))?;
    Ok(format!("{prefix}{}", base64url(&bytes)))
}

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Base64url without padding.
fn base64url(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = chunk.get(1).copied().unwrap_or(0);
        let third = chunk.get(2).copied().unwrap_or(0);
        out.push(ALPHABET[(first >> 2) as usize] as char);
        out.push(ALPHABET[(((first & 0b11) << 4) | (second >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[(((second & 0b1111) << 2) | (third >> 6)) as usize] as char);
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(third & 0b11_1111) as usize] as char);
        }
    }
    out
}

/// The in-process signal that credentials changed (a removal, a revocation, a
/// new key). A stream that outlives its credential listens for it, so it closes
/// at once instead of at its next re-check; a removal in another process is
/// caught by that re-check.
fn signal() -> &'static broadcast::Sender<()> {
    static SIGNAL: OnceLock<broadcast::Sender<()>> = OnceLock::new();
    SIGNAL.get_or_init(|| broadcast::channel(16).0)
}

/// Subscribe to [`signal`].
pub fn credentials_changed() -> broadcast::Receiver<()> {
    signal().subscribe()
}

fn notify() {
    // No receiver is not an error.
    let _ = signal().send(());
}

fn member_from(row: &rusqlite::Row<'_>) -> rusqlite::Result<Option<Member>> {
    let id = row.get(0)?;
    let name = row.get(1)?;
    let role: String = row.get(2)?;
    Ok(Role::parse(&role).map(|role| Member { id, name, role }))
}

/// A live member's id by name.
pub(super) fn live_member_id(connection: &Connection, name: &str) -> Result<Option<i64>> {
    connection
        .query_row(
            "SELECT id FROM members WHERE name = ?1 AND removed_at IS NULL",
            [name],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| translate(error, connection))
}

/// A live member by id.
pub(super) fn live_member_by_id(connection: &Connection, id: i64) -> Result<Option<Member>> {
    connection
        .query_row(
            "SELECT id, name, role FROM members WHERE id = ?1 AND removed_at IS NULL",
            [id],
            member_from,
        )
        .optional()
        .map_err(|error| translate(error, connection))
        .map(Option::flatten)
}

/// Whether the member `id` is a live admin and the only one.
fn is_last_admin(connection: &Connection, id: i64) -> Result<bool> {
    let admins: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM members WHERE role = 'admin' AND removed_at IS NULL",
            [],
            |row| row.get(0),
        )
        .map_err(|error| translate(error, connection))?;
    let is_admin: bool = connection
        .query_row(
            "SELECT 1 FROM members WHERE id = ?1 AND role = 'admin' AND removed_at IS NULL",
            [id],
            |_| Ok(true),
        )
        .optional()
        .map_err(|error| translate(error, connection))?
        .unwrap_or(false);
    Ok(is_admin && admins <= 1)
}

impl Store {
    /// Add a member as the owner (the home's holder). See [`Store::member_add_by`].
    pub fn member_add(
        &self,
        name: &str,
        role: Role,
        now: i64,
    ) -> std::result::Result<Member, MemberError> {
        self.member_add_by(name, role, now, &AccessActor::owner())
    }

    /// Add a member. The first one puts the home in team mode, in the same
    /// write, and nothing unsets it. The access record gets its row in that
    /// write too.
    pub fn member_add_by(
        &self,
        name: &str,
        role: Role,
        now: i64,
        by: &AccessActor,
    ) -> std::result::Result<Member, MemberError> {
        if let Some(rule) = member_name_rule(name) {
            return Err(MemberError::Invalid(rule));
        }
        self.session_write(|connection| {
            let taken: bool = connection
                .query_row("SELECT 1 FROM members WHERE name = ?1", [name], |_| {
                    Ok(true)
                })
                .optional()
                .map_err(|error| translate(error, connection))?
                .unwrap_or(false);
            if taken {
                return Ok(Err(MemberError::Taken));
            }
            let admins: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM members WHERE role = 'admin' AND removed_at IS NULL",
                    [],
                    |row| row.get(0),
                )
                .map_err(|error| translate(error, connection))?;
            if admins == 0 && role != Role::Admin {
                return Ok(Err(MemberError::NoAdmin));
            }
            connection
                .execute(
                    "INSERT INTO members (name, role, created_at) VALUES (?1, ?2, ?3)",
                    params![name, role.as_str(), now],
                )
                .map_err(|error| translate(error, connection))?;
            let id = connection.last_insert_rowid();
            connection
                .execute(
                    "INSERT OR REPLACE INTO store_meta (key, value) VALUES (?1, 1)",
                    [TEAM_MODE_KEY],
                )
                .map_err(|error| translate(error, connection))?;
            append(
                connection,
                by,
                "member-added",
                Some(name),
                Some(role.as_str()),
                None,
                now,
            )?;
            Ok(Ok(Member {
                id,
                name: name.to_string(),
                role,
            }))
        })?
    }

    /// Remove a member as the owner. See [`Store::member_remove_by`].
    pub fn member_remove(&self, name: &str, now: i64) -> std::result::Result<bool, MemberError> {
        self.member_remove_by(name, now, &AccessActor::owner())
    }

    /// Remove a member: the row is tombstoned (the name stays taken) and their
    /// sessions, tokens and sign-in key are deleted in the same write. The one
    /// removal every door calls. `false` when there is no such live member, and
    /// [`MemberError::LastAdmin`] when they are the last admin: the count and
    /// the removal are one transaction, so two admins removing each other at
    /// once leave one.
    pub fn member_remove_by(
        &self,
        name: &str,
        now: i64,
        by: &AccessActor,
    ) -> std::result::Result<bool, MemberError> {
        let removed = self.session_write(|connection| {
            let Some(id) = live_member_id(connection, name)? else {
                return Ok(Ok(false));
            };
            if is_last_admin(connection, id)? {
                return Ok(Err(MemberError::LastAdmin));
            }
            let run = |sql: &str, parameters: &[&dyn rusqlite::ToSql]| {
                connection
                    .execute(sql, parameters)
                    .map_err(|error| translate(error, connection))
            };
            run("DELETE FROM browser_sessions WHERE member_id = ?1", &[&id])?;
            run("DELETE FROM member_tokens WHERE member_id = ?1", &[&id])?;
            run(
                "DELETE FROM member_signin_keys WHERE member_id = ?1",
                &[&id],
            )?;
            run(
                "UPDATE members SET removed_at = ?2 WHERE id = ?1",
                &[&id, &now],
            )?;
            append(
                connection,
                by,
                "member-removed",
                Some(name),
                None,
                None,
                now,
            )?;
            Ok(Ok(true))
        })??;
        if removed {
            notify();
        }
        Ok(removed)
    }

    /// Change a member's role as the owner. See [`Store::member_set_role_by`].
    pub fn member_set_role(
        &self,
        name: &str,
        role: Role,
        now: i64,
    ) -> std::result::Result<bool, MemberError> {
        self.member_set_role_by(name, role, now, &AccessActor::owner())
    }

    /// Give a live member a new role. `false` when there is no such member.
    /// The last admin cannot be demoted, by any door: the count and the change
    /// are one transaction. A member changing their own role ends their own
    /// sessions, and every open stream re-checks at once. A role is read from
    /// the store on every request, so any other change reaches the member's
    /// next request. Setting the role a member already has changes and records
    /// nothing.
    pub fn member_set_role_by(
        &self,
        name: &str,
        role: Role,
        now: i64,
        by: &AccessActor,
    ) -> std::result::Result<bool, MemberError> {
        let outcome = self.session_write(|connection| {
            let Some(id) = live_member_id(connection, name)? else {
                return Ok(Ok(None));
            };
            let Some(current) = live_member_by_id(connection, id)? else {
                return Ok(Ok(None));
            };
            if current.role == role {
                return Ok(Ok(Some(false)));
            }
            if current.role == Role::Admin && is_last_admin(connection, id)? {
                return Ok(Err(MemberError::LastAdmin));
            }
            connection
                .execute(
                    "UPDATE members SET role = ?2 WHERE id = ?1",
                    params![id, role.as_str()],
                )
                .map_err(|error| translate(error, connection))?;
            if by.is_member(name) {
                connection
                    .execute("DELETE FROM browser_sessions WHERE member_id = ?1", [id])
                    .map_err(|error| translate(error, connection))?;
            }
            append(
                connection,
                by,
                "member-role-changed",
                Some(name),
                Some(role.as_str()),
                None,
                now,
            )?;
            Ok(Ok(Some(true)))
        })??;
        if outcome == Some(true) {
            notify();
        }
        Ok(outcome.is_some())
    }

    /// A live member by name. A store with no member tables reads as no member.
    pub fn member_find(&self, name: &str) -> Result<Option<Member>> {
        Ok(self
            .read(|connection| {
                if !has_table(connection, "members")? {
                    return Ok(None);
                }
                connection
                    .query_row(
                        "SELECT id, name, role FROM members WHERE name = ?1 AND removed_at IS NULL",
                        [name],
                        member_from,
                    )
                    .optional()
                    .map_err(|error| translate(error, connection))
                    .map(Option::flatten)
            })?
            .flatten())
    }

    /// Every live member, oldest first. No member tables reads as none.
    pub fn members(&self) -> Result<Vec<Member>> {
        Ok(self
            .read(|connection| {
                if !has_table(connection, "members")? {
                    return Ok(Vec::new());
                }
                let mut statement = connection
                    .prepare(
                        "SELECT id, name, role FROM members WHERE removed_at IS NULL ORDER BY id",
                    )
                    .map_err(|error| translate(error, connection))?;
                let rows = statement
                    .query_map([], member_from)
                    .map_err(|error| translate(error, connection))?;
                let mut members = Vec::new();
                for row in rows {
                    if let Some(member) = row.map_err(|error| translate(error, connection))? {
                        members.push(member);
                    }
                }
                Ok(members)
            })?
            .unwrap_or_default())
    }

    /// Whether this home is in team mode: the persisted flag, or any member row.
    /// A store with no member tables reads as not in team mode, and a read never
    /// migrates. An unreadable store is an error, so a caller fails closed.
    pub fn team_mode(&self) -> Result<bool> {
        Ok(self
            .read(|connection| {
                if has_table(connection, "store_meta")? {
                    let flag: Option<i64> = connection
                        .query_row(
                            "SELECT value FROM store_meta WHERE key = ?1",
                            [TEAM_MODE_KEY],
                            |row| row.get(0),
                        )
                        .optional()
                        .map_err(|error| translate(error, connection))?;
                    if flag.is_some_and(|flag| flag != 0) {
                        return Ok(true);
                    }
                }
                if !has_table(connection, "members")? {
                    return Ok(false);
                }
                connection
                    .query_row("SELECT 1 FROM members LIMIT 1", [], |_| Ok(true))
                    .optional()
                    .map_err(|error| translate(error, connection))
                    .map(|row| row.unwrap_or(false))
            })?
            .unwrap_or(false))
    }

    /// Mint a personal token for a live member. The value is returned once and
    /// only its digest is stored. `None` when the member is not live. A label
    /// that breaks [`token_label_rule`] and a member already at
    /// [`MAX_LIVE_TOKENS`] are refused before anything is written; the count and
    /// the insert are one transaction, so two creates at once cannot pass the
    /// limit. The value never reaches the access record or a log: the row holds
    /// the label alone.
    pub fn member_token_issue(
        &self,
        member: &str,
        label: &str,
        now: i64,
    ) -> std::result::Result<Option<IssuedCredential>, TokenError> {
        self.member_token_issue_by(member, label, now, &AccessActor::owner())
    }

    /// [`Store::member_token_issue`], recorded as made by `by`.
    pub fn member_token_issue_by(
        &self,
        member: &str,
        label: &str,
        now: i64,
        by: &AccessActor,
    ) -> std::result::Result<Option<IssuedCredential>, TokenError> {
        if let Some(rule) = token_label_rule(label) {
            return Err(TokenError::Label(rule));
        }
        let value = mint(TOKEN_PREFIX)?;
        let digest = credential_digest(&value);
        let stored = self.session_write(|connection| {
            let Some(id) = live_member_id(connection, member)? else {
                return Ok(Ok(false));
            };
            let live: i64 = connection
                .query_row(
                    "SELECT COUNT(*) FROM member_tokens
                     WHERE member_id = ?1 AND revoked_at IS NULL",
                    [id],
                    |row| row.get(0),
                )
                .map_err(|error| translate(error, connection))?;
            if usize::try_from(live).unwrap_or(usize::MAX) >= MAX_LIVE_TOKENS {
                return Ok(Err(TokenError::Limit));
            }
            connection
                .execute(
                    "INSERT INTO member_tokens (digest, member_id, label, created_at)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![digest, id, label, now],
                )
                .map_err(|error| translate(error, connection))?;
            append(
                connection,
                by,
                "token-created",
                Some(member),
                Some(label),
                None,
                now,
            )?;
            Ok(Ok(true))
        })??;
        Ok(stored.then_some(IssuedCredential { value }))
    }

    /// A member's tokens, revoked ones included, oldest first.
    pub fn member_tokens(&self, member: &str) -> Result<Vec<TokenInfo>> {
        Ok(self
            .read(|connection| {
                if !has_table(connection, "member_tokens")? {
                    return Ok(Vec::new());
                }
                let Some(id) = live_member_id(connection, member)? else {
                    return Ok(Vec::new());
                };
                let mut statement = connection
                    .prepare(
                        "SELECT id, label, created_at, last_used_at, revoked_at
                         FROM member_tokens WHERE member_id = ?1 ORDER BY id",
                    )
                    .map_err(|error| translate(error, connection))?;
                let rows = statement
                    .query_map([id], |row| {
                        Ok(TokenInfo {
                            id: row.get(0)?,
                            label: row.get(1)?,
                            created_at: row.get(2)?,
                            last_used_at: row.get(3)?,
                            revoked_at: row.get(4)?,
                        })
                    })
                    .map_err(|error| translate(error, connection))?;
                rows.map(|row| row.map_err(|error| translate(error, connection)))
                    .collect()
            })?
            .unwrap_or_default())
    }

    /// Revoke one of a member's tokens at once. `false` when it is not theirs or
    /// is already revoked.
    pub fn member_token_revoke(&self, member: &str, token_id: i64, now: i64) -> Result<bool> {
        self.member_token_revoke_by(member, token_id, now, &AccessActor::owner())
    }

    /// [`Store::member_token_revoke`], recorded as made by `by`.
    pub fn member_token_revoke_by(
        &self,
        member: &str,
        token_id: i64,
        now: i64,
        by: &AccessActor,
    ) -> Result<bool> {
        let revoked = self.session_write(|connection| {
            let Some(id) = live_member_id(connection, member)? else {
                return Ok(false);
            };
            let label: Option<String> = connection
                .query_row(
                    "SELECT label FROM member_tokens
                     WHERE id = ?1 AND member_id = ?2 AND revoked_at IS NULL",
                    params![token_id, id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|error| translate(error, connection))?;
            let Some(label) = label else {
                return Ok(false);
            };
            connection
                .execute(
                    "UPDATE member_tokens SET revoked_at = ?3
                     WHERE id = ?1 AND member_id = ?2 AND revoked_at IS NULL",
                    params![token_id, id, now],
                )
                .map_err(|error| translate(error, connection))?;
            append(
                connection,
                by,
                "token-revoked",
                Some(member),
                Some(&label),
                None,
                now,
            )?;
            Ok(true)
        })?;
        if revoked {
            notify();
        }
        Ok(revoked)
    }

    /// Mint the member's sign-in key, replacing a live one (one per member).
    /// `None` when the member is not live.
    pub fn member_signin_key_issue(
        &self,
        member: &str,
        now: i64,
    ) -> Result<Option<IssuedCredential>> {
        self.member_signin_key_issue_by(member, now, &AccessActor::owner())
    }

    /// [`Store::member_signin_key_issue`], recorded as made by `by`.
    pub fn member_signin_key_issue_by(
        &self,
        member: &str,
        now: i64,
        by: &AccessActor,
    ) -> Result<Option<IssuedCredential>> {
        let value = mint(SIGNIN_KEY_PREFIX)?;
        let digest = credential_digest(&value);
        let stored = self.session_write(|connection| {
            let Some(id) = live_member_id(connection, member)? else {
                return Ok(false);
            };
            connection
                .execute(
                    "INSERT OR REPLACE INTO member_signin_keys (member_id, digest, created_at)
                     VALUES (?1, ?2, ?3)",
                    params![id, digest, now],
                )
                .map_err(|error| translate(error, connection))?;
            append(
                connection,
                by,
                "signin-key-issued",
                Some(member),
                None,
                None,
                now,
            )?;
            Ok(true)
        })?;
        Ok(stored.then_some(IssuedCredential { value }))
    }

    /// The member a personal token belongs to. Revoked, removed, unknown and
    /// malformed all read as `None`. With `touch`, a last use a day or more old
    /// is rewritten. A store with no member tables reads as no member; a store
    /// that cannot be read is an error, which a caller refuses on.
    pub fn member_by_token(&self, value: &str, now: i64, touch: bool) -> Result<Option<Member>> {
        if credential_kind(value) != Some(CredentialKind::Token) {
            return Ok(None);
        }
        let digest = credential_digest(value);
        let found = self
            .read(|connection| {
                if !has_table(connection, "member_tokens")? {
                    return Ok(None);
                }
                let row = connection
                    .query_row(
                        "SELECT m.id, m.name, m.role, t.last_used_at
                         FROM member_tokens t JOIN members m ON m.id = t.member_id
                         WHERE t.digest = ?1 AND t.revoked_at IS NULL AND m.removed_at IS NULL",
                        [&digest],
                        |row| Ok((member_from(row)?, row.get::<_, Option<i64>>(3)?)),
                    )
                    .optional()
                    .map_err(|error| translate(error, connection))?;
                Ok(row)
            })?
            .flatten();
        let Some((Some(member), last_used)) = found else {
            return Ok(None);
        };
        if touch && last_used.is_none_or(|last| now - last >= TOKEN_TOUCH_SECONDS) {
            // A touch that loses the lock is not a refusal: the token is live.
            let _ = self.session_write(|connection| {
                connection
                    .execute(
                        "UPDATE member_tokens SET last_used_at = ?2
                         WHERE digest = ?1 AND revoked_at IS NULL
                           AND (last_used_at IS NULL OR last_used_at <= ?3)",
                        params![digest, now, now - TOKEN_TOUCH_SECONDS],
                    )
                    .map_err(|error| translate(error, connection))
            });
        }
        Ok(Some(member))
    }

    /// The member a sign-in key belongs to, or `None`.
    pub fn member_by_signin_key(&self, value: &str) -> Result<Option<Member>> {
        if credential_kind(value) != Some(CredentialKind::SigninKey) {
            return Ok(None);
        }
        let digest = credential_digest(value);
        Ok(self
            .read(|connection| {
                if !has_table(connection, "member_signin_keys")? {
                    return Ok(None);
                }
                connection
                    .query_row(
                        "SELECT m.id, m.name, m.role
                         FROM member_signin_keys k JOIN members m ON m.id = k.member_id
                         WHERE k.digest = ?1 AND m.removed_at IS NULL",
                        [&digest],
                        member_from,
                    )
                    .optional()
                    .map_err(|error| translate(error, connection))
                    .map(Option::flatten)
            })?
            .flatten())
    }
}
