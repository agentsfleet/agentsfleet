//! The CLI device-flow session blob, and the one transition that must be atomic.
//!
//! # The script lives here, and is proven identical to the Zig one
//!
//! `session/verify_consume.lua` is this crate's own copy, included from this
//! crate's own tree — because M181 deletes `src/agentsfleetd/` at cutover, and
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
        let mut cmd = redis::cmd(CMD_EVAL);
        cmd.arg(VERIFY_AND_CONSUME_LUA)
            .arg(1)
            .arg(&key)
            .arg(submitted_hmac_hex)
            .arg(now_ms.to_string())
            .arg(request_fingerprint_hex)
            .arg(CONSUME_REPLAY_WINDOW.as_millis().to_string())
            .arg(MAX_VERIFY_ATTEMPTS.to_string())
            .arg(SESSION_TTL.as_secs().to_string());

        let reply: Vec<String> = self.redis.command(CMD_EVAL, &key, &cmd).await?;
        parse_outcome(&reply)
    }
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
        TAG_SUCCESS | "replay" => {
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
        "missing" => Ok(VerifyOutcome::Missing),
        "expired" => Ok(VerifyOutcome::Expired),
        "aborted" => Ok(VerifyOutcome::Aborted(field(1)?)),
        "consumed" => Ok(VerifyOutcome::Consumed),
        "not_approved" => Ok(VerifyOutcome::NotApproved),
        "rate_limited" => Ok(VerifyOutcome::RateLimited),
        "invalid_code" => Ok(VerifyOutcome::InvalidCode(
            field(1)?.parse().map_err(|_parse| unexpected())?,
        )),
        _ => Err(unexpected()),
    }
}
