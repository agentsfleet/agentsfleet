//! The CLI device-flow session blob, and the one transition that must be atomic.
//!
//! # The script lives here, and is proven identical to the Zig one
//!
//! `session/verify_consume.lua` is this crate's own copy, included from this
//! crate's own tree — because M181 deleted the Zig daemon tree at cutover, and
//! a crate that reaches into a directory scheduled for deletion stops building
//! the day it goes. Reaching across was the first shape and it was wrong for
//! exactly that reason.
//!
//! What keeps the two honest while both exist is a test, not a path:
//! `test_the_verify_script_matches_the_zig_daemons` compares the two files
//! BYTE FOR BYTE and fails on any drift. Both binaries therefore send the same
//! script, and when the Zig copy is deleted the test goes with it, leaving this
//! one as the source of truth rather than a fork nobody noticed.
//!
//! Redis evaluates a script body to completion against a single-threaded
//! server, so read-check-write inside `EVAL` has no window. The same sequence
//! written as `GET` then `SET` from the client has one, and that window is
//! exactly where two concurrent verifications both see `verification_pending`
//! and both succeed — a device-flow code redeemed twice.
//!
//! # What this milestone carries, and what it does not
//!
//! §3 is the store: the key, the time-to-live, the blob, and the atomic
//! verify-and-consume. The device-flow surface around it — approve, the audit
//! peppers, request fingerprinting, the replay window's callers — is §4's and
//! M178's. The blob type here carries every field the script reads, because a
//! partial blob would fail the script rather than fail a test.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::client::Redis;
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
/// A hint rather than a bound — Redis may answer with more, and
/// [`Redis::scan_keys`] takes whatever comes rather than sizing a buffer for
/// it, which is the one place the Zig scan can fail on a page it did not
/// expect.
const SCAN_PAGE_HINT: usize = 100;

/// Where a session lives, keyed by its id.
pub const SESSION_KEY_PREFIX: &str = "auth:session:";

/// How long a session survives without being touched.
///
/// Five minutes, matching `SESSION_TTL_SECONDS` on the Zig side. It is a
/// device-flow window, not a session lifetime: long enough to paste a code,
/// short enough that an abandoned one is gone before anyone finds it.
pub const SESSION_TTL: Duration = Duration::from_secs(300);

/// The atomic transition. Byte-identical to the Zig daemon's copy, which a
/// test asserts for as long as that copy exists.
const VERIFY_AND_CONSUME_LUA: &str = include_str!("session/verify_consume.lua");

/// The atomic `pending` -> `verification_pending` transition.
///
/// Not byte-pinned to the Zig copy, and deliberately: that one is assembled
/// from four concatenated Zig string literals with the tag names spliced in,
/// so there is no single file to compare against. What the two share is the
/// state machine, and `test_approve_refuses_a_session_past_pending` is what
/// holds this copy to it.
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
/// Field names and JSON shape match `auth/session_state.zig` exactly, because
/// the Lua script reads them by name and both binaries write the same key. The
/// hex-encoded fields are hex because Lua has neither bit operations nor crypto
/// across the Redis versions this has to run on, so it compares them as text.
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

/// Sessions in Redis.
#[derive(Debug, Clone)]
pub struct SessionStore {
    redis: Redis,
}

impl SessionStore {
    /// Binds session operations to a connection.
    #[must_use]
    pub const fn new(redis: Redis) -> Self {
        Self { redis }
    }

    /// Writes a session, refreshing its time-to-live.
    ///
    /// # Errors
    /// Returns a command error when the write fails.
    pub async fn put(&self, state: &SessionState) -> Result<()> {
        let key = session_key(&state.session_id);
        let blob =
            serde_json::to_string(state).map_err(|_json| error::unexpected_reply(CMD_SET))?;
        let mut cmd = redis::cmd(CMD_SET);
        cmd.arg(&key).arg(blob).arg("EX").arg(SESSION_TTL.as_secs());
        let _: String = self.redis.command(CMD_SET, &key, &cmd).await?;
        Ok(())
    }

