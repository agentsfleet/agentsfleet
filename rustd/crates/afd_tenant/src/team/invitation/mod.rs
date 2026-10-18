//! One invitation into an account, and the rule that decides what accepting
//! it does.
//!
//! What the store does to invitations, issuing, listing, revoking and
//! accepting, is [`lifecycle`]. This half is the invitation itself, so the
//! acceptance rule is a plain function of the stored row and is tested
//! without a database.

mod lifecycle;
mod mail_status;
mod status;

pub use self::mail_status::{
    EMAIL_STATUS_FAILED, EMAIL_STATUS_SENT, EMAIL_STATUS_UNCONFIGURED, EmailAttempt, EmailStatus,
};
pub(crate) use self::status::InviteStatus;

use std::time::Duration;

use afd_core::id::Uuid7;
use afd_core::timing::DAY;
use jiff::Timestamp;

use super::{Invitee, email};
use crate::sql::COLUMN_ID;
use crate::workspace::access::Role;
use crate::{Result, stored};

/// How many days an invitation can be accepted. The invite email states this
/// count, so it never promises a window the invitation does not keep.
pub const INVITE_VALID_DAYS: i64 = 7;

/// How long an invitation can be accepted: [`INVITE_VALID_DAYS`] of exact
/// days, bound as the interval the insert adds to the database clock.
pub const INVITE_TTL: Duration = Duration::from_secs(
    DAY.as_secs()
        .saturating_mul(INVITE_VALID_DAYS.unsigned_abs()),
);

/// The table an invitation is read from, as a malformed row reports it.
const TABLE: &str = "identity.invites";
/// The column naming who accepted an invitation, as a malformed one reports it.
const COLUMN_ACCEPTED_BY: &str = "accepted_by_user_id";
/// The column naming an invitation's account, as a malformed one reports it.
const COLUMN_TENANT_ID: &str = "tenant_id";

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
    pub expires_at: Timestamp,
    /// When it was issued.
    pub created_at: Timestamp,
    /// When it was accepted, revoked or retired as expired, once it has been.
    /// Outlives `accepted_by`, which clears if the accepter's user row is ever
    /// deleted.
    pub resolved_at: Option<Timestamp>,
    /// Who accepted it, once someone has.
    pub accepted_by: Option<Uuid7>,
    /// What became of its most recent email.
    pub email_status: EmailStatus,
    /// When the relay last accepted its email, if it ever has.
    pub email_sent_at: Option<Timestamp>,
}

/// One invitation row, by the column names every invitation statement selects.
///
/// Decoded by `FromRow`; [`InvitationRow::parse`] then parses the identifiers
/// and the role, which is the only part a stored value can fail.
#[derive(Debug, sqlx::FromRow)]
pub(super) struct InvitationRow {
    id: String,
    tenant_id: String,
    email: String,
    role: String,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    expires_at: Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: Timestamp,
    accepted_by_user_id: Option<String>,
    resolved_at: Option<jiff_sqlx::Timestamp>,
    email_status: Option<String>,
    email_sent_at: Option<jiff_sqlx::Timestamp>,
}

impl InvitationRow {
    /// The invitation this row stores.
    fn parse(self) -> Result<Invitation> {
        let uuid = |column: &'static str, value: &str| stored::uuid(TABLE, column, value);
        Ok(Invitation {
            id: uuid(COLUMN_ID, &self.id)?,
            tenant: uuid(COLUMN_TENANT_ID, &self.tenant_id)?,
            email: self.email,
            role: Role::parse(&self.role)?,
            expires_at: self.expires_at,
            created_at: self.created_at,
            resolved_at: self.resolved_at.map(jiff_sqlx::Timestamp::to_jiff),
            accepted_by: self
                .accepted_by_user_id
                .as_deref()
                .map(|user| uuid(COLUMN_ACCEPTED_BY, user))
                .transpose()?,
            email_status: EmailStatus::from_stored(self.email_status.as_deref()),
            email_sent_at: self.email_sent_at.map(jiff_sqlx::Timestamp::to_jiff),
        })
    }
}

/// An invitation row held for an accept, with the database clock read once the
/// lock was granted: the instant the accept decides expiry against.
#[derive(Debug, sqlx::FromRow)]
pub(super) struct LockedRow {
    #[sqlx(flatten)]
    invitation: InvitationRow,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    locked_at: Timestamp,
}

