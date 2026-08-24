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
use crate::error::{self, Error};

/// The commands this module issues, named once each (RULE UFS).
const CMD_XADD: &str = "XADD";
const CMD_XGROUP: &str = "XGROUP";
const CMD_XREADGROUP: &str = "XREADGROUP";
const CMD_XACK: &str = "XACK";
const CMD_PUBLISH: &str = "PUBLISH";

/// Consumer group every fleet stream is read under.
pub const FLEET_CONSUMER_GROUP: &str = "fleet_lease";

/// Approximate cap on a fleet stream's retained entries (`MAXLEN ~ 10000`).
const STREAM_MAXLEN: usize = 10_000;

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

/// A Redis stream entry id, which is also the canonical event id.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventId(String);

impl EventId {
    /// The id as Redis spelled it, `{millis}-{sequence}`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
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
    pub async fn ensure_group(&self, fleet_id: &str) -> Result<(), Error> {
        self.create_group(fleet_id, GROUP_START_BEGIN).await
    }

    async fn create_group(&self, fleet_id: &str, start: &str) -> Result<(), Error> {
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
    pub async fn append(&self, fleet_id: &str, fields: &[(&str, &str)]) -> Result<EventId, Error> {
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
        tracing::debug!(fleet_id, event_id = %id, "xadd_fleet_event");
        Ok(EventId(id))
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
    pub async fn read_new(
        &self,
        fleet_id: &str,
        consumer: &str,
    ) -> Result<Option<FleetEvent>, Error> {
        self.read(fleet_id, consumer, NEW_ENTRIES).await
    }

    /// Reads this consumer's oldest pending entry — one delivered but never
    /// acknowledged, which is what a re-poll after a crash has to find first.
    ///
    /// # Errors
    /// As [`FleetStreams::read_new`].
    pub async fn read_pending(
        &self,
        fleet_id: &str,
        consumer: &str,
    ) -> Result<Option<FleetEvent>, Error> {
        self.read(fleet_id, consumer, OWN_PENDING).await
    }

    async fn read(
        &self,
        fleet_id: &str,
        consumer: &str,
        read_id: &str,
    ) -> Result<Option<FleetEvent>, Error> {
        match self.read_once(fleet_id, consumer, read_id).await {
            Err(failure) if failure.is_group_missing() => {
                // Hoisted: see the `tracing` note in the workspace Cargo.toml.
                let error_code = afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str();
                tracing::warn!(
                    fleet_id,
                    error_code,
                    "fleet_consumer_group_missing_repaired"
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
    ) -> Result<Option<FleetEvent>, Error> {
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
    pub async fn ack(&self, fleet_id: &str, id: &EventId) -> Result<bool, Error> {
        let key = fleet_stream_key(fleet_id);
        let mut cmd = redis::cmd(CMD_XACK);
        cmd.arg(&key).arg(FLEET_CONSUMER_GROUP).arg(id.as_str());
        let acknowledged: i64 = self.redis.command(CMD_XACK, &key, &cmd).await?;
        Ok(acknowledged > 0)
    }

    /// Publishes on a channel, for the subscription hub's readers.
    ///
    /// # Errors
    /// Returns a command error when the publish fails.
    pub async fn publish(&self, channel: &str, payload: &str) -> Result<i64, Error> {
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
