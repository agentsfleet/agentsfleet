//! `connector:outbound` — the durable queue a fleet's answer travels back on.
//!
//! The RETURN leg. A question arrives through a connector (a Slack mention, a
//! Jira comment), a fleet runs, and the answer has to reach the place the
//! question came from. That delivery is a vendor HTTP call which can be slow,
//! rate-limited or down, so it does not happen on the report path: the report
//! enqueues here and a worker delivers.
//!
//! # Provider is an opaque string, and that is Invariant 9
//!
//! Nothing in this module knows what a provider IS. The job carries `provider`
//! as text, so the report path enqueues without importing a connector and this
//! crate stays out of the connector graph entirely. Exactly one crate resolves
//! that string to a poster — `afd_outbound` — which is what keeps a new
//! connector from being a change to the report path. `connector_outbound.zig`
//! makes the same split for the same reason.
//!
//! # Two types because there are two connections
//!
//! [`OutboundQueue`] enqueues and acknowledges over the shared [`Redis`]: both
//! are ordinary commands and both are issued from the request path, which is
//! where a shared multiplexed connection belongs.
//!
//! [`OutboundReader`] reads, and reading is where this stream differs from
//! every other one in this crate: it BLOCKS. `streams/consume.rs` never passes
//! `BLOCK` because parking the shared socket would park the whole process; the
//! reader therefore takes a [`Dedicated`] connection, and the split into two
//! types is how that requirement is stated in the type system rather than in a
//! comment somebody has to read first.
//!
//! # Pending-first, and why it is not an optimisation
//!
//! `XREADGROUP >` only ever hands out entries nobody has seen. An entry
//! delivered to this consumer and not acknowledged — a process that stopped
//! mid-post, a cancelled read whose reply was already on the wire — sits in
//! that consumer's pending list, and NOTHING re-offers it. So every loop asks
//! for its own pending entries before asking for new ones, and the consumer
//! name has to be one the next process comes back to. See
//! [`outbound_consumer`].

use redis::ToRedisArgs as _;
use redis::streams::StreamReadOptions;

use crate::client::Redis;
use crate::dedicated::Dedicated;
use crate::error::{self, Result};
use crate::streams::EventId;

/// The commands this module issues, named once each (RULE UFS).
const CMD_XADD: &str = "XADD";
const CMD_XGROUP: &str = "XGROUP";
const CMD_XREADGROUP: &str = "XREADGROUP";
const CMD_XACK: &str = "XACK";

/// The stream every connector answer is queued on.
///
/// ONE stream for every provider, not one per provider: the ordering guarantee
/// that matters is per destination thread, delivery is serial, and a stream per
/// provider would multiply consumer groups without buying anything. A DATA
/// FORMAT shared with the Zig daemon — both binaries read this key.
pub const OUTBOUND_STREAM_KEY: &str = "connector:outbound";

/// The consumer group the workers read under. Shared with the Zig daemon.
pub const OUTBOUND_CONSUMER_GROUP: &str = "connector_workers";

/// Approximate cap on retained entries.
///
/// A wedged consumer can then never grow the stream without bound; `~` is the
/// trim Redis performs without scanning. The Zig spells the same number.
const OUTBOUND_MAXLEN: usize = 100_000;

/// The job's fields on the wire, named once each. A DATA FORMAT: these are the
/// field names `connector_outbound.zig` writes and reads.
const FIELD_PROVIDER: &str = "provider";
/// See [`FIELD_PROVIDER`].
const FIELD_WORKSPACE_ID: &str = "workspace_id";
/// See [`FIELD_PROVIDER`].
const FIELD_FLEET_ID: &str = "fleet_id";
/// See [`FIELD_PROVIDER`].
const FIELD_EVENT_ID: &str = "event_id";
/// See [`FIELD_PROVIDER`].
const FIELD_ANSWER: &str = "answer";

/// Read id meaning "entries never delivered to any consumer".
const NEW_ENTRIES: &str = ">";

/// Read id meaning "this consumer's own pending entries, oldest first".
const OWN_PENDING: &str = "0";

