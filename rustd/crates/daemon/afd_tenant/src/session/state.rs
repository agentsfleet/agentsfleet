//! What a device-login session can be, and what each transition answers.

use af_core::spelling::from_spelling;
use af_crypto::mac::HmacSha256Tag;

/// A session's lifecycle (`docs/AUTH_DEVICE_LOGIN.md` §Session state machine).
///
/// The `rename_all` below is the reading vocabulary: a stored spelling parses
/// through serde (`af_core::spelling::from_spelling`). [`SessionStatus::as_str`]
/// is the writing one, for the `&'static str` a statement binds, and this
/// module's tests pin the two against each other. No schema CHECK repeats it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Opened by the CLI, awaiting the dashboard's approval.
    Pending,
    /// Approved; awaiting the CLI's verification code.
    VerificationPending,
    /// Redeemed once; the replay window may still answer the same request.
    Consumed,
    /// Past its deadline.
    Expired,
    /// Cancelled, or locked out after too many wrong codes.
    Aborted,
}

impl SessionStatus {
    /// Every status, for spelling round-trips.
    pub const ALL: [Self; 5] = [
        Self::Pending,
        Self::VerificationPending,
        Self::Consumed,
        Self::Expired,
        Self::Aborted,
    ];

    /// The stored spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::VerificationPending => "verification_pending",
            Self::Consumed => "consumed",
            Self::Expired => "expired",
            Self::Aborted => "aborted",
        }
    }

    /// Reads a stored spelling; `None` for one this build never writes.
    #[must_use]
    pub fn parse(stored: &str) -> Option<Self> {
        from_spelling(stored)
    }
}

/// Why a session was aborted, as stored in `aborted_reason`.
///
/// Read through serde and written through [`AbortReason::as_str`], pinned
/// together by this module's tests, as [`SessionStatus`] is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AbortReason {
    /// The approver cancelled it.
    ExplicitCancel,
    /// The fifth wrong code locked it.
    RateLimitExceeded,
}

impl AbortReason {
    /// The reason a stored spelling names, or `None` for one this build does
    /// not write.
    pub(crate) fn parse(stored: &str) -> Option<Self> {
        from_spelling(stored)
    }

    /// The stored spelling.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::ExplicitCancel => "explicit_cancel",
            Self::RateLimitExceeded => "rate_limit_exceeded",
        }
    }
}

/// What the CLI receives on redemption: the dashboard's sealed credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerifyPayload {
    pub(crate) dashboard_public_key: String,
    pub(crate) ciphertext: String,
    pub(crate) nonce: String,
}

/// Every answer a verify can give, as the retired script gave it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VerifyOutcome {
    Success(VerifyPayload),
    Replay(VerifyPayload),
    Missing,
    Expired,
    Aborted(Option<AbortReason>),
    Consumed,
    NotApproved,
    InvalidCode(u8),
    RateLimited,
}

/// What an approval answers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ApproveOutcome {
    Approved,
    Missing,
    Conflict(Option<SessionStatus>),
}

/// The dashboard's half of an approval.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Approval<'a> {
    pub(crate) dashboard_public_key: &'a str,
    pub(crate) ciphertext: &'a str,
    pub(crate) nonce: &'a str,
    pub(crate) code: &'a HmacSha256Tag,
    pub(crate) approver: &'a str,
}

#[cfg(test)]
mod tests {
    use af_core::spelling::to_spelling;

    use super::{AbortReason, SessionStatus};

    #[test]
    fn every_status_reads_back_from_its_spelling() {
        for status in SessionStatus::ALL {
            assert_eq!(SessionStatus::parse(status.as_str()), Some(status));
            assert_eq!(
                to_spelling(&status).as_deref(),
                Some(status.as_str()),
                "the written spelling is the one serde reads"
            );
        }
        assert_eq!(
            SessionStatus::parse("verified"),
            None,
            "never a stored state"
        );
    }

    #[test]
    fn abort_reasons_keep_their_stored_spellings() {
        // pin test: literal is the stored contract
        assert_eq!(AbortReason::ExplicitCancel.as_str(), "explicit_cancel");
        assert_eq!(
            AbortReason::RateLimitExceeded.as_str(),
            "rate_limit_exceeded"
        );
        for reason in [AbortReason::ExplicitCancel, AbortReason::RateLimitExceeded] {
            assert_eq!(AbortReason::parse(reason.as_str()), Some(reason));
            assert_eq!(to_spelling(&reason).as_deref(), Some(reason.as_str()));
        }
        assert_eq!(AbortReason::parse("timeout"), None, "never a stored reason");
    }
}
