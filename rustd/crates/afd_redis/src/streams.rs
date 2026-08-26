//! Per-fleet event streams: append, read as a group, acknowledge.
//!
//! # The entry id IS the event id
//!
//! `XADD … *` makes Redis mint the id, and that id is the canonical
//! `event_id` the API returns and the runner correlates on — there is no second
//! identifier anywhere. [`EventId`] exists to say so in the type system: it is
//! produced by an append and consumed by an acknowledgement, so an id from
//! somewhere else cannot be passed to `XACK` by accident.
//!
//! # A missing group repairs itself, once, at the stream's end
//!
//! The group is created on the write path, so the steady state here is a plain
//! read with no setup command in front of it. It can still vanish — deleted out
//! of band, a restart without persistence, a failover to an empty replica — and
//! every one of those announces itself the same way: `NOGROUP` on the next read.
//!
//! The repair recreates it at `$`, the stream's newest entry, and reads again
//! exactly once. Not at `0`: the stream retains up to its trim length of
//! entries that were already delivered and acknowledged under the vanished
//! group, and a group recreated at `0` hands every one of them out again —
//! historical agent runs re-executing with real provider spend and real
//! connector writes. Recreated at `$`, nothing historical re-runs; the cost is
//! that entries appended during the groupless window are skipped. That loss is
//! bounded and repairable by re-submission. A re-executed run cannot be
//! un-spent. (`redis_fleet.zig` reasons the same way, at length.)

use redis::ToRedisArgs as _;
use redis::streams::{StreamReadOptions, StreamReadReply};

use crate::client::Redis;
use crate::error::{self, Result};

/// The commands this module issues, named once each (RULE UFS).
const CMD_XADD: &str = "XADD";
const CMD_XGROUP: &str = "XGROUP";
const CMD_XREADGROUP: &str = "XREADGROUP";
const CMD_XACK: &str = "XACK";
const CMD_PUBLISH: &str = "PUBLISH";
const CMD_XAUTOCLAIM: &str = "XAUTOCLAIM";
const CMD_XINFO: &str = "XINFO";
const CMD_EVAL: &str = "EVAL";
const CMD_DEL: &str = "DEL";

/// Where an append-once key lives.
const ONCE_KEY_PREFIX: &str = "fleet:repair-verification:";

/// What the script says when the event was already there.
const OUTCOME_REPLAYED: &str = "replayed";

/// Appends one event and remembers that it did, atomically.
///
/// Lua because the two operations must not be separable: a `SET NX` and an
/// `XADD` as two round trips leave a crash-shaped hole between them, and the
/// caller retrying through that hole is exactly the case this exists for.
///
/// The type check is not defensive padding — a key that is not a stream means
/// something else in this deployment is using the name, and appending to it
/// would corrupt whatever that is.
static APPEND_ONCE: std::sync::LazyLock<redis::Script> = std::sync::LazyLock::new(|| {
    redis::Script::new(
        r"local existing = redis.call('GET', KEYS[1])
if existing then return {existing, 'replayed'} end
local kind = redis.call('TYPE', KEYS[2]).ok
if kind ~= 'none' and kind ~= 'stream' then
  return redis.error_reply('append-once target is not a stream')
end
local event_id = redis.call('XADD', KEYS[2], 'MAXLEN', '~', ARGV[1], '*', unpack(ARGV, 2))
redis.call('SET', KEYS[1], event_id)
return {event_id, 'emitted'}",
    )
});

/// What an append-once call did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Appended {
    /// The event's identifier — this call's, or the earlier call's.
    pub id: EventId,
    /// Whether an earlier call already wrote it.
    pub replayed: bool,
}

/// Consumer group every fleet stream is read under.
pub const FLEET_CONSUMER_GROUP: &str = "fleet_lease";

/// Approximate cap on a fleet stream's retained entries (`MAXLEN ~ 10000`).
const STREAM_MAXLEN: usize = 10_000;

/// How long an entry must have sat undelivered before it may be claimed away
/// from the consumer holding it.
///
/// Comfortably past the lease window, which is what stops the sweep racing live
/// work: a consumer still working an entry has not been idle this long, and one
/// that has is a retired instance or a legacy throwaway consumer name.
const AUTOCLAIM_MIN_IDLE_MS: usize = 300_000;