/// Group start id: from the beginning, so a job queued before any worker ever
/// read is still delivered.
///
/// Safe here in a way it is not on a fleet stream: an outbound entry is
/// acknowledged as soon as it is delivered or permanently dropped, so a group
/// created at `0` re-offers only what is genuinely unacknowledged. The fleet
/// streams recreate at `$` because their entries are RUNS, and re-offering a
/// delivered one would re-execute it.
const GROUP_START_BEGIN: &str = "0";

/// The prefix an outbound consumer name is built on. Shared with the Zig.
const CONSUMER_PREFIX: &str = "agentsfleetd";

/// What an instance with no name of its own reads under.
const CONSUMER_FALLBACK_HOST: &str = "localhost";

/// The consumer name this process reads the outbound stream under.
///
/// Host-derived and timestamp-free, so a restarted instance comes back to the
/// SAME pending list and [`OutboundReader::read_pending`] can find what the
/// previous process was handed and never acknowledged.
///
/// # Why not [`afd_fleet::lease::runner_consumer`]'s shape
///
/// That name carries the process id, and correctly: a fleet stream has a
/// reclaim sweeper that claims stranded entries out of dead consumers, so a
/// per-process name costs nothing there. This stream has no sweeper. A name
/// that changed per process would stand every unacknowledged answer in a
/// pending list nothing ever reads again — the entry would be neither
/// delivered nor lost, just permanently invisible, which is the worst of the
/// three. `redis_client.zig` reaches the same conclusion in the comment above
/// `stableConsumerId`, having shipped the per-probe version first.
///
/// # The name is the host's, through the syscall rather than the environment
///
/// `HOSTNAME` is a shell variable, not an exported one, so a systemd unit
/// reading it finds nothing and every instance on the deployment collapses
/// onto one consumer name — the exact stranding this function exists to
/// prevent, reintroduced by the cheaper lookup. The `hostname` crate is a safe
/// wrapper over the one call `redis_client.zig` makes, so the two daemons name
/// themselves identically and, being different hosts or containers, do not
/// collide.
#[must_use]
pub fn outbound_consumer() -> String {
    let host = hostname::get()
        .ok()
        .and_then(|name| name.into_string().ok())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| {
            // Loud, because recovery attribution blurs: every instance that
            // cannot name itself shares one pending list. Correctness survives
            // — a redelivered answer lands in the destination's own thread —
            // but an operator reading two instances' work under one consumer
            // deserves to know why. The Zig logs the same fallback.
            tracing::warn!(
                fallback = CONSUMER_FALLBACK_HOST,
                event = "consumer_id_hostname_fallback"
            );
            CONSUMER_FALLBACK_HOST.to_owned()
        });
    format!("{CONSUMER_PREFIX}-{host}")
}

/// One answer waiting to be delivered.
///
/// Borrowed on the way in: the enqueue reads these and Redis owns them after,
/// so nothing here needs to allocate a copy the caller already holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutboundJob<'a> {
    /// Which connector the answer goes back through, as opaque text.
    pub provider: &'a str,
    /// The workspace whose grant pays for the delivery.
    pub workspace_id: &'a str,
    /// The fleet that produced the answer.
    pub fleet_id: &'a str,
    /// The event the question arrived on, which is where the answer is threaded.
    pub event_id: &'a str,
    /// What to say.
    pub answer: &'a str,
}

/// A job read back off the stream, with the id that acknowledges it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutboundDelivery {
    /// The entry id, which is what [`OutboundQueue::ack`] takes.
    pub id: EventId,
    /// See [`OutboundJob::provider`].
    pub provider: String,
    /// See [`OutboundJob::workspace_id`].
    pub workspace_id: String,
    /// See [`OutboundJob::fleet_id`].
    pub fleet_id: String,
    /// See [`OutboundJob::event_id`].
    pub event_id: String,
    /// See [`OutboundJob::answer`].
    pub answer: String,
}

/// The write half: enqueue and acknowledge, over the shared connection.
#[derive(Debug, Clone)]
pub struct OutboundQueue {
    redis: Redis,
}

