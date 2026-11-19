//! The one permission function (ADR 0052, CUT-39): what each kind of principal
//! may do, in one place, so a route added later cannot forget a check.
//!
//! Pure: no SQL and no HTTP types. The guard resolves a request into a
//! [`PrincipalKind`] once, maps its route to a [`Permission`], and asks
//! [`permits`]. The rules are credential kind first, role second:
//!
//! - **owner** (loopback, `--allow-unauthenticated`, the `TANDEM_TOKEN` holder
//!   and an owner browser session): every permission.
//! - **member session**: an admin gets all five; a member gets `board` and
//!   `manage-own-tokens`.
//! - **member token** (a personal token), of any role including admin: `board`
//!   only, and the guard admits it on `/mcp` alone. A token never reaches
//!   `export`, `manage-people`, `read-access-record` or `manage-own-tokens`, so
//!   a leaked token cannot mint siblings.
//! - **operator**: none of these. Its acts are the operator interface's own.
//!
//! It fails closed: an unknown principal or an unknown permission is refused.

use crate::store::Role;

/// What a route may need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// Read and write tickets and notes.
    Board,
    /// Download the whole home.
    Export,
    /// Add, remove and change the role of members; issue sign-in keys.
    ManagePeople,
    /// Create, list and revoke a member's own personal tokens.
    ManageOwnTokens,
    /// Read the access record.
    ReadAccessRecord,
    /// A permission this build does not know. Nothing holds it.
    Unknown,
}

impl Permission {
    /// Every permission this build knows, in the order the docs list them.
    pub const ALL: [Permission; 5] = [
        Permission::Board,
        Permission::Export,
        Permission::ManagePeople,
        Permission::ManageOwnTokens,
        Permission::ReadAccessRecord,
    ];

    /// The permission a name stands for; a name this build does not know is
    /// [`Permission::Unknown`], which nobody holds.
    pub fn parse(name: &str) -> Self {
        match name {
            "board" => Self::Board,
            "export" => Self::Export,
            "manage-people" => Self::ManagePeople,
            "manage-own-tokens" => Self::ManageOwnTokens,
            "read-access-record" => Self::ReadAccessRecord,
            _ => Self::Unknown,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Board => "board",
            Self::Export => "export",
            Self::ManagePeople => "manage-people",
            Self::ManageOwnTokens => "manage-own-tokens",
            Self::ReadAccessRecord => "read-access-record",
            Self::Unknown => "unknown",
        }
    }
}

/// Who asks, as a closed kind: the credential first, the role second.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrincipalKind {
    /// Whoever holds the server.
    Owner,
    /// A browser session that belongs to a member of this role.
    MemberSession(Role),
    /// A personal token of a member of this role.
    MemberToken(Role),
    /// The hosting provider's narrow key.
    Operator,
    /// A principal this build does not know. It holds nothing.
    Unknown,
}

/// Whether `principal` holds `permission`. The only place the rules live.
pub fn permits(principal: PrincipalKind, permission: Permission) -> bool {
    use Permission::*;
    match (principal, permission) {
        (_, Unknown) | (PrincipalKind::Unknown, _) => false,
        (PrincipalKind::Owner, _) => true,
        (PrincipalKind::MemberSession(Role::Admin), _) => true,
        (PrincipalKind::MemberSession(Role::Member), Board | ManageOwnTokens) => true,
        (PrincipalKind::MemberSession(Role::Member), _) => false,
        (PrincipalKind::MemberToken(_), Board) => true,
        (PrincipalKind::MemberToken(_), _) => false,
        (PrincipalKind::Operator, _) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KINDS: [PrincipalKind; 7] = [
        PrincipalKind::Owner,
        PrincipalKind::MemberSession(Role::Admin),
        PrincipalKind::MemberSession(Role::Member),
        PrincipalKind::MemberToken(Role::Admin),
        PrincipalKind::MemberToken(Role::Member),
        PrincipalKind::Operator,
        PrincipalKind::Unknown,
    ];

    #[test]
    fn every_principal_kind_against_every_permission_fails_closed() {
        use Permission::*;
        // Rows follow KINDS; columns follow Permission::ALL.
        let table: [[bool; 5]; 7] = [
            // board, export, manage-people, manage-own-tokens, read-access-record
            [true, true, true, true, true],      // owner
            [true, true, true, true, true],      // admin session
            [true, false, false, true, false],   // member session
            [true, false, false, false, false],  // admin token
            [true, false, false, false, false],  // member token
            [false, false, false, false, false], // operator
            [false, false, false, false, false], // unknown
        ];
        assert_eq!(
            Permission::ALL,
            [
                Board,
                Export,
                ManagePeople,
                ManageOwnTokens,
                ReadAccessRecord
            ]
        );
        for (kind, row) in KINDS.iter().zip(table.iter()) {
            for (permission, expected) in Permission::ALL.iter().zip(row.iter()) {
                assert_eq!(
                    permits(*kind, *permission),
                    *expected,
                    "{kind:?} against {permission:?}"
                );
            }
            // A permission this build does not know is held by no one.
            assert!(!permits(*kind, Permission::Unknown), "{kind:?}");
            assert!(!permits(*kind, Permission::parse("teleport")), "{kind:?}");
        }
        // Every name round trips, so the table above covers what the docs list.
        for permission in Permission::ALL {
            assert_eq!(Permission::parse(permission.as_str()), permission);
        }
    }

    #[test]
    fn a_token_never_gets_export_people_or_the_record_whatever_its_role() {
        for role in [Role::Admin, Role::Member] {
            let token = PrincipalKind::MemberToken(role);
            for permission in [
                Permission::Export,
                Permission::ManagePeople,
                Permission::ReadAccessRecord,
                Permission::ManageOwnTokens,
            ] {
                assert!(!permits(token, permission), "{role:?} token {permission:?}");
            }
            assert!(permits(token, Permission::Board));
        }
    }
}