/// Where an autoclaim scan starts, and how many entries it takes.
///
/// Always from the beginning of the pending list: a claimed entry's idle clock
/// RESETS, so the same entry is not eligible twice and the scan makes progress
/// without a cursor to carry.
const AUTOCLAIM_START: &str = "0-0";

/// Read id meaning "entries never delivered to any consumer".
const NEW_ENTRIES: &str = ">";

/// Read id meaning "this consumer's own pending entries, oldest first".
const OWN_PENDING: &str = "0";

/// Group start id for a stream that is brand new, where "from the beginning"
/// and "from now" are the same position.
const GROUP_START_BEGIN: &str = "0";

/// Group start id for a repair, where they are emphatically not the same.
const GROUP_START_END: &str = "$";

/// The key a fleet's events live on.
#[must_use]
pub fn fleet_stream_key(fleet_id: &str) -> String {
    format!("fleet:{fleet_id}:events")
}

/// The key one append-once intent is remembered under.
///
/// One spelling (RULE UFS): `append_once` writes it and `forget_once` deletes
/// it, and a pair that drifted would leave the write remembered forever while
/// the delete removed nothing — a repair intent that could never run again.
fn once_key(once_id: &str) -> String {
    format!("{ONCE_KEY_PREFIX}{once_id}")
}

/// The channel a fleet's live-tail frames are published on.
///
/// `activity_publisher.zig` builds this into a 128-byte stack buffer and has a
/// failure arm for the overflow, which is a fact about `bufPrint` rather than
/// about the channel. Here the string owns its own length and the arm is gone —
/// there is no truncated-channel case left to handle, so nothing has to decide
/// what publishing to a truncated channel would mean.
///
/// Distinct from [`fleet_stream_key`] and deliberately adjacent to it: one is a
/// durable STREAM that survives a restart and one is a pub/sub CHANNEL with no
/// retention at all, and a caller reaching for the wrong one would either lose
/// every frame or persist cosmetic ones forever.
#[must_use]
pub fn fleet_activity_channel(fleet_id: &str) -> String {
    format!("fleet:{fleet_id}:activity")
}

/// A Redis stream entry id, which is also the canonical event id.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventId(String);

impl EventId {
    /// The id as Redis spelled it, `{millis}-{sequence}`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// An id read back out of a column rather than minted by an append.
    ///
    /// The report path needs this: the entry was acknowledged long after the
    /// poll that read it, by a different request, and what survives in between
    /// is the `fleet.runner_leases.event_id` text. Deliberately not a `From`
    /// impl — an id is a thing Redis produced, and a blanket conversion from
    /// `&str` would let any string in the program become one silently.
    #[must_use]
    pub fn of(stored: &str) -> Self {
        Self(stored.to_owned())
    }
}

impl std::fmt::Display for EventId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// One event read off a fleet stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FleetEvent {
    /// The entry id, which is the event id.
    pub id: EventId,
    /// The entry's fields, in the order Redis returned them.
    pub fields: Vec<(String, String)>,
}

impl FleetEvent {
    /// The value of one field, if the entry carries it.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

/// Fleet stream operations against one connection.
#[derive(Debug, Clone)]
pub struct FleetStreams {
    redis: Redis,
}

impl FleetStreams {
    /// Binds stream operations to a connection.
    #[must_use]
    pub const fn new(redis: Redis) -> Self {
        Self { redis }
    }

    /// Creates the consumer group, delivering from the stream's beginning.
    ///
    /// Idempotent: an existing group answers `BUSYGROUP`, which is the expected
    /// steady state and not a failure. `MKSTREAM` so the first call on a fleet
    /// that has never had an event still leaves a group behind.
    ///
    /// # Errors
    /// Returns a command error when the group could not be created for any
    /// reason other than already existing.
    pub async fn ensure_group(&self, fleet_id: &str) -> Result<()> {
        self.create_group(fleet_id, GROUP_START_BEGIN).await
    }

    async fn create_group(&self, fleet_id: &str, start: &str) -> Result<()> {
        let key = fleet_stream_key(fleet_id);
        let mut cmd = redis::cmd(CMD_XGROUP);
        cmd.arg("CREATE")
            .arg(&key)
            .arg(FLEET_CONSUMER_GROUP)
            .arg(start)
            .arg("MKSTREAM");

        match self.redis.command::<String>(CMD_XGROUP, &key, &cmd).await {
            Ok(_) => Ok(()),
            Err(failure) if failure.is_group_exists() => Ok(()),
            Err(failure) => Err(failure),
        }
    }

