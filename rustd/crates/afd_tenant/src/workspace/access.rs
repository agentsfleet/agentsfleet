//! What the ownership verdict grants once a workspace is the caller's to open.
//!
//! The verdict used to carry one fact, the owning tenant. With team accounts it
//! carries a second: how the caller holds that access. A member and an owner
//! open the same workspace, and only the owner writes its secrets and
//! connectors. A platform operator reaches it from outside the account and has
//! no role there at all, which is why the grant is an enum rather than a role
//! beside an optional platform flag.

use afd_core::id::Uuid7;
use afd_core::spelling::from_spelling;

use crate::{Result, error};

/// The stored and wire spelling of the account owner's role.
pub const ROLE_OWNER: &str = "owner";

/// The stored and wire spelling of an invited member's role.
pub const ROLE_MEMBER: &str = "member";

/// A person's standing inside one account.
///
/// Roles only subtract. The capability gate runs first, on the caller's own
/// scopes, so a member never exceeds what their identity grants; the role then
/// withholds the owner-grade routes inside somebody else's account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Created the account, or holds it as its owner.
    Owner,
    /// Joined through an accepted invite.
    Member,
}

impl Role {
    /// The spelling `core.memberships.role` stores and the API renders.
    #[must_use]
    pub const fn wire(self) -> &'static str {
        match self {
            Self::Owner => ROLE_OWNER,
            Self::Member => ROLE_MEMBER,
        }
    }

    /// The role a stored value names.
    ///
    /// # Errors
    /// A value this build does not know is a row a newer daemon wrote, and it
    /// is reported as that rather than read as either role: guessing `member`
    /// would lock an owner out, and guessing `owner` would hand a stranger the
    /// account's secrets.
    pub fn parse(stored: &str) -> Result<Self> {
        from_spelling(stored).ok_or_else(|| error::role_unknown(stored))
    }

    /// The role an access row carries, where no membership row means the
    /// caller's own account, which they own.
    ///
    /// # Errors
    /// As [`Role::parse`], for a stored value this build does not know.
    pub fn held(stored: Option<&str>) -> Result<Self> {
        stored.map_or(Ok(Self::Owner), Self::parse)
    }
}

/// How the caller holds access to a workspace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grant {
    /// From inside the owning account, with this role.
    Membership(Role),
    /// From outside it, through the audited platform-wide scope.
    Platform,
}

impl Grant {
    /// The caller's role in the owning account, when they have one.
    #[must_use]
    pub const fn role(self) -> Option<Role> {
        match self {
            Self::Membership(role) => Some(role),
            Self::Platform => None,
        }
    }
}

/// The ownership verdict: whose workspace this is, and how the caller holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Access {
    /// The tenant that owns the workspace.
    pub tenant: Uuid7,
    /// How the caller reached it.
    pub grant: Grant,
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "test module: an unmet precondition should fail the test loudly"
)]
mod tests {
    use super::{Grant, ROLE_MEMBER, ROLE_OWNER, Role};

    #[test]
    fn every_role_round_trips_through_its_stored_spelling() {
        for role in [Role::Owner, Role::Member] {
            assert_eq!(Role::parse(role.wire()).ok(), Some(role));
        }
        assert_eq!(Role::Owner.wire(), ROLE_OWNER);
        assert_eq!(Role::Member.wire(), ROLE_MEMBER);
    }

    #[test]
    fn an_unknown_stored_role_is_an_error_and_not_a_guess() {
        let refused = Role::parse("admin").expect_err("no such role in this build");
        assert_eq!(
            refused.code(),
            afd_core::error_code::INTERNAL_DB_QUERY,
            "a row this build cannot read is a datastore fault, never a denial"
        );
        assert!(!refused.is_datastore_unavailable());
    }

    #[test]
    fn only_a_membership_carries_a_role() {
        assert_eq!(Grant::Membership(Role::Member).role(), Some(Role::Member));
        assert_eq!(Grant::Platform.role(), None);
    }

    /// The contributor auth page names both roles and the refusal a member
    /// meets, spelled from the constants a rename would change.
    #[test]
    fn test_docs_name_member_roles() {
        const AUTH_PAGE: &str = include_str!("../../../../../docs/AUTH.md");
        let refusal = afd_core::error_code::AUTH_OWNER_ONLY;
        for named in [ROLE_OWNER, ROLE_MEMBER, refusal.as_str()] {
            let spelled = format!("`{named}`");
            assert!(AUTH_PAGE.contains(&spelled), "docs/AUTH.md names {spelled}");
        }
    }
}
