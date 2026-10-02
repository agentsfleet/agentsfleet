//! One invitation into an account, and the rule that decides what accepting
//! it does.
//!
//! What the store does to invitations, issuing, listing, revoking and
//! accepting, is [`lifecycle`]. This half is the invitation itself, so the
//! acceptance rule is a plain function of the stored row and is tested
//! without a database.

mod lifecycle;
mod mail_status;

pub use self::mail_status::{
    EMAIL_STATUS_FAILED, EMAIL_STATUS_SENT, EMAIL_STATUS_UNCONFIGURED, EmailAttempt, EmailStatus,
};

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_core::timing::DAY_MS;
use sqlx::Row as _;
use sqlx::postgres::PgRow;

use super::{Invitee, email};
use crate::sql::{COLUMN_EMAIL, COLUMN_ID, COLUMN_ROLE, COLUMN_TENANT_ID};
use crate::workspace::access::Role;
use crate::{Result, error, stored};

/// How many days an invitation can be accepted. The invite email states this
/// count, so it never promises a window the invitation does not keep.
pub const INVITE_VALID_DAYS: i64 = 7;

/// How long an invitation can be accepted, in milliseconds.
pub const INVITE_TTL_MS: i64 = INVITE_VALID_DAYS * DAY_MS;

/// The table an invitation is read from, as a malformed row reports it.
const TABLE: &str = "core.invites";
/// The columns more than one invitation read names, each spelled once.
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
    /// When it was accepted, once it has been. Outlives `accepted_by`, which
    /// clears if the accepter's user row is ever deleted.
    pub accepted_at_ms: Option<i64>,
    /// Who accepted it, once someone has.
    pub accepted_by: Option<Uuid7>,
    /// When the owner revoked it, if they did.
    pub revoked_at_ms: Option<i64>,
    /// What became of its most recent email.
    pub email_status: EmailStatus,
    /// When the relay last accepted its email, if it ever has.
    pub email_sent_at_ms: Option<i64>,
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
            None if !self.is_open(now) => Acceptance::Closed,
            None if self.email != email::fold(invitee.email) => Acceptance::WrongAddress,
            None => Acceptance::Join,
        }
    }

    /// Whether this invitation can still be accepted at `now`: nobody accepted
    /// it, nobody revoked it, and it has not expired.
    ///
    /// The row-side twin of the statements' `pending_invite!` predicate and
    /// their `expires_at > now` comparison.
    const fn is_open(&self, now: UnixMillis) -> bool {
        self.accepted_at_ms.is_none()
            && self.revoked_at_ms.is_none()
            && self.expires_at_ms > now.as_millis()
    }

    /// Reads one invitation by column name, as every invitation statement
    /// selects it (`select_invitation!`).
    fn read(row: &PgRow) -> Result<Self> {
        let unreadable = error::query(CONTEXT_READ);
        let uuid = |column: &'static str, value: &str| stored::uuid(TABLE, column, value);
        let id: String = row.try_get(COLUMN_ID).map_err(&unreadable)?;
        let tenant: String = row.try_get(COLUMN_TENANT_ID).map_err(&unreadable)?;
        let role: String = row.try_get(COLUMN_ROLE).map_err(&unreadable)?;
        let accepted_by: Option<String> = row.try_get(COLUMN_ACCEPTED_BY).map_err(&unreadable)?;
        let email_status: Option<String> = row.try_get("email_status").map_err(&unreadable)?;
        Ok(Self {
            id: uuid(COLUMN_ID, &id)?,
            tenant: uuid(COLUMN_TENANT_ID, &tenant)?,
            email: row.try_get(COLUMN_EMAIL).map_err(&unreadable)?,
            role: Role::parse(&role)?,
            expires_at_ms: row.try_get(COLUMN_EXPIRES_AT).map_err(&unreadable)?,
            created_at_ms: row.try_get("created_at").map_err(&unreadable)?,
            accepted_at_ms: row.try_get("accepted_at").map_err(&unreadable)?,
            accepted_by: accepted_by
                .as_deref()
                .map(|user| uuid(COLUMN_ACCEPTED_BY, user))
                .transpose()?,
            revoked_at_ms: row.try_get("revoked_at").map_err(&unreadable)?,
            email_status: EmailStatus::from_stored(email_status.as_deref()),
            email_sent_at_ms: row.try_get("email_sent_at").map_err(&unreadable)?,
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

    use super::{Acceptance, EmailStatus, Invitation};
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
            accepted_at_ms: accepted_by.map(|_| NOW - 1),
            accepted_by: accepted_by.map(uuid),
            revoked_at_ms,
            email_status: EmailStatus::Failed,
            email_sent_at_ms: None,
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

    /// The accepter's user row is gone, which clears `accepted_by`; the stamp
    /// keeps the invite spent, so a new account at that address cannot reuse it.
    #[test]
    fn an_invitation_whose_accepter_was_deleted_stays_closed() {
        let spent = Invitation {
            accepted_at_ms: Some(NOW - 1),
            ..invitation(None, None, NOW + 1)
        };
        assert_eq!(for_bob(&spent, "bob@example.com"), Acceptance::Closed);
    }

    #[test]
    fn an_open_invitation_for_another_address_is_refused_as_such() {
        assert_eq!(
            for_bob(&invitation(None, None, NOW + 1), "carol@example.com"),
            Acceptance::WrongAddress
        );
    }
}
