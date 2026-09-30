//! One invitation into an account, and the rule that decides what accepting
//! it does.
//!
//! What the store does to invitations, issuing, listing, revoking and
//! accepting, is [`lifecycle`]. This half is the invitation itself, so the
//! acceptance rule is a plain function of the stored row and is tested
//! without a database.

mod lifecycle;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use sqlx::Row as _;
use sqlx::postgres::PgRow;

use super::{Invitee, email};
use crate::workspace::access::Role;
use crate::{Result, error};

/// How long an invitation can be accepted: seven days.
pub const INVITE_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// The table an invitation is read from, as a malformed row reports it.
const TABLE: &str = "core.invites";
/// The columns more than one invitation read names, each spelled once.
const COLUMN_ID: &str = "id";
const COLUMN_TENANT_ID: &str = "tenant_id";
const COLUMN_EXPIRES_AT: &str = "expires_at";
const COLUMN_ACCEPTED_BY: &str = "accepted_by";
/// The context an unreadable invitation row reports under.
const CONTEXT_READ: &str = "read invitation";

/// One invitation into an account, as stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invitation {
    /// Its identifier, which the accept link names.
    pub id: Uuid7,
    /// The account it opens.
    pub tenant: Uuid7,
    /// The address it is for, lowercased.
    pub email: String,
    /// The role accepting it grants.
    pub role: Role,
    /// When it stops being acceptable.
    pub expires_at_ms: i64,
    /// When it was issued.
    pub created_at_ms: i64,
    /// Who accepted it, once someone has.
    pub accepted_by: Option<Uuid7>,
    /// When the owner revoked it, if they did.
    pub revoked_at_ms: Option<i64>,
}

/// What accepting an invitation does for one person, now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Acceptance {
    /// They join the account.
    Join,
    /// They joined through it already, so accepting again is a replay.
    AlreadyJoined,
    /// It was sent to another address.
    WrongAddress,
    /// It expired, was revoked, or somebody else accepted it.
    Closed,
}

impl Invitation {
    /// What accepting this invitation does for `invitee` at `now`.
    #[must_use]
    pub fn acceptance_for(&self, invitee: &Invitee<'_>, now: UnixMillis) -> Acceptance {
        match &self.accepted_by {
            Some(user) if user == invitee.user => Acceptance::AlreadyJoined,
            Some(_) => Acceptance::Closed,
            None if self.revoked_at_ms.is_some() || self.expires_at_ms <= now.as_millis() => {
                Acceptance::Closed
            }
            None if self.email != email::fold(invitee.email) => Acceptance::WrongAddress,
            None => Acceptance::Join,
        }
    }

    /// Reads one invitation by column name, as every invitation statement
    /// selects it (`select_invitation!`).
    fn read(row: &PgRow) -> Result<Self> {
        let unreadable = error::query(CONTEXT_READ);
        let uuid = |column: &'static str, value: &str| {
            Uuid7::parse(value).map_err(error::row_malformed(TABLE, column))
        };
        let id: String = row.try_get(COLUMN_ID).map_err(&unreadable)?;
        let tenant: String = row.try_get(COLUMN_TENANT_ID).map_err(&unreadable)?;
        let role: String = row.try_get("role").map_err(&unreadable)?;
        let accepted_by: Option<String> = row.try_get(COLUMN_ACCEPTED_BY).map_err(&unreadable)?;
        Ok(Self {
            id: uuid(COLUMN_ID, &id)?,
            tenant: uuid(COLUMN_TENANT_ID, &tenant)?,
            email: row.try_get("email").map_err(&unreadable)?,
            role: Role::parse(&role)?,
            expires_at_ms: row.try_get(COLUMN_EXPIRES_AT).map_err(&unreadable)?,
            created_at_ms: row.try_get("created_at").map_err(&unreadable)?,
            accepted_by: accepted_by
                .as_deref()
                .map(|user| uuid(COLUMN_ACCEPTED_BY, user))
                .transpose()?,
            revoked_at_ms: row.try_get("revoked_at").map_err(&unreadable)?,
        })
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "test module: an unmet precondition should fail the test loudly"
)]
mod tests {
    use afd_core::clock::UnixMillis;
    use afd_core::id::Uuid7;

    use super::{Acceptance, Invitation};
    use crate::team::Invitee;
    use crate::workspace::access::Role;

    const NOW: i64 = 1_767_225_600_000;
    const BOB: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1011";
    const CAROL: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1012";

    fn uuid(value: &str) -> Uuid7 {
        Uuid7::parse(value).expect("the fixture identifier is UUIDv7")
    }

    fn invitation(
        accepted_by: Option<&str>,
        revoked_at_ms: Option<i64>,
        expires_at_ms: i64,
    ) -> Invitation {
        Invitation {
            id: uuid(CAROL),
            tenant: uuid(CAROL),
            email: "bob@example.com".to_owned(),
            role: Role::Member,
            expires_at_ms,
            created_at_ms: NOW - 1,
            accepted_by: accepted_by.map(uuid),
            revoked_at_ms,
        }
    }

    fn for_bob(invitation: &Invitation, email: &str) -> Acceptance {
        let bob = uuid(BOB);
        invitation.acceptance_for(&Invitee { user: &bob, email }, UnixMillis::from_millis(NOW))
    }

    #[test]
    fn an_open_invitation_joins_its_address_in_any_case() {
        assert_eq!(
            for_bob(&invitation(None, None, NOW + 1), " Bob@Example.com "),
            Acceptance::Join
        );
    }

    #[test]
    fn accepting_again_is_a_replay_for_the_person_who_joined() {
        assert_eq!(
            for_bob(&invitation(Some(BOB), None, NOW + 1), "bob@example.com"),
            Acceptance::AlreadyJoined
        );
    }

    #[test]
    fn expired_revoked_and_someone_elses_are_one_answer() {
        for closed in [
            invitation(None, None, NOW),
            invitation(None, Some(NOW - 1), NOW + 1),
            invitation(Some(CAROL), None, NOW + 1),
        ] {
            assert_eq!(for_bob(&closed, "bob@example.com"), Acceptance::Closed);
        }
    }

    #[test]
    fn an_open_invitation_for_another_address_is_refused_as_such() {
        assert_eq!(
            for_bob(&invitation(None, None, NOW + 1), "carol@example.com"),
            Acceptance::WrongAddress
        );
    }
}
