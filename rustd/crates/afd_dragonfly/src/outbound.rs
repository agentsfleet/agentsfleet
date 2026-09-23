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
//! connector from being a change to the report path.
//!
//! # Two types because there are two connections
//!
//! [`OutboundQueue`] enqueues and acknowledges over the shared [`Dragonfly`]: both
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

use crate::client::Dragonfly;
use crate::error::{self, Result};
use crate::streams::{ACKNOWLEDGED_HISTORY, EventId, Trimmed, retain};

/// The commands this module issues, named once each (RULE UFS).
const CMD_XADD: &str = "XADD";
const CMD_XGROUP: &str = "XGROUP";

/// Asks what a key holds, so a create is never issued over the wrong thing.
const CMD_TYPE: &str = "TYPE";

/// `TYPE`'s answer for a key that does not exist.
const TYPE_NONE: &str = "none";

/// `TYPE`'s answer for a key that is already a stream.
const TYPE_STREAM: &str = "stream";
pub(super) const CMD_XREADGROUP: &str = "XREADGROUP";
pub(super) const CMD_XACK: &str = "XACK";

/// The stream every connector answer is queued on.
///
/// ONE stream for every provider, not one per provider: the ordering guarantee
/// that matters is per destination thread, delivery is serial, and a stream per
/// provider would multiply consumer groups without buying anything. A DATA
/// FORMAT shared with the Zig daemon — both binaries read this key.
pub const OUTBOUND_STREAM_KEY: &str = "connector:outbound";

/// The consumer group the workers read under. Shared with the Zig daemon.
pub const OUTBOUND_CONSUMER_GROUP: &str = "connector_workers";

/// The job's fields on the wire, named once each. A DATA FORMAT: a reader
/// deserialises by these exact names, so renaming one is a wire change.
pub(super) const FIELD_PROVIDER: &str = "provider";
/// See [`FIELD_PROVIDER`].
pub(super) const FIELD_WORKSPACE_ID: &str = "workspace_id";
/// See [`FIELD_PROVIDER`].
pub(super) const FIELD_FLEET_ID: &str = "fleet_id";
/// See [`FIELD_PROVIDER`].
pub(super) const FIELD_EVENT_ID: &str = "event_id";
/// See [`FIELD_PROVIDER`].
pub(super) const FIELD_ANSWER: &str = "answer";
/// See [`FIELD_PROVIDER`]. Added after the Zig daemon retired, so an entry
/// it wrote carries none and is dropped as undecodable — every such entry was
/// owed to a model provider and could not be delivered anyway.
pub(super) const FIELD_DESTINATION: &str = "destination";

/// Read id meaning "entries never delivered to any consumer".
pub(super) const NEW_ENTRIES: &str = ">";

/// Read id meaning "this consumer's own pending entries, oldest first".
pub(super) const OWN_PENDING: &str = "0";

/// Group start id: from the beginning, so a job queued before any worker ever
/// read is still delivered.
///
/// Safe here in a way it is not on a fleet stream: an outbound entry is
/// acknowledged as soon as it is delivered or permanently dropped, so a group
/// created at `0` re-offers only what is genuinely unacknowledged. The fleet
/// streams recreate at `$` because their entries are RUNS, and re-offering a
/// delivered one would re-execute it.
const GROUP_START_BEGIN: &str = "0";

/// The `XGROUP` subcommand that creates a consumer group.
const XGROUP_CREATE: &str = "CREATE";

/// Creates the stream alongside the group when the stream does not exist yet.
const XGROUP_MKSTREAM: &str = "MKSTREAM";

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
/// three. The per-probe version shipped first and was replaced for this
/// reason.
///
/// # The name is the host's, through the syscall rather than the environment
///
/// `HOSTNAME` is a shell variable, not an exported one, so a systemd unit
/// reading it finds nothing and every instance on the deployment collapses
/// onto one consumer name — the exact stranding this function exists to
/// prevent, reintroduced by the cheaper lookup. The `hostname` crate is a safe
/// wrapper over that syscall, so instances name themselves by host and,
/// being different hosts or containers, do not collide.
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
/// Borrowed on the way in: the enqueue reads these and Dragonfly owns them after,
/// so nothing here needs to allocate a copy the caller already holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutboundJob<'a> {
    /// Which connector the answer goes back through, as opaque text.
    pub provider: &'a str,
    /// Where that connector posts it, as the producer recorded it: opaque
    /// here, read only by that connector's poster.
    pub destination: &'a str,
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
    /// See [`OutboundJob::destination`].
    pub destination: String,
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
    redis: Dragonfly,
}

impl OutboundQueue {
    /// Binds the queue to a connection.
    #[must_use]
    pub const fn new(redis: Dragonfly) -> Self {
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
        // The same guard `FleetStreams::create_group` carries, for the same
        // reason: `MKSTREAM` below creates the key, a create reaches
        // `DbSlice::AddNew`, and Dragonfly v1.40.2 aborts the node there rather
        // than answering `WRONGTYPE` when the key holds something else.
        let mut probe = redis::cmd(CMD_TYPE);
        probe.arg(OUTBOUND_STREAM_KEY);
        let holds: String = self
            .redis
            .command(CMD_TYPE, OUTBOUND_STREAM_KEY, &probe)
            .await?;
        if holds != TYPE_NONE && holds != TYPE_STREAM {
            return Err(error::wrong_type(CMD_XGROUP, OUTBOUND_STREAM_KEY, &holds));
        }

        let mut cmd = redis::cmd(CMD_XGROUP);
        cmd.arg(XGROUP_CREATE)
            .arg(OUTBOUND_STREAM_KEY)
            .arg(OUTBOUND_CONSUMER_GROUP)
            .arg(GROUP_START_BEGIN)
            .arg(XGROUP_MKSTREAM);

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

    /// Queues one answer for delivery, returning the id Dragonfly minted.
    ///
    /// No `MAXLEN`, for the reason the fleet streams carry none: an append
    /// cannot know what the worker still owes. [`OutboundQueue::trim`] runs
    /// on the acknowledgement path with the floor that knows.
    ///
    /// # Errors
    /// Returns a command error when the append fails, a full error when the
    /// datastore refuses to grow, and an unexpected-reply error when Dragonfly
    /// answers with something that is not an id.
    pub async fn enqueue(&self, job: OutboundJob<'_>) -> Result<EventId> {
        let mut cmd = redis::cmd(CMD_XADD);
        cmd.arg(OUTBOUND_STREAM_KEY)
            .arg("*")
            .arg(FIELD_PROVIDER)
            .arg(job.provider)
            .arg(FIELD_DESTINATION)
            .arg(job.destination)
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

    /// Trims delivered history to [`ACKNOWLEDGED_HISTORY`], never crossing
    /// the oldest entry a worker still owes.
    ///
    /// The same floor the fleet streams use, over the same reader state:
    /// the group's last delivered id, its oldest pending entry, and the
    /// history window.
    ///
    /// # Errors
    /// As [`FleetStreams::trim`](crate::streams::FleetStreams::trim).
    pub async fn trim(&self) -> Result<Trimmed> {
        retain::trim_history(
            &self.redis,
            OUTBOUND_STREAM_KEY,
            OUTBOUND_CONSUMER_GROUP,
            ACKNOWLEDGED_HISTORY,
        )
        .await
    }
}

mod reader;

pub use self::reader::OutboundReader;

#[cfg(test)]
mod tests {
    use super::*;

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
