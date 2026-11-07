//! Which way an invitation went, as the statement that resolves it stores it.
//!
//! `resolved_at` is what decides an invitation is pending; `status` only
//! records which way it resolved. The spellings live here and are bound into
//! each statement, because SQL cannot import a Rust constant and a literal
//! there would drift from this one (RULE STS).

/// The stored spelling of an invitation nobody has resolved yet.
pub(crate) const INVITE_STATUS_PENDING: &str = "pending";
/// The stored spelling of an invitation its invitee accepted.
pub(crate) const INVITE_STATUS_ACCEPTED: &str = "accepted";
/// The stored spelling of an invitation an owner revoked.
pub(crate) const INVITE_STATUS_REVOKED: &str = "revoked";
/// The stored spelling of an invitation retired after its window closed.
pub(crate) const INVITE_STATUS_EXPIRED: &str = "expired";

/// Where an invitation stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InviteStatus {
    /// Issued and not yet resolved.
    Pending,
    /// Its invitee joined through it.
    Accepted,
    /// An owner withdrew it.
    Revoked,
    /// Its window closed before anyone accepted it, and a new invitation to
    /// the same address retired it.
    Expired,
}

impl InviteStatus {
    /// The stored spelling.
    pub(crate) const fn wire(self) -> &'static str {
        match self {
            Self::Pending => INVITE_STATUS_PENDING,
            Self::Accepted => INVITE_STATUS_ACCEPTED,
            Self::Revoked => INVITE_STATUS_REVOKED,
            Self::Expired => INVITE_STATUS_EXPIRED,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::InviteStatus;
    use crate::sql::invite as sql;

    const EVERY: [InviteStatus; 4] = [
        InviteStatus::Pending,
        InviteStatus::Accepted,
        InviteStatus::Revoked,
        InviteStatus::Expired,
    ];

    #[test]
    fn every_status_has_its_own_spelling() {
        let spellings: HashSet<&str> = EVERY.iter().map(|status| status.wire()).collect();
        assert_eq!(spellings.len(), EVERY.len(), "{spellings:?}");
    }

    /// RULE STS: each spelling is bound, so no statement may write one.
    #[test]
    fn no_invite_statement_spells_a_status() {
        let statements = [
            sql::INSERT_INVITE,
            sql::MARK_ACCEPTED,
            sql::REVOKE_INVITE,
            sql::RETIRE_EXPIRED_PENDING,
            sql::LOCK_INVITE,
            sql::LOCK_PENDING_FOR_EMAIL,
            sql::SELECT_TENANT_PENDING,
            sql::SELECT_PENDING_FOR_EMAIL,
            sql::BEGIN_EMAIL_ATTEMPT,
            sql::RECORD_EMAIL_RESULT,
        ];
        for status in EVERY {
            let literal = format!("'{}'", status.wire());
            for statement in statements {
                assert!(!statement.contains(&literal), "{literal} in {statement}");
            }
        }
    }
}