    /// Appends an event, returning the id Redis minted for it.
    ///
    /// `MAXLEN ~ 10000` caps retention approximately, which is the trim Redis
    /// can do without scanning: an exact trim would make every append pay for
    /// the whole stream.
    ///
    /// # Errors
    /// Returns a command error when the append fails, and an unexpected-reply
    /// error when Redis answers with something that is not an id.
    pub async fn append(&self, fleet_id: &str, fields: &[(&str, &str)]) -> Result<EventId> {
        let key = fleet_stream_key(fleet_id);
        let mut cmd = redis::cmd(CMD_XADD);
        cmd.arg(&key)
            .arg("MAXLEN")
            .arg("~")
            .arg(STREAM_MAXLEN)
            .arg("*");
        for (name, value) in fields {
            cmd.arg(*name).arg(*value);
        }

        let id: String = self.redis.command(CMD_XADD, &key, &cmd).await?;
        if id.is_empty() {
            return Err(error::unexpected_reply(CMD_XADD));
        }
        tracing::debug!(fleet_id, event_id = %id, event = "xadd_fleet_event");
        Ok(EventId(id))
    }

    /// Appends one event AT MOST ONCE, however many times this is called.
    ///
    /// The durable intent behind a repair verification is retried until the
    /// database records which event it produced, and those two writes cannot be
    /// one transaction — one is Redis and one is Postgres. So the retry has to
    /// be safe, and "safe" here means the second attempt returns the FIRST
    /// attempt's event id rather than appending a second event: a duplicate
    /// would run the same verification twice, with real provider spend.
    ///
    /// A `SET NX` beside the append would not do it — the two are separate
    /// round trips and a crash between them leaves either an event nothing
    /// remembers or a key naming no event. The script makes the pair atomic,
    /// which is the property the whole retry loop rests on.
    ///
    /// Answers the event id and whether this call is the one that wrote it.
    ///
    /// # Errors
    /// Returns a command error, or an unavailable error when Redis is gone. A
    /// key holding something that is not a stream is refused by the script
    /// rather than appended to.
    pub async fn append_once(
        &self,
        once_id: &str,
        fleet_id: &str,
        fields: &[(&str, &str)],
    ) -> Result<Appended> {
        let key = fleet_stream_key(fleet_id);
        let mut invocation = APPEND_ONCE.prepare_invoke();
        invocation
            .key(once_key(once_id))
            .key(&key)
            .arg(STREAM_MAXLEN);
        for (name, value) in fields {
            invocation.arg(*name).arg(*value);
        }

        let (event_id, outcome): (String, String) =
            self.redis.script(CMD_EVAL, &key, &invocation).await?;
        if event_id.is_empty() {
            return Err(error::unexpected_reply(CMD_EVAL));
        }
        Ok(Appended {
            id: EventId(event_id),
            replayed: outcome == OUTCOME_REPLAYED,
        })
    }

    /// Forgets an append-once key.
    ///
    /// Called only AFTER the database records which event the intent produced.
    /// Cleared any earlier and a retry in between would append a second event,
    /// which is the exact duplicate the key exists to prevent.
    ///
    /// # Errors
    /// Returns a command error when the delete fails.
    pub async fn forget_once(&self, once_id: &str) -> Result<()> {
        let key = once_key(once_id);
        let mut cmd = redis::cmd(CMD_DEL);
        cmd.arg(&key);
        let _removed: i64 = self.redis.command(CMD_DEL, &key, &cmd).await?;
        Ok(())
    }

    /// Reads the next undelivered event, without blocking.
    ///
    /// Never `BLOCK`: this connection is multiplexed, so parking on one stream
    /// would park every other caller sharing it. The assignment scan probes
    /// several fleets per poll and the runner long-polls client-side instead.
    ///
    /// # Errors
    /// Returns a command error, or an unavailable error when Redis is gone.
    /// A vanished group is repaired here rather than reported.
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
        match self.read_once(fleet_id, consumer, read_id).await {
            Err(failure) if failure.is_group_missing() => {
                // Hoisted: see the `tracing` note in the workspace Cargo.toml.
                let error_code = afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
                tracing::warn!(
                    fleet_id,
                    error_code,
                    event = "fleet_consumer_group_missing_repaired"
                );
                self.create_group(fleet_id, GROUP_START_END).await?;
                self.read_once(fleet_id, consumer, read_id).await
            }
            other => other,
        }
    }