impl OutboundQueue {
    /// Binds the queue to a connection.
    #[must_use]
    pub const fn new(redis: Redis) -> Self {
        Self { redis }
    }

    /// Creates the consumer group, delivering from the stream's beginning.
    ///
    /// Idempotent: an existing group answers `BUSYGROUP`, which is the steady
    /// state. `MKSTREAM` so the first call on a deployment that has never
    /// delivered an answer still leaves a group behind for the worker to read.
    ///
    /// # Errors
    /// Returns a command error when the group could not be created for any
    /// reason other than already existing.
    pub async fn ensure_group(&self) -> Result<()> {
        let mut cmd = redis::cmd(CMD_XGROUP);
        cmd.arg("CREATE")
            .arg(OUTBOUND_STREAM_KEY)
            .arg(OUTBOUND_CONSUMER_GROUP)
            .arg(GROUP_START_BEGIN)
            .arg("MKSTREAM");

        match self
            .redis
            .command::<String>(CMD_XGROUP, OUTBOUND_STREAM_KEY, &cmd)
            .await
        {
            Ok(_) => Ok(()),
            Err(failure) if failure.is_group_exists() => Ok(()),
            Err(failure) => Err(failure),
        }
    }

    /// Queues one answer for delivery, returning the id Redis minted.
    ///
    /// # Errors
    /// Returns a command error when the append fails, and an unexpected-reply
    /// error when Redis answers with something that is not an id.
    pub async fn enqueue(&self, job: OutboundJob<'_>) -> Result<EventId> {
        let mut cmd = redis::cmd(CMD_XADD);
        cmd.arg(OUTBOUND_STREAM_KEY)
            .arg("MAXLEN")
            .arg("~")
            .arg(OUTBOUND_MAXLEN)
            .arg("*")
            .arg(FIELD_PROVIDER)
            .arg(job.provider)
            .arg(FIELD_WORKSPACE_ID)
            .arg(job.workspace_id)
            .arg(FIELD_FLEET_ID)
            .arg(job.fleet_id)
            .arg(FIELD_EVENT_ID)
            .arg(job.event_id)
            .arg(FIELD_ANSWER)
            .arg(job.answer);

        let id: String = self
            .redis
            .command(CMD_XADD, OUTBOUND_STREAM_KEY, &cmd)
            .await?;
        if id.is_empty() {
            return Err(error::unexpected_reply(CMD_XADD));
        }
        // Hoisted: see the `tracing` note in the workspace Cargo.toml.
        let provider = job.provider;
        let fleet_id = job.fleet_id;
        tracing::debug!(
            provider,
            fleet_id,
            entry_id = %id,
            event = "outbound_enqueued"
        );
        Ok(EventId::of(&id))
    }

    /// Acknowledges a delivery, removing it from the consumer's pending list.
    ///
    /// Over the SHARED connection rather than the reader's, deliberately: the
    /// reader may be parked in a `BLOCK` at the moment an acknowledgement is
    /// ready, and an ack queued behind it would wait out the whole interval.
    ///
    /// # Errors
    /// Returns a command error when the acknowledgement fails.
    pub async fn ack(&self, id: &EventId) -> Result<bool> {
        let mut cmd = redis::cmd(CMD_XACK);
        cmd.arg(OUTBOUND_STREAM_KEY)
            .arg(OUTBOUND_CONSUMER_GROUP)
            .arg(id.as_str());
        let acknowledged: i64 = self
            .redis
            .command(CMD_XACK, OUTBOUND_STREAM_KEY, &cmd)
            .await?;
        Ok(acknowledged > 0)
    }
}

/// The read half: one worker's own connection, which it is allowed to park on.
#[derive(Debug)]
pub struct OutboundReader {
    connection: Dedicated,
    consumer: String,
}

impl OutboundReader {
    /// Binds a reader to a connection nothing else holds.
    ///
    /// Takes the [`Dedicated`] by value, which is the invariant: a connection
    /// this reader will block on cannot also be somebody else's.
    #[must_use]
    pub fn new(connection: Dedicated, consumer: String) -> Self {
        Self {
            connection,
            consumer,
        }
    }

