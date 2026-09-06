//! Responses from device login session transitions.
use afd_redis::session::SessionStatus;

/// A login that has just been opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opened {
    /// The identifier the command line polls on.
    pub session_id: String,
    /// The page a person approves it on.
    pub login_url: String,
}

/// A login that is still in flight.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Waiting {
    /// Whether it is waiting for a human or for a code.
    pub status: SessionStatus,
    /// The public key the command line presented, echoed back to it.
    pub cli_public_key: String,
    /// What the credential will be called.
    pub token_name: String,
    /// When the window closes, in milliseconds since the epoch.
    pub expires_at_ms: i64,
}

/// A redeemed login's sealed credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Redeemed {
    /// The dashboard's public key, for the client's key agreement.
    pub dashboard_public_key: String,
    /// The sealed credential.
    pub ciphertext: String,
    /// The nonce it was sealed under.
    pub nonce: String,
    /// Whether this was a repeat inside the replay window.
    ///
    /// Carried so the caller can tell the two apart in its audit record. The
    /// WIRE shape is identical for both, deliberately: a command line asking
    /// again after a dropped connection must not be able to tell that its first
    /// request landed, or it would have to decide what to do about it.
    pub repeated: bool,
}

impl Redeemed {
    /// The first redemption of a session.
    pub(super) fn first(payload: afd_redis::session::VerifyPayload) -> Self {
        Self::of(payload, false)
    }

    /// A repeat inside the replay window.
    pub(super) fn repeated(payload: afd_redis::session::VerifyPayload) -> Self {
        Self::of(payload, true)
    }

    fn of(payload: afd_redis::session::VerifyPayload, repeated: bool) -> Self {
        Self {
            dashboard_public_key: payload.dashboard_public_key,
            ciphertext: payload.ciphertext,
            nonce: payload.nonce,
            repeated,
        }
    }
}

/// Whether a cancel transitioned the session or found it already terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cancelled {
    /// This call performed the abort — the caller writes the audit record.
    Now,
    /// It was already aborted, so nothing changed and nothing is recorded.
    Already,
}
