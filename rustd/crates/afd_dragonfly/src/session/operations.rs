//! Session transitions, each performed atomically on one scoped key.

use super::{
    ABORT, APPROVE, AbortOutcome, AbortReason, Approval, ApproveOutcome, CMD_EVAL, CMD_GET,
    CMD_SET, CONSUME_REPLAY_WINDOW, Dragonfly, MAX_VERIFY_ATTEMPTS, Result, SCAN_PAGE_HINT,
    SESSION_KEY_GLOB, SESSION_KEY_PREFIX, SESSION_TTL, SessionState, SessionStore, TAG_ABORTED,
    TAG_ALREADY_ABORTED, TAG_CONFLICT, TAG_CONSUMED, TAG_EXPIRED, TAG_INVALID_CODE, TAG_MISSING,
    TAG_NOT_APPROVED, TAG_NOT_OWNER, TAG_OK, TAG_RATE_LIMITED, TAG_REPLAY, TAG_SUCCESS,
    VERIFY_AND_CONSUME, VerifyOutcome, VerifyPayload, error, session_key,
};

impl SessionStore {
    /// Binds session operations to a connection.
    #[must_use]
    pub const fn new(redis: Dragonfly) -> Self {
        Self { redis }
    }

    /// Writes a session, refreshing its time-to-live.
    ///
    /// # Errors
    /// Returns a command error when the write fails.
    pub async fn put(&self, state: &SessionState) -> Result<()> {
        let key = self.redis.key(&session_key(&state.session_id));
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
        let key = self.redis.key(&session_key(session_id));
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
        let key = self.redis.key(&session_key(session_id));
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
        let key = self.redis.key(&session_key(approval.session_id));
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
    /// Scans rather than reading an index, because there is no per-owner
    /// index to read: sessions are keyed by their
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
        let key = &self.redis.key(key);
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
pub(super) fn parse_outcome(reply: &[String]) -> Result<VerifyOutcome> {
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