impl LockedRow {
    /// The locked invitation, and when the lock was granted.
    fn parse(self) -> Result<(Invitation, Timestamp)> {
        Ok((self.invitation.parse()?, self.locked_at))
    }
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
    /// What accepting this invitation does for `invitee` at `at`.
    #[must_use]
    pub fn acceptance_for(&self, invitee: &Invitee<'_>, at: Timestamp) -> Acceptance {
        match &self.accepted_by {
            Some(user) if user == invitee.user => Acceptance::AlreadyJoined,
            Some(_) => Acceptance::Closed,
            None if !self.is_open(at) => Acceptance::Closed,
            None if self.email != email::fold(invitee.email) => Acceptance::WrongAddress,
            None => Acceptance::Join,
        }
    }

    /// Whether this invitation can still be accepted at `at`: nothing resolved
    /// it, and it has not expired.
    ///
    /// The row-side twin of the statements' `resolved_at IS NULL` predicate and
    /// their `expires_at >` comparison.
    fn is_open(&self, at: Timestamp) -> bool {
        self.resolved_at.is_none() && self.expires_at > at
    }
}

#[cfg(test)]
#[expect(
    clippy::expect_used,
    reason = "test module: an unmet precondition should fail the test loudly"
)]
mod tests {
    use afd_core::id::Uuid7;
    use jiff::{SignedDuration, Timestamp};

    use super::{Acceptance, DAY, EmailStatus, INVITE_TTL, INVITE_VALID_DAYS, Invitation};
    use crate::team::Invitee;
    use crate::workspace::access::Role;

    /// 2026-01-01T00:00:00Z.
    const NOW: Timestamp = Timestamp::constant(1_767_225_600, 0);
    const TICK: SignedDuration = SignedDuration::from_millis(1);
    const BOB: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1011";
    const CAROL: &str = "0195b4ba-8d3a-7f13-8abc-2b3e1e0c1012";

    fn uuid(value: &str) -> Uuid7 {
        Uuid7::parse(value).expect("the fixture identifier is UUIDv7")
    }

    fn ago() -> Timestamp {
        NOW.checked_sub(TICK)
            .expect("the fixture instant is in range")
    }

    fn later() -> Timestamp {
        NOW.checked_add(TICK)
            .expect("the fixture instant is in range")
    }

    fn invitation(
        accepted_by: Option<&str>,
        resolved_at: Option<Timestamp>,
        expires_at: Timestamp,
    ) -> Invitation {
        Invitation {
            id: uuid(CAROL),
            tenant: uuid(CAROL),
            email: "bob@example.com".to_owned(),
            role: Role::Member,
            expires_at,
            created_at: ago(),
            resolved_at: resolved_at.or_else(|| accepted_by.map(|_| ago())),
            accepted_by: accepted_by.map(uuid),
            email_status: EmailStatus::Failed,
            email_sent_at: None,
        }
    }

    fn for_bob(invitation: &Invitation, email: &str) -> Acceptance {
        let bob = uuid(BOB);
        invitation.acceptance_for(&Invitee { user: &bob, email }, NOW)
    }

    #[test]
    fn an_open_invitation_joins_its_address_in_any_case() {
        assert_eq!(
            for_bob(&invitation(None, None, later()), " Bob@Example.com "),
            Acceptance::Join
        );
    }

    #[test]
    fn accepting_again_is_a_replay_for_the_person_who_joined() {
        assert_eq!(
            for_bob(&invitation(Some(BOB), None, later()), "bob@example.com"),
            Acceptance::AlreadyJoined
        );
    }

    #[test]
    fn expired_revoked_and_someone_elses_are_one_answer() {
        for closed in [
            invitation(None, None, NOW),
            invitation(None, Some(ago()), later()),
            invitation(Some(CAROL), None, later()),
        ] {
            assert_eq!(for_bob(&closed, "bob@example.com"), Acceptance::Closed);
        }
    }

    /// The accepter's user row is gone, which clears `accepted_by`; the stamp
    /// keeps the invite spent, so a new account at that address cannot reuse it.
    #[test]
    fn an_invitation_whose_accepter_was_deleted_stays_closed() {
        let spent = Invitation {
            resolved_at: Some(ago()),
            ..invitation(None, None, later())
        };
        assert_eq!(for_bob(&spent, "bob@example.com"), Acceptance::Closed);
    }

    #[test]
    fn an_open_invitation_for_another_address_is_refused_as_such() {
        assert_eq!(
            for_bob(&invitation(None, None, later()), "carol@example.com"),
            Acceptance::WrongAddress
        );
    }

    /// The window the insert adds to the database clock is the one the email
    /// promises, in exact days.
    #[test]
    fn the_invite_window_is_the_days_the_email_states() {
        let days = INVITE_TTL.as_secs() / DAY.as_secs();
        assert_eq!(i64::try_from(days).ok(), Some(INVITE_VALID_DAYS));
        assert_eq!(INVITE_TTL.subsec_nanos(), 0);
    }
}
