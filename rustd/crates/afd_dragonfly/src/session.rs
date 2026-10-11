//! The CLI device-flow session blob, and the one transition that must be atomic.
//!
//! # The script lives here
//!
//! `session/verify_consume.lua` is included from this crate's own tree with
//! `include_str!`, so the bytes every replica sends are the bytes in this
//! crate and nowhere else: a crate that reached into another directory for
//! them would stop building the day that directory moved.
//!
//! Dragonfly evaluates a script body to completion against a single-threaded
//! server, so read-check-write inside `EVAL` has no window. The same sequence
//! written as `GET` then `SET` from the client has one, and that window is
//! exactly where two concurrent verifications both see `verification_pending`
//! and both succeed — a device-flow code redeemed twice.
//!
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::client::Dragonfly;
use crate::error::{self, Result};

/// The commands this store issues, named once each (RULE UFS).
const CMD_SET: &str = "SET";
const CMD_GET: &str = "GET";
const CMD_EVAL: &str = "EVAL";
/// The script tag a first redemption answers with.
const TAG_SUCCESS: &str = "success";
/// The script tag every successful transition answers with.
const TAG_OK: &str = "ok";
/// The script tag a session no longer at the key answers with.
const TAG_MISSING: &str = "missing";
/// The script tag an abort of an already-aborted session answers with.
const TAG_ALREADY_ABORTED: &str = "already_aborted";
/// The script tag an abort by anyone but the owner answers with.
const TAG_NOT_OWNER: &str = "not_owner";
/// The script tag a terminal, already-redeemed session answers with.
const TAG_CONSUMED: &str = "consumed";
/// The script tag a second approval answers with.
const TAG_CONFLICT: &str = "conflict";
/// The script tag a redemption inside the replay window answers with.
const TAG_REPLAY: &str = "replay";
/// The script tag a session past its TTL answers with.
const TAG_EXPIRED: &str = "expired";
/// The script tag an aborted session answers with, carrying the reason.
const TAG_ABORTED: &str = "aborted";
/// The script tag a session whose approval never came answers with.
const TAG_NOT_APPROVED: &str = "not_approved";
/// The script tag a session over its attempt budget answers with.
const TAG_RATE_LIMITED: &str = "rate_limited";
/// The script tag a wrong code answers with, carrying attempts remaining.
const TAG_INVALID_CODE: &str = "invalid_code";

/// The `SCAN` glob that matches every session key.
const SESSION_KEY_GLOB: &str = "auth:session:*";

/// How many keys one `SCAN` page asks for.
///
/// A hint rather than a bound — Dragonfly may answer with more, and
/// [`Dragonfly::scan_keys`] takes whatever comes rather than sizing a buffer for
/// it, so a page larger than the hint cannot fail the scan.
const SCAN_PAGE_HINT: usize = 100;

/// Where a session lives, keyed by its id.
pub const SESSION_KEY_PREFIX: &str = "auth:session:";

/// How long a session survives without being touched.
///
/// Five minutes. It is a device-flow window, not a session lifetime: long
/// enough to paste a code, short enough that an abandoned one is gone before
/// anyone finds it.
pub const SESSION_TTL: Duration = Duration::from_secs(300);

/// The atomic transition, as the bytes this crate ships.
const VERIFY_AND_CONSUME_LUA: &str = include_str!("session/verify_consume.lua");

/// The atomic `pending` -> `verification_pending` transition.
///
/// The state machine is the thing under test: a session past `pending` must
/// be refused here, and the approval tests in this crate's integration lane
/// hold the script to that.
const APPROVE_LUA: &str = include_str!("session/approve.lua");

/// The owner-checked abort, for both the single delete and the bulk sweep.
const ABORT_LUA: &str = include_str!("session/abort.lua");

/// The three scripts above, each prepared once for the life of the process.
///
/// # Why a `Script` rather than `EVAL` with the body
///
/// `EVAL` ships the whole program on every call. These three are the device
/// flow's hot path — an approval gate and a code redemption run one each, per
/// attempt, per user — so the body of `verify_consume.lua` travelled the socket
/// once for every six digits anybody ever typed. `redis::Script` sends the
/// 40-byte digest with `EVALSHA` and falls back to loading the body only when
/// the server has never seen it, which after the first call of a deployment is
/// never. Same script, same atomicity, a fraction of the bytes.
///
/// `streams/once.rs`'s `APPEND_ONCE` has done it this way since M176; these are
/// the sites that had not caught up. The `LazyLock` is what makes the digest
/// computed once instead of per call.
static VERIFY_AND_CONSUME: std::sync::LazyLock<redis::Script> =
    std::sync::LazyLock::new(|| redis::Script::new(VERIFY_AND_CONSUME_LUA));
/// See [`VERIFY_AND_CONSUME`].
static APPROVE: std::sync::LazyLock<redis::Script> =
    std::sync::LazyLock::new(|| redis::Script::new(APPROVE_LUA));
/// See [`VERIFY_AND_CONSUME`].
static ABORT: std::sync::LazyLock<redis::Script> =
    std::sync::LazyLock::new(|| redis::Script::new(ABORT_LUA));

/// How long a consumed session still answers a repeat of the same request.
const CONSUME_REPLAY_WINDOW: Duration = Duration::from_secs(60);

/// How many wrong codes a session tolerates before it aborts itself.
const MAX_VERIFY_ATTEMPTS: u8 = 5;

/// Where a session is in its life. Monotonic: no state goes backwards, and the
/// three terminal ones reject every later mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Created, waiting for a dashboard approval.
    Pending,
    /// Approved, waiting for the CLI to present its code.
    VerificationPending,
    /// Redeemed. Terminal.
    Consumed,
    /// Timed out. Terminal.
    Expired,
    /// Cancelled, replaced, or rate-limited. Terminal.
    Aborted,
}