    /// Reads a session, or `None` when it never existed or has expired.
    ///
    /// # Errors
    /// Returns a command error when the read fails, and an unexpected-reply
    /// error when the stored blob is not a session.
    pub async fn get(&self, session_id: &str) -> Result<Option<SessionState>> {
        let key = session_key(session_id);
        let mut cmd = redis::cmd(CMD_GET);
        cmd.arg(&key);
        let blob: Option<String> = self.redis.command(CMD_GET, &key, &cmd).await?;
        blob.map(|text| {
            serde_json::from_str(&text).map_err(|_json| error::unexpected_reply("session blob"))
        })
        .transpose()
    }

    /// Presents a code, redeeming the session if it matches.
    ///
    /// Every check and every write happen inside one script evaluation, so two
    /// concurrent presentations cannot both succeed. That is the whole point;
    /// see the module documentation.
    ///
    /// # Errors
    /// Returns a command error when the evaluation fails, and an
    /// unexpected-reply error when the script answers with a shape this client
    /// does not know.
    pub async fn verify_and_consume(
        &self,
        session_id: &str,
        submitted_hmac_hex: &str,
        now_ms: i64,
        request_fingerprint_hex: &str,
    ) -> Result<VerifyOutcome> {
        let key = session_key(session_id);
        let mut invocation = VERIFY_AND_CONSUME.prepare_invoke();
        invocation
            .key(&key)
            .arg(submitted_hmac_hex)
            .arg(now_ms)
            .arg(request_fingerprint_hex)
            .arg(CONSUME_REPLAY_WINDOW.as_millis())
            .arg(MAX_VERIFY_ATTEMPTS)
            .arg(SESSION_TTL.as_secs());

        let reply: Vec<String> = self.redis.script(CMD_EVAL, &key, &invocation).await?;
        parse_outcome(&reply)
    }

    /// Moves a pending session to `verification_pending`, once.
    ///
    /// Every value is relayed, not interpreted: the daemon performs no
    /// elliptic-curve operation for device flow, so the public key and the
    /// ciphertext are opaque strings it stores and hands back. The code is the
    /// exception, and only as a digest — `code_hmac_hex` is what the caller
    /// computed, and the plaintext six digits never reach this crate.
    ///
    /// # Errors
    /// Returns a command error when the evaluation fails, and an
    /// unexpected-reply error when the script answers with a shape this client
    /// does not know.
    pub async fn approve(&self, approval: &Approval<'_>, now_ms: i64) -> Result<ApproveOutcome> {
        let key = session_key(approval.session_id);
        let mut invocation = APPROVE.prepare_invoke();
        invocation
            .key(&key)
            .arg(approval.dashboard_public_key)
            .arg(approval.ciphertext)
            .arg(approval.nonce)
            .arg(approval.code_hmac_hex)
            .arg(approval.approver)
            .arg(now_ms)
            .arg(SESSION_TTL.as_secs());

        let reply: Vec<String> = self.redis.script(CMD_EVAL, &key, &invocation).await?;
        match reply.first().map(String::as_str) {
            Some(TAG_OK) => Ok(ApproveOutcome::Approved),
            Some(TAG_MISSING) => Ok(ApproveOutcome::Missing),
            // The status the script found is carried through rather than
            // dropped: an approve that lost a race and an approve of an
            // already-consumed session are both conflicts, and only the status
            // says which — the caller renders one refusal and logs the other.
            Some(TAG_CONFLICT) => Ok(ApproveOutcome::Conflict(reply.get(1).cloned())),
            _ => Err(error::unexpected_reply("approve")),
        }
    }

    /// Aborts one session, if `owner` is the identity that approved it.
    ///
    /// The ownership check rides inside the script for the reason the whole
    /// transition does: split into a read and a write, the window between them
    /// is where a session the command line has just redeemed gets aborted
    /// anyway.
    ///
    /// # Errors
    /// As [`SessionStore::approve`].
    pub async fn abort(
        &self,
        session_id: &str,
        owner: &str,
        reason: AbortReason,
    ) -> Result<AbortOutcome> {
        let key = session_key(session_id);
        self.abort_key(&key, owner, reason).await
    }