    async fn read_once(
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
                id: EventId(entry.id),
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
            id: EventId(entry.id),
            fields: entry
                .map
                .into_iter()
                .map(|(name, value)| (name, stringify(&value)))
                .collect(),
        }))
    }

    /// Whether this fleet holds work a runner could still pick up.
    ///
    /// The backstop for a readiness mark that was lost — an ingress mark that
    /// failed, an index that was evicted or flushed. The streams are the system
    /// of record and the index is a hint, so this asks the record.
    ///
    /// Two things count as deliverable: entries a group has been handed and not
    /// acknowledged (`pending`), and entries nobody has been handed at all
    /// (`lag`). The second is the half a claim can never find, because an entry
    /// nobody has read is in nobody's pending list.
    ///
    /// # Errors
    /// Returns a command error, or an unavailable error when Redis is gone. A
    /// probe that cannot answer is REPORTED rather than read as "nothing to
    /// recover" — this is the recovery path's own backstop, and a silent false
    /// would leave it inert while looking exactly like an idle system.
    pub async fn has_deliverable(&self, fleet_id: &str) -> Result<bool> {
        let key = fleet_stream_key(fleet_id);
        let mut stream_info = redis::cmd(CMD_XINFO);
        stream_info.arg("STREAM").arg(&key);
        let stream: redis::streams::StreamInfoStreamReply =
            self.redis.command(CMD_XINFO, &key, &stream_info).await?;
        // No entries ever generated, so nothing to deliver whatever the group
        // says about itself.
        if stream.length == 0 {
            return Ok(false);
        }

        let mut group_info = redis::cmd(CMD_XINFO);
        group_info.arg("GROUPS").arg(&key);
        let groups: redis::streams::StreamInfoGroupsReply =
            self.redis.command(CMD_XINFO, &key, &group_info).await?;
        let Some(group) = groups
            .groups
            .into_iter()
            .find(|group| group.name == FLEET_CONSUMER_GROUP)
        else {
            // No consumer group yet: no runner has ever read this fleet, so
            // every entry present is undelivered.
            return Ok(true);
        };
        // A `lag` Redis cannot determine is read as deliverable. The direction
        // matters and only one of them is safe: a false positive costs one
        // wasted candidate check, and a false negative strands an event.
        Ok(group.pending > 0 || group.lag.is_none_or(|lag| lag > 0))
    }

    /// Publishes on a channel, for the subscription hub's readers.
    ///
    /// # Errors
    /// Returns a command error when the publish fails.
    pub async fn publish(&self, channel: &str, payload: &str) -> Result<i64> {
        let mut cmd = redis::cmd(CMD_PUBLISH);
        cmd.arg(channel).arg(payload);
        self.redis.command(CMD_PUBLISH, channel, &cmd).await
    }
}

/// Every reply shape [`stringify`] renders, each with the label it is rendered
/// from.
///
/// Exposed under `test-util` because Redis will not produce these on demand: a
/// stream field is a bulk string on the wire, so the arms that keep a
/// surprising value readable have no other way to be reached. A producer that
/// starts writing something else — or a redis-rs release that decodes an
/// integer field differently — is exactly the surprise these arms exist for,
/// and an unrendered one reaching a caller as an empty string is silent.
#[cfg(feature = "test-util")]
#[must_use]
pub fn rendered_field_samples() -> Vec<(&'static str, String)> {
    vec![
        (
            "bulk string",
            stringify(&redis::Value::BulkString(b"ready".to_vec())),
        ),
        (
            "simple string",
            stringify(&redis::Value::SimpleString("OK".to_owned())),
        ),
        ("integer", stringify(&redis::Value::Int(42))),
        ("anything else", stringify(&redis::Value::Nil)),
    ]
}

/// Renders a stream field value as text.
///
/// Stream fields are byte strings on the wire. Anything else is a value this
/// producer did not write, and rendering it through `Debug` keeps a surprising
/// entry readable instead of failing the whole read.
fn stringify(value: &redis::Value) -> String {
    match value {
        redis::Value::BulkString(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        redis::Value::SimpleString(text) => text.clone(),
        redis::Value::Int(number) => number.to_string(),
        other => format!("{other:?}"),
    }
}
