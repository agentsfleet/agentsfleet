//! The consumer half: reading a fleet's events, acknowledging them, reclaiming
//! what a dead runner left behind.
//!
//! Split from the writer half next door along the line the stream itself draws.
//! Everything here runs on the RUNNER's poll — several times a second, per
//! fleet, forever — where an append runs once per event; the two halves have
//! different costs, different failure modes, and no caller that needs both.

use redis::ToRedisArgs as _;
use redis::streams::{StreamReadOptions, StreamReadReply};

use super::{
    AUTOCLAIM_MIN_IDLE_MS, AUTOCLAIM_START, CMD_XACK, CMD_XAUTOCLAIM, CMD_XREADGROUP, EventId,
    FLEET_CONSUMER_GROUP, FleetEvent, FleetStreams, NEW_ENTRIES, OWN_PENDING, fleet_stream_key,
    stringify,
};
use crate::error::Result;

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
            .arg("COUNT")
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