    /// Aborts every in-flight session `owner` holds, answering how many.
    ///
    /// Scans rather than reading an index, exactly as the Zig daemon does,
    /// because there is no per-owner index to read: sessions are keyed by their
    /// own id and live five minutes, so the set walked here is bounded by that
    /// window rather than by how long the tenant has existed.
    ///
    /// # Errors
    /// As [`SessionStore::approve`]. A page that fails ends the walk — the
    /// count returned by a partial sweep would claim a completeness it does not
    /// have.
    pub async fn abort_all_for_owner(
        &self,
        owner: &str,
        reason: AbortReason,
    ) -> Result<Vec<String>> {
        let keys = self
            .redis
            .scan_keys(SESSION_KEY_GLOB, SCAN_PAGE_HINT)
            .await?;

        let mut aborted = Vec::new();
        for key in &keys {
            if self.abort_key(key, owner, reason).await? == AbortOutcome::Aborted {
                // The id, not the key: every caller above this one talks in
                // session ids, and the prefix is this module's business.
                if let Some(id) = key.strip_prefix(SESSION_KEY_PREFIX) {
                    aborted.push(id.to_owned());
                }
            }
        }
        Ok(aborted)
    }

    /// Aborts the session at one already-built key.
    async fn abort_key(&self, key: &str, owner: &str, reason: AbortReason) -> Result<AbortOutcome> {
        let mut invocation = ABORT.prepare_invoke();
        invocation
            .key(key)
            .arg(owner)
            .arg(reason.as_str())
            .arg(SESSION_TTL.as_secs());

        let reply: Vec<String> = self.redis.script(CMD_EVAL, key, &invocation).await?;
        match reply.first().map(String::as_str) {
            Some(TAG_OK) => Ok(AbortOutcome::Aborted),
            Some(TAG_ALREADY_ABORTED) => Ok(AbortOutcome::AlreadyAborted),
            Some(TAG_MISSING) => Ok(AbortOutcome::Missing),
            Some(TAG_NOT_OWNER) => Ok(AbortOutcome::NotOwner),
            Some(TAG_CONSUMED) => Ok(AbortOutcome::Consumed),
            _ => Err(error::unexpected_reply("abort")),
        }
    }
}

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
/// A closed set rather than a caller-supplied string, which is the one place
/// this deliberately diverges from the Zig store. There the reason is a
/// `[]const u8` the handler passes and the audit sink separately re-derives, so
/// the stored reason and the audited one are two spellings that agree by
/// convention. Here they are one value, and a reason nobody has declared cannot
/// be written.
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

/// Turns the script's tagged array into the outcome it describes.
///
/// Exposed under `test-util` because it is the whole of this module that can be
/// checked without a server: the script's return contract is nine tags, and a
/// tag read as the wrong outcome is a device-flow bug that a live test would
/// only catch for whichever tag it happened to produce.
///
/// # Errors
/// Returns an unexpected-reply error when the tag is not one the script emits,
/// or a tagged reply is missing the fields that tag carries.
#[cfg(feature = "test-util")]
pub fn outcome_from_reply(reply: &[String]) -> Result<VerifyOutcome> {
    parse_outcome(reply)
}

/// Turns the script's tagged array into the outcome it describes.
fn parse_outcome(reply: &[String]) -> Result<VerifyOutcome> {
    let unexpected = || error::unexpected_reply("verify-and-consume");
    let tag = reply.first().ok_or_else(unexpected)?.as_str();
    let field = |index: usize| reply.get(index).cloned().ok_or_else(unexpected);

    match tag {
        TAG_SUCCESS | TAG_REPLAY => {
            let payload = VerifyPayload {
                dashboard_public_key: field(1)?,
                ciphertext: field(2)?,
                nonce: field(3)?,
            };
            Ok(if tag == TAG_SUCCESS {
                VerifyOutcome::Success(payload)
            } else {
                VerifyOutcome::Replay(payload)
            })
        }
        TAG_MISSING => Ok(VerifyOutcome::Missing),
        TAG_EXPIRED => Ok(VerifyOutcome::Expired),
        TAG_ABORTED => Ok(VerifyOutcome::Aborted(field(1)?)),
        TAG_CONSUMED => Ok(VerifyOutcome::Consumed),
        TAG_NOT_APPROVED => Ok(VerifyOutcome::NotApproved),
        TAG_RATE_LIMITED => Ok(VerifyOutcome::RateLimited),
        TAG_INVALID_CODE => Ok(VerifyOutcome::InvalidCode(
            field(1)?.parse().map_err(|_parse| unexpected())?,
        )),
        _ => Err(unexpected()),
    }
}