    /// The name this reader claims entries under.
    #[must_use]
    pub fn consumer(&self) -> &str {
        &self.consumer
    }

    /// This consumer's oldest unacknowledged entry, without blocking.
    ///
    /// What a restart has to ask first — see the module note on pending-first.
    /// `None` means the pending list is empty, which is the ordinary answer.
    ///
    /// # Errors
    /// Returns a command error, or an unavailable error when Redis is gone.
    pub async fn read_pending(&mut self) -> Result<Option<OutboundDelivery>> {
        self.read(OWN_PENDING, None).await
    }

    /// The next undelivered entry, parking up to `block_ms` for one to arrive.
    ///
    /// The park is the point: the Zig polls every 250 ms because its pooled
    /// connections could not hold a `BLOCK`, and pays that latency on every
    /// answer plus a command per interval forever. Here the server holds the
    /// read open and answers the instant an entry lands.
    ///
    /// `block_ms` bounds it anyway, because a read that never returns is a
    /// task that cannot be joined: the caller races this against its
    /// cancellation token, and dropping the future does NOT cancel the command
    /// server-side — Redis may still assign an entry to this consumer after
    /// the drop. That entry is not lost, it is pending, and the next process's
    /// [`Self::read_pending`] is what finds it. Dimension 5.2.
    ///
    /// # Errors
    /// As [`Self::read_pending`].
    pub async fn read_blocking(&mut self, block_ms: usize) -> Result<Option<OutboundDelivery>> {
        self.read(NEW_ENTRIES, Some(block_ms)).await
    }

    /// One `XREADGROUP`, built the way [`crate::streams::FleetStreams`] builds
    /// its own.
    ///
    /// Through [`StreamReadOptions`] rather than by spelling `GROUP … COUNT …
    /// BLOCK …` in order: which clause `XREADGROUP` wants where is the redis
    /// crate's to know, and hand-writing it here would be a second copy of that
    /// knowledge thirty lines from the first, each free to drift.
    async fn read(
        &mut self,
        read_id: &str,
        block_ms: Option<usize>,
    ) -> Result<Option<OutboundDelivery>> {
        let mut options = StreamReadOptions::default()
            .group(OUTBOUND_CONSUMER_GROUP, &self.consumer)
            .count(1);
        if let Some(millis) = block_ms {
            options = options.block(millis);
        }

        let mut cmd = redis::cmd(CMD_XREADGROUP);
        for arg in options.to_redis_args() {
            cmd.arg(arg);
        }
        cmd.arg("STREAMS").arg(OUTBOUND_STREAM_KEY).arg(read_id);

        let reply: redis::streams::StreamReadReply = self
            .connection
            .command(CMD_XREADGROUP, OUTBOUND_STREAM_KEY, &cmd)
            .await?;
        Ok(reply
            .keys
            .into_iter()
            .flat_map(|stream| stream.ids)
            .next()
            .as_ref()
            .and_then(decode))
    }
}

