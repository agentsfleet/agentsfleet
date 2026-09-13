//! The consumer half: reading a fleet's events, acknowledging them, reclaiming
//! what a dead runner left behind.
//!
//! Split from the writer half next door along the line the stream itself draws.
//! Everything here runs on the RUNNER's poll — several times a second, per
//! fleet, forever — where an append runs once per event; the two halves have
//! different costs, different failure modes, and no caller that needs both.

use redis::ToRedisArgs as _;
use redis::streams::{StreamRangeReply, StreamReadOptions, StreamReadReply};

use super::{
    ARG_COUNT, AUTOCLAIM_MIN_IDLE_MS, AUTOCLAIM_START, CMD_XACK, CMD_XAUTOCLAIM, CMD_XRANGE,
    CMD_XREADGROUP, EventId, FLEET_CONSUMER_GROUP, FleetEvent, FleetStreams, NEW_ENTRIES,
    OWN_PENDING, fleet_stream_key, stringify,
};
use crate::error::Result;

/// How many entries the existence probe asks for: the one it named.
const JUST_THE_ONE: usize = 1;

impl FleetStreams {
    /// Reads the next undelivered event, without blocking.
    ///
    /// Never `BLOCK`: this connection is multiplexed, so parking on one stream
    /// would park every other caller sharing it. The assignment scan probes
    /// several fleets per poll and the runner long-polls client-side instead.
    ///
    /// # Errors
    /// Returns a command error, or an unavailable error when Redis is gone. A
    /// vanished group is REPORTED, as a group-missing error, for the caller
    /// holding the ledgers to restore through [`FleetStreams::restore_group`]
    /// — see the module note on why this crate cannot pick the position.
    pub async fn read_new(&self, fleet_id: &str, consumer: &str) -> Result<Option<FleetEvent>> {
        self.read(fleet_id, consumer, NEW_ENTRIES).await
    }

    /// Reads this consumer's oldest pending entry — one delivered but never
    /// acknowledged, which is what a re-poll after a crash has to find first.
    ///
    /// # Errors
    /// As [`FleetStreams::read_new`].
    pub async fn read_pending(&self, fleet_id: &str, consumer: &str) -> Result<Option<FleetEvent>> {
        self.read(fleet_id, consumer, OWN_PENDING).await
    }

    async fn read(
        &self,
        fleet_id: &str,
        consumer: &str,
        read_id: &str,
    ) -> Result<Option<FleetEvent>> {
        let key = fleet_stream_key(fleet_id);
        let options = StreamReadOptions::default()
            .group(FLEET_CONSUMER_GROUP, consumer)
            .count(1);
        let mut cmd = redis::cmd(CMD_XREADGROUP);
        for arg in options.to_redis_args() {
            cmd.arg(arg);
        }
        cmd.arg("STREAMS").arg(&key).arg(read_id);

        let reply: StreamReadReply = self.redis.command(CMD_XREADGROUP, &key, &cmd).await?;
        Ok(reply
            .keys
            .into_iter()
            .flat_map(|stream| stream.ids)
            .next()
            .map(|entry| FleetEvent {
                receipt: EventId(entry.id),
                fields: entry
                    .map
                    .into_iter()
                    .map(|(name, value)| (name, stringify(&value)))
                    .collect(),
            }))
    }

    /// Whether this fleet's stream still holds the entry `receipt` names.
    ///
    /// The recovery path's one question. An admission carries the receipt its
    /// append answered with, and retention never crosses an entry a consumer
    /// still owes (see [`super::retain`]), so a receipt the stream cannot
    /// produce names an entry that was DESTROYED — a flush, a restart without
    /// persistence, a failover to an empty replica. That is accepted work the
    /// producer was told yes about, and the ledger is the only place it
    /// survives.
    ///
    /// Asked of the server rather than computed from an id comparison: the
    /// server knows, and an ordering done here would have to beat the text
    /// ordering under which `999-0` sorts after `1000-0`. `XRANGE` bounds are
    /// inclusive, so naming the receipt as both ends asks for exactly it.
    ///
    /// # Errors
    /// Returns a command error, or an unavailable error when Redis is gone. A
    /// caller must read that as "unknown" and leave the row alone: re-appending
    /// on a stream that would not answer duplicates work it may still hold.
    pub async fn holds_entry(&self, fleet_id: &str, receipt: &EventId) -> Result<bool> {
        let key = fleet_stream_key(fleet_id);
        let mut cmd = redis::cmd(CMD_XRANGE);
        cmd.arg(&key)
            .arg(receipt.as_str())
            .arg(receipt.as_str())
            .arg(ARG_COUNT)
            .arg(JUST_THE_ONE);
        let reply: StreamRangeReply = self.redis.command(CMD_XRANGE, &key, &cmd).await?;
        Ok(!reply.ids.is_empty())
    }

    /// Acknowledges an event, removing it from the consumer's pending list.
    ///
    /// # Errors
    /// Returns a command error when the acknowledgement fails.
    pub async fn ack(&self, fleet_id: &str, id: &EventId) -> Result<bool> {
        let key = fleet_stream_key(fleet_id);
        let mut cmd = redis::cmd(CMD_XACK);
        cmd.arg(&key).arg(FLEET_CONSUMER_GROUP).arg(id.as_str());
        let acknowledged: i64 = self.redis.command(CMD_XACK, &key, &cmd).await?;
        Ok(acknowledged > 0)
    }

    /// Claims one entry stranded in a dead consumer's pending list.
    ///
    /// Entries delivered to a consumer that no longer reads — a retired daemon
    /// instance, a legacy per-probe consumer name — sit in that consumer's
    /// pending list forever, because `XREADGROUP >` only ever hands out entries
    /// nobody has seen. Nothing recovers them except claiming them away, which
    /// is what this does; the lease path's own-pending read then re-enters the
    /// entry into the lease flow on the next poll.
    ///
    /// One entry per call, so a pathological stream cannot monopolise a sweep
    /// pass. `None` means the pending list held nothing idle enough, which is
    /// the ordinary answer for a healthy fleet.
    ///
    /// # Errors
    /// Returns a command error, or an unavailable error when Redis is gone.
    pub async fn autoclaim(&self, fleet_id: &str, consumer: &str) -> Result<Option<FleetEvent>> {
        let key = fleet_stream_key(fleet_id);
        let mut cmd = redis::cmd(CMD_XAUTOCLAIM);
        cmd.arg(&key)
            .arg(FLEET_CONSUMER_GROUP)
            .arg(consumer)
            .arg(AUTOCLAIM_MIN_IDLE_MS)
            .arg(AUTOCLAIM_START)
            .arg(ARG_COUNT)
            .arg(1);

        // The typed reply is the crate's. `redis_fleet_decode.zig` hand-decodes
        // the same nested array — a length check, two index reads and a field
        // walk — for want of one.
        let reply: redis::streams::StreamAutoClaimReply =
            self.redis.command(CMD_XAUTOCLAIM, &key, &cmd).await?;
        Ok(reply.claimed.into_iter().next().map(|entry| FleetEvent {
            receipt: EventId(entry.id),
            fields: entry
                .map
                .into_iter()
                .map(|(name, value)| (name, stringify(&value)))
                .collect(),
        }))
    }
}