impl SessionStatus {
    /// Whether this state rejects every further transition.
    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Consumed | Self::Expired | Self::Aborted)
    }
}

/// The blob a session is stored as.
///
/// Field names and JSON shape are a DATA FORMAT, not a convention: the Lua
/// script reads them by name off a key it does not otherwise parse. The
/// hex-encoded fields are hex because Lua has neither bit operations nor crypto
/// across the Dragonfly versions this has to run on, so it compares them as text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionState {
    /// The session's own identifier.
    pub session_id: String,
    /// Where it is in its life.
    pub status: SessionStatus,
    /// The CLI's public key, as presented at creation.
    pub cli_public_key: String,
    /// What the resulting token will be called.
    pub token_name: String,

    /// The dashboard's public key, once approved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dashboard_public_key: Option<String>,
    /// The encrypted payload the CLI collects.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ciphertext: Option<String>,
    /// That payload's nonce.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
    /// Lower-case hex of the HMAC over the verification code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification_code_hmac_hex: Option<String>,

    /// How many wrong codes have been presented.
    #[serde(default)]
    pub verification_attempts: u8,
    /// When it was created, in milliseconds since the epoch.
    pub created_at_ms: i64,
    /// When it expires, in milliseconds since the epoch.
    pub expires_at_ms: i64,
    /// When it was approved, if it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approved_at_ms: Option<i64>,
    /// When it was consumed, if it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consumed_at_ms: Option<i64>,
    /// Why it aborted, if it did.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aborted_reason: Option<String>,
    /// The identity that approved it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clerk_user_id: Option<String>,

    /// The fingerprint of the request that consumed it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consumed_client_fingerprint_hex: Option<String>,
    /// When the consumed payload stops being replayable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub consume_payload_expires_at_ms: Option<i64>,
}

/// What a verify-and-consume attempt did.
///
/// One variant per tag the script returns, so a caller matches on the outcome
/// rather than on a string. The script's contract is documented in its own
/// header; this enum is that contract in the type system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyOutcome {
    /// Redeemed. The payload is the CLI's to decrypt.
    Success(VerifyPayload),
    /// The same request, inside the replay window: the same payload again.
    Replay(VerifyPayload),
    /// No such session — never created, or its time-to-live passed.
    Missing,
    /// Terminal: timed out.
    Expired,
    /// Terminal: cancelled, replaced, or rate-limited, with the reason.
    Aborted(String),
    /// Terminal: already redeemed, and not a replay of the same request.
    Consumed,
    /// Not approved yet, so there is nothing to redeem.
    NotApproved,
    /// Wrong code. The count is how many wrong ones this session has seen.
    InvalidCode(u8),
    /// Wrong code, and that was the last one allowed: the session is aborted.
    RateLimited,
}

/// What a redeemed session hands back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyPayload {
    /// The dashboard's public key.
    pub dashboard_public_key: String,
    /// The encrypted token.
    pub ciphertext: String,
    /// The nonce it was encrypted under.
    pub nonce: String,
}

/// The session key for an id.
#[must_use]
pub fn session_key(session_id: &str) -> String {
    format!("{SESSION_KEY_PREFIX}{session_id}")
}

/// Sessions in Dragonfly.
#[derive(Debug, Clone)]
pub struct SessionStore {
    redis: Dragonfly,
}

mod operations;
#[cfg(feature = "test-util")]
pub use self::operations::outcome_from_reply;

/// What one dashboard approval carries.
///
/// A struct rather than seven positional parameters, because five of them are
/// strings and two of those are opaque base64: a caller that transposed the
/// ciphertext and the nonce would compile, store a session nothing can redeem,
/// and fail in the command line minutes later (`M-TOO-MANY-ARGS`).
#[derive(Debug, Clone, Copy)]
pub struct Approval<'a> {
    /// Which session is being approved.
    pub session_id: &'a str,
    /// The dashboard's public key, relayed verbatim.
    pub dashboard_public_key: &'a str,
    /// The encrypted credential, relayed verbatim and never opened.
    pub ciphertext: &'a str,
    /// The nonce the ciphertext was sealed under.
    pub nonce: &'a str,
    /// Lower-case hex of the peppered code digest.
    pub code_hmac_hex: &'a str,
    /// The identity clicking Approve.
    pub approver: &'a str,
}

/// What an approval attempt did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApproveOutcome {
    /// The session moved to `verification_pending`.
    Approved,
    /// No session at that key — never created, or its time-to-live passed.
    Missing,
    /// The session was already past `pending`, carrying the status it was in.
    Conflict(Option<String>),
}

/// Why a session was aborted.
///
/// A closed set rather than a caller-supplied string: a free-form reason the
/// handler passes and the audit sink separately re-derives is two spellings
/// that agree by convention. Here they are one value, and a reason nobody has
/// declared cannot be written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbortReason {
    /// A person cancelled their own login.
    ExplicitCancel,
    /// The code was presented wrongly too many times.
    RateLimitExceeded,
}

impl AbortReason {
    /// The stored spelling, which both binaries read.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExplicitCancel => "explicit_cancel",
            Self::RateLimitExceeded => "rate_limit_exceeded",
        }
    }
}

/// What an abort attempt did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbortOutcome {
    /// This call performed the abort.
    Aborted,
    /// The session was already aborted, so this call changed nothing.
    ///
    /// Distinguished from [`AbortOutcome::Aborted`] so an audit record is
    /// written on the transition and not on every idempotent repeat of it.
    AlreadyAborted,
    /// No session at that key.
    Missing,
    /// The session belongs to another identity.
    NotOwner,
    /// The session was already redeemed, so there is nothing to abort.
    Consumed,
}
