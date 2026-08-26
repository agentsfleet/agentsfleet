//! The readiness index: which fleets currently hold work.
//!
//! One global hash, field per fleet, value a token. A lease poll reads this
//! before it opens a Postgres connection, so an idle poll costs one bounded
//! Redis read and no database round-trip at all.
//!
//! # It is a hint, never the record
//!
//! The streams are the record. A lost mark costs delivery latency, never the
//! event — the reclaim sweeper re-derives readiness from the streams on a pass
//! it already makes. So every write here is best-effort, and no failure may
//! propagate into an accepted ingress call.
//!
//! # Why the field carries a token
//!
//! A poll that finds a fleet holds nothing clears it. Ingress takes no per-fleet
//! claim, so it can append and mark at any instant — including inside the gap
//! between that poll's last read and its clear. An unconditional delete would
//! erase a mark for genuinely undelivered work, and nothing would rediscover it
//! until a sweep. So a clear deletes a field only when the token still matches
//! the one the caller saw, and the comparison happens inside Redis where there
//! is no gap.

use crate::client::Redis;
use crate::error::Result;

/// The one index key for the whole deployment.
pub const READY_INDEX_KEY: &str = "fleet:ready";

/// Delete the field only if it still carries the token the caller observed.
///
/// A client-side read-then-delete does not express this: the gap between the
/// read and the delete is exactly the window a concurrent mark wins.
const CLEAR_IF_TOKEN_MATCHES: &str = r"
if redis.call('HGET', KEYS[1], ARGV[1]) == ARGV[2] then
  return redis.call('HDEL', KEYS[1], ARGV[1])
end
return 0
";

/// The commands this index issues. Named once each (RULE UFS): a verb
/// spelled twice is a verb that can be spelled two ways.
const CMD_HSET: &str = "HSET";
const CMD_HLEN: &str = "HLEN";
const CMD_HRANDFIELD: &str = "HRANDFIELD";
const CMD_EVAL: &str = "EVAL";
const CMD_HDEL: &str = "HDEL";

/// Names one generation of a fleet's readiness mark.
///
/// Minted rather than counted: nothing ever compares two tokens for order, only
/// for equality, and every counter shape breaks on reuse — a clear deletes the
/// field, so a per-fleet count restarts and re-mints a token a stale poll still
/// holds.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ReadyToken(String);

impl ReadyToken {
    /// The token as stored.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One ready fleet and the token its last mark minted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ready {
    /// The fleet holding work.
    pub fleet_id: String,
    /// The generation token to pass back to [`ReadyIndex::clear_if_unchanged`].
    pub token: ReadyToken,
}

/// The readiness index against one connection.
#[derive(Debug, Clone)]
pub struct ReadyIndex {
    redis: Redis,
}

impl ReadyIndex {
    /// Binds index operations to a connection.
    #[must_use]
    pub const fn new(redis: Redis) -> Self {
        Self { redis }
    }

    /// Marks a fleet as holding work under `token`.
    ///
    /// The caller mints the token, because the caller is the ingress path that
    /// already has an identifier to hand and this module has no business
    /// deciding what generation means. `afd_core::id::Uuid7` parses one today;
    /// minting arrives with the crate that needs to mint.
    ///
    /// # Errors
    /// Returns a command error when the write fails. Callers on the ingress
    /// path log and continue: the append already succeeded, and the sweeper
    /// re-derives what a lost mark would have said.
    pub async fn mark(&self, fleet_id: &str, token: &str) -> Result<ReadyToken> {
        let value = token.to_owned();
        let mut cmd = redis::cmd(CMD_HSET);
        cmd.arg(READY_INDEX_KEY).arg(fleet_id).arg(&value);
        let _: i64 = self.redis.command(CMD_HSET, READY_INDEX_KEY, &cmd).await?;
        Ok(ReadyToken(value))
    }

    /// How many fleets the index currently holds.
    ///
    /// # Errors
    /// Returns a command error when the read fails.
    pub async fn len(&self) -> Result<u64> {
        let mut cmd = redis::cmd(CMD_HLEN);
        cmd.arg(READY_INDEX_KEY);
        self.redis.command(CMD_HLEN, READY_INDEX_KEY, &cmd).await
    }

    /// Whether the index holds nothing.
    ///
    /// # Errors
    /// As [`ReadyIndex::len`].
    pub async fn is_empty(&self) -> Result<bool> {
        Ok(self.len().await? == 0)
    }

    /// Samples up to `count` ready fleets.
    ///
    /// Random rather than ordered, because every replica polls this index and
    /// an ordered read would send all of them at the same fleet first.
    ///
    /// # Errors
    /// Returns a command error when the read fails.
    pub async fn peek(&self, count: usize) -> Result<Vec<Ready>> {
        let mut cmd = redis::cmd(CMD_HRANDFIELD);
        cmd.arg(READY_INDEX_KEY).arg(count).arg("WITHVALUES");
        let flat: Vec<String> = self
            .redis
            .command(CMD_HRANDFIELD, READY_INDEX_KEY, &cmd)
            .await?;
        // `HRANDFIELD … WITHVALUES` answers a flat field/value list, so the
        // pairing is positional. `as_chunks` proves the width to the compiler
        // rather than leaving a remainder case nobody handles.
        let (pairs, _remainder) = flat.as_chunks::<2>();
        Ok(pairs
            .iter()
            .map(|[fleet_id, token]| Ready {
                fleet_id: fleet_id.clone(),
                token: ReadyToken(token.clone()),
            })
            .collect())
    }

    /// Clears a fleet unconditionally, whatever its mark says.
    ///
    /// The token comparison exists to stop a stale poll from clearing a fleet
    /// that has since taken on new work. This is the case where that question
    /// does not arise: the fleet has been PAUSED, so the candidate query — which
    /// filters `status = 'active'` — will never return it again, and a field
    /// left behind names work no poll can reach. The poll-site clear is
    /// unreachable for the same reason, which is why the pause path has to do
    /// it here.
    ///
    /// # Errors
    /// Returns a command error when the delete fails. Callers treat it as
    /// best-effort: a stale field costs one wasted candidate check on a later
    /// poll, and the fleet is already stopped where it counts.
    pub async fn force_clear(&self, fleet_id: &str) -> Result<()> {
        let mut cmd = redis::cmd(CMD_HDEL);
        cmd.arg(READY_INDEX_KEY).arg(fleet_id);
        let _: i64 = self.redis.command(CMD_HDEL, READY_INDEX_KEY, &cmd).await?;
        Ok(())
    }

    /// Clears a fleet, but only if its mark is still the one observed.
    ///
    /// Returns whether the field was actually removed. `false` means an ingress
    /// mark landed in between and the fleet holds newer work — which is the
    /// case this whole design exists for.
    ///
    /// # Errors
    /// Returns a command error when the evaluation fails.
    pub async fn clear_if_unchanged(&self, fleet_id: &str, token: &ReadyToken) -> Result<bool> {
        let mut cmd = redis::cmd(CMD_EVAL);
        cmd.arg(CLEAR_IF_TOKEN_MATCHES)
            .arg(1)
            .arg(READY_INDEX_KEY)
            .arg(fleet_id)
            .arg(token.as_str());
        let removed: i64 = self.redis.command(CMD_EVAL, READY_INDEX_KEY, &cmd).await?;
        Ok(removed > 0)
    }
}
