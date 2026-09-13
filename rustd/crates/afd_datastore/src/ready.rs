//! The readiness index: which fleets currently hold work.
//!
//! A fixed set of hashes — one per [`Partition`] — with a field per fleet and
//! a token for its value. A lease poll reads ONE partition, the one its
//! cursor names, before it opens a Postgres connection, so an idle poll costs
//! one bounded Redis read and no database round-trip at all, and a rotation
//! of polls visits every partition whatever one of them holds.
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

pub mod partition;

use futures_util::future::try_join_all;

use crate::client::Redis;
use crate::error::Result;

pub use self::partition::{Partition, READY_INDEX_KEY, READY_PARTITIONS, ReadyCursor};

/// Delete the field only if it still carries the token the caller observed.
///
/// A client-side read-then-delete does not express this: the gap between the
/// read and the delete is exactly the window a concurrent mark wins. One key
/// — the fleet's partition — so it runs on a cluster unchanged.
const CLEAR_IF_TOKEN_MATCHES: &str = r"
if redis.call('HGET', KEYS[1], ARGV[1]) == ARGV[2] then
  return redis.call('HDEL', KEYS[1], ARGV[1])
end
return 0
";

/// [`CLEAR_IF_TOKEN_MATCHES`], prepared once for the life of the process.
///
/// `EVAL` ships the body on every call, and this one runs on the lease-poll
/// path — once per fleet per poll, for every fleet a deployment holds. The
/// digest goes over the wire instead, via `EVALSHA`, with the body loaded only
/// when the server has never seen it.
static CLEAR_IF_TOKEN_MATCHES_SCRIPT: std::sync::LazyLock<redis::Script> =
    std::sync::LazyLock::new(|| redis::Script::new(CLEAR_IF_TOKEN_MATCHES));

/// The commands this index issues. Named once each (RULE UFS): a verb
/// spelled twice is a verb that can be spelled two ways.
const CMD_HSET: &str = "HSET";
const CMD_HLEN: &str = "HLEN";
const CMD_HRANDFIELD: &str = "HRANDFIELD";
const CMD_EVAL: &str = "EVAL";
const CMD_HDEL: &str = "HDEL";
const ARG_WITHVALUES: &str = "WITHVALUES";

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
        let key = Partition::of(fleet_id).key();
        let mut cmd = redis::cmd(CMD_HSET);
        cmd.arg(&key).arg(fleet_id).arg(&value);
        let _: i64 = self.redis.command(CMD_HSET, &key, &cmd).await?;
        Ok(ReadyToken(value))
    }

    /// How many fleets one partition currently holds.
    ///
    /// # Errors
    /// Returns a command error when the read fails.
    pub async fn len_of(&self, partition: Partition) -> Result<u64> {
        let key = partition.key();
        let mut cmd = redis::cmd(CMD_HLEN);
        cmd.arg(&key);
        self.redis.command(CMD_HLEN, &key, &cmd).await
    }

    /// How many fleets the index currently holds across every partition.
    ///
    /// The partitions are independent keys on independent slots, so they are
    /// asked concurrently rather than one after another.
    ///
    /// # Errors
    /// Returns a command error when any partition's read fails.
    pub async fn len(&self) -> Result<u64> {
        let counts = try_join_all(Partition::all().map(|partition| self.len_of(partition))).await?;
        Ok(counts.into_iter().sum())
    }

    /// Whether the index holds nothing.
    ///
    /// # Errors
    /// As [`ReadyIndex::len`].
    pub async fn is_empty(&self) -> Result<bool> {
        Ok(self.len().await? == 0)
    }

    /// Samples up to `count` ready fleets from one partition.
    ///
    /// Random within the partition rather than ordered, because every replica
    /// polls this index and an ordered read would send all of them at the
    /// same fleet first. Which partition is the caller's cursor's decision —
    /// see [`ReadyCursor`] — so that a rotation of polls reaches every one.
    ///
    /// # Errors
    /// Returns a command error when the read fails.
    pub async fn peek(&self, partition: Partition, count: usize) -> Result<Vec<Ready>> {
        let key = partition.key();
        let mut cmd = redis::cmd(CMD_HRANDFIELD);
        cmd.arg(&key).arg(count).arg(ARG_WITHVALUES);
        // RESP3 answers `WITHVALUES` as an array of pairs; the driver's pair
        // decoder also accepts the flat RESP2 framing, so either wire shape
        // lands here as (field, value).
        let pairs: Vec<(String, String)> = self.redis.command(CMD_HRANDFIELD, &key, &cmd).await?;
        Ok(pairs
            .into_iter()
            .map(|(fleet_id, token)| Ready {
                fleet_id,
                token: ReadyToken(token),
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
        let key = Partition::of(fleet_id).key();
        let mut cmd = redis::cmd(CMD_HDEL);
        cmd.arg(&key).arg(fleet_id);
        let _: i64 = self.redis.command(CMD_HDEL, &key, &cmd).await?;
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
        let key = Partition::of(fleet_id).key();
        let mut invocation = CLEAR_IF_TOKEN_MATCHES_SCRIPT.prepare_invoke();
        invocation.key(&key).arg(fleet_id).arg(token.as_str());
        let removed: i64 = self.redis.script(CMD_EVAL, &key, &invocation).await?;
        Ok(removed > 0)
    }
}