/// A stream entry as a delivery, or nothing when a field is missing.
///
/// Every field is written by [`OutboundQueue::enqueue`], so an entry short of
/// one was not written by this daemon — operator tooling, a foreign writer, a
/// format that drifted. `None` rather than an error, because the caller's only
/// sane response is the same either way: acknowledge it and move on, since
/// redelivering something undeliverable forever is the one outcome worse than
/// dropping it. The Zig raises `RedisUnexpectedResponse` here and its worker
/// then swallows it, which is the same decision spelled twice.
fn decode(entry: &redis::streams::StreamId) -> Option<OutboundDelivery> {
    let field = |name: &str| entry.get::<String>(name);
    let delivery = OutboundDelivery {
        id: EventId::of(&entry.id),
        provider: field(FIELD_PROVIDER)?,
        workspace_id: field(FIELD_WORKSPACE_ID)?,
        fleet_id: field(FIELD_FLEET_ID)?,
        event_id: field(FIELD_EVENT_ID)?,
        answer: field(FIELD_ANSWER)?,
    };
    Some(delivery)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a stream entry the way Redis hands one back.
    fn entry(fields: &[(&str, &str)]) -> redis::streams::StreamId {
        redis::streams::StreamId {
            id: "1700000000001-0".to_owned(),
            map: fields
                .iter()
                .map(|(name, value)| {
                    (
                        (*name).to_owned(),
                        redis::Value::BulkString((*value).as_bytes().to_vec()),
                    )
                })
                .collect(),
            // Present on a pending read and absent on a fresh one; the decoder
            // reads neither, so a plain read's shape is what is built here.
            delivered_count: None,
            milliseconds_elapsed_from_delivery: None,
        }
    }

    /// Every field an enqueue writes, which is what a complete job looks like.
    fn complete() -> Vec<(&'static str, &'static str)> {
        vec![
            (FIELD_PROVIDER, "slack"),
            (FIELD_WORKSPACE_ID, "0199a0b0-0000-7000-8000-000000000001"),
            (FIELD_FLEET_ID, "0199a0b0-0000-7000-8000-000000000002"),
            (FIELD_EVENT_ID, "1700000000000-0"),
            (FIELD_ANSWER, "Aurora is healthy."),
        ]
    }

    /// Asserted as one whole-value equality rather than field by field: a
    /// decoder that dropped a field would still pass every assertion about the
    /// fields it kept, and the failure this guards is a field going missing.
    #[test]
    fn test_decode_round_trips_every_field_and_the_entry_id() {
        let decoded = decode(&entry(&complete()));

        assert_eq!(
            decoded,
            Some(OutboundDelivery {
                id: EventId::of("1700000000001-0"),
                provider: "slack".to_owned(),
                workspace_id: "0199a0b0-0000-7000-8000-000000000001".to_owned(),
                fleet_id: "0199a0b0-0000-7000-8000-000000000002".to_owned(),
                event_id: "1700000000000-0".to_owned(),
                answer: "Aurora is healthy.".to_owned(),
            })
        );
    }

    /// One case per field, so a decoder that stopped checking one is caught by
    /// the case naming it rather than by a single entry missing everything.
    #[test]
    fn test_decode_refuses_an_entry_missing_any_required_field() {
        for (index, (name, _)) in complete().iter().enumerate() {
            let mut fields = complete();
            fields.remove(index);

            assert_eq!(
                decode(&entry(&fields)),
                None,
                "an entry with no `{name}` is not a job this daemon wrote"
            );
        }
    }

    /// The answer is model output, so it carries whatever a run produced.
    #[test]
    fn test_decode_keeps_an_answer_that_is_not_ascii() {
        let answer = "はい — 稼働中 ✅\nnewline and \"quotes\"";
        let mut fields = complete();
        fields.retain(|(name, _)| *name != FIELD_ANSWER);
        fields.push((FIELD_ANSWER, answer));

        assert_eq!(
            decode(&entry(&fields)).map(|delivered| delivered.answer),
            Some(answer.to_owned())
        );
    }

    /// The consumer name is what a restart comes back to, so it must carry
    /// nothing that differs between two runs of the same instance.
    ///
    /// Asserted as an EQUALITY against the two inputs the name is built from,
    /// not as "it does not contain a process id": the failure being guarded
    /// against is a name gaining a per-run component, and only a full-string
    /// comparison catches every shape of that.
    #[test]
    fn test_the_consumer_name_is_the_prefix_and_the_host_and_nothing_else() {
        let host = hostname::get()
            .ok()
            .and_then(|name| name.into_string().ok())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| CONSUMER_FALLBACK_HOST.to_owned());

        assert_eq!(
            outbound_consumer(),
            format!("{CONSUMER_PREFIX}-{host}"),
            "a name with any per-run component would strand every pending \
             entry the previous process was handed"
        );
        assert_eq!(
            outbound_consumer(),
            outbound_consumer(),
            "two calls in one process must agree, which a clock or a counter \
             in the name would break first"
        );
    }
}
