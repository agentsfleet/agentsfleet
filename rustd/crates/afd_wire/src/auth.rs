//! The device-flow login payloads, as they appear on the wire.
//!
//! Six endpoints, four bodies, three replies. They live here rather than beside
//! the handlers for the reason every other module in this crate does: a wire
//! shape is a promise to a client that ships independently of this daemon, and
//! a promise defined inside the code that happens to serve it is a promise
//! nobody can read without opening a handler.
//!
//! # Everything borrows
//!
//! A request body is deserialized straight out of the bytes axum already holds,
//! and every field is relayed rather than transformed — the ciphertext goes to
//! Redis, the public key comes back on the next poll. Owning them would copy
//! four kilobytes of base64 per approval to no end.
//!
//! # `deny_unknown_fields`, and what it is actually for
//!
//! A field this daemon does not know is a client believing something about the
//! flow that is not true — a `scope`, an `expires_in`, a second key. Accepting
//! it silently means that belief survives to production. The Zig parses with
//! `std.json` defaults, which ignore unknown members; refusing is the stricter
//! and the safer half of the difference, and it is the same rule
//! [`crate::memory`] already holds the runner plane to.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};

/// `POST /v1/auth/sessions` — the command line opens a login.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenSessionRequest<'a> {
    /// The client's public key. Relayed; this daemon runs no curve over it.
    #[serde(borrow)]
    pub public_key: Cow<'a, str>,
    /// What the credential this login mints will be called.
    #[serde(borrow)]
    pub token_name: Cow<'a, str>,
}

/// What opening a login answers with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenSessionResponse<'a> {
    /// The identifier the command line polls on.
    pub session_id: Cow<'a, str>,
    /// The page a person approves the login on.
    pub login_url: Cow<'a, str>,
    /// The request that opened it, for an operator correlating a failed login.
    pub request_id: Cow<'a, str>,
}

/// `GET /v1/auth/sessions/{session_id}` — where a login has got to.
///
/// Never carries ciphertext. The poll is unauthenticated — the id is the only
/// thing presented — so anything it returns is readable by whoever holds the
/// id, and the sealed credential is released only by `/verify`, against a code
/// that never travelled the same channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PollSessionResponse<'a> {
    /// `pending` or `verification_pending`.
    pub status: Cow<'a, str>,
    /// The public key the command line presented, echoed back.
    pub cli_public_key: Cow<'a, str>,
    /// What the credential will be called.
    pub token_name: Cow<'a, str>,
    /// When the window closes, in milliseconds since the epoch.
    pub expires_at_ms: i64,
}

/// `PATCH /v1/auth/sessions/{session_id}/approve` — a person clicked Approve.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApproveSessionRequest<'a> {
    /// The dashboard's public key, for the client's key agreement.
    #[serde(borrow)]
    pub dashboard_public_key: Cow<'a, str>,
    /// The sealed credential. Relayed; this daemon never opens it.
    #[serde(borrow)]
    pub ciphertext: Cow<'a, str>,
    /// The nonce it was sealed under.
    #[serde(borrow)]
    pub nonce: Cow<'a, str>,
    /// The six digits shown to the person, whose digest is what gets stored.
    #[serde(borrow)]
    pub verification_code: Cow<'a, str>,
}

/// What approving answers with.
///
/// The request id and nothing else. An approval has no state worth returning —
/// the dashboard already holds everything it sent — and echoing the session
/// back would put the ciphertext on a second response for no reason.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ApproveSessionResponse<'a> {
    /// The request that recorded the approval.
    pub request_id: Cow<'a, str>,
}

/// `POST /v1/auth/sessions/{session_id}/verify` — the code is presented.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifySessionRequest<'a> {
    /// The six digits a person read out of the browser.
    #[serde(borrow)]
    pub verification_code: Cow<'a, str>,
}

/// What a redeemed login hands back.
///
/// Identical for a first redemption and for a repeat inside the replay window,
/// deliberately: a command line asking again after a dropped connection must
/// not be able to learn that its first request landed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerifySessionResponse<'a> {
    /// The dashboard's public key.
    pub dashboard_public_key: Cow<'a, str>,
    /// The sealed credential.
    pub ciphertext: Cow<'a, str>,
    /// The nonce it was sealed under.
    pub nonce: Cow<'a, str>,
}

/// What aborting every in-flight login answers with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeleteAllSessionsResponse {
    /// How many were aborted by this call.
    pub aborted_count: usize,
}
