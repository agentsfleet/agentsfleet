//! The lane's Redis, and a clean `connector:outbound` to run one test against.
//!
//! # Why this harness resets a key instead of namespacing one
//!
//! Every other integration suite in this workspace mints a per-test key prefix
//! (`afd_redis/tests/support/redis_harness.rs`) so parallel targets never
//! collide. That is not available here: `OUTBOUND_STREAM_KEY` and
//! `OUTBOUND_CONSUMER_GROUP` are constants shared with the Zig daemon — both
//! binaries read the same stream by name — and a test that pointed the worker
//! at `afdt123_outbound` would be proving something about a key production
//! never uses.
//!
//! So the stream is reset instead, and the tests that use it hold
//! [`OUTBOUND_LANE`] one at a time. Serialising is the honest cost of grading
//! the real key; the alternative is a green suite that never touched it.
//!
//! # The consumer name is the real one, deliberately
//!
//! [`afd_redis::outbound_consumer`] is host-derived and constant for the life
//! of a process, which is exactly the property Dimension 5.2 depends on: a
//! restarted worker has to come back to the same pending list. A test that
//! invented its own name would prove the pending-first read works for a name
//! nothing in production uses.

use std::time::Duration;

use afd_redis::config::{RedisConfig, RedisRole};
use afd_redis::{
    Dedicated, OUTBOUND_CONSUMER_GROUP, OUTBOUND_STREAM_KEY, OutboundQueue, OutboundReader, Redis,
    outbound_consumer,
};

/// The knob `make test-integration-rustd` exports. See `make/test-infra.mk`.
const URL_KNOB: &str = "TEST_REDIS_URL";
/// See [`URL_KNOB`].
const CA_KNOB: &str = "TEST_REDIS_CA_CERT";

/// Commands this harness issues directly, named once each (RULE UFS).
const CMD_DEL: &str = "DEL";
/// See [`CMD_DEL`].
const CMD_XPENDING: &str = "XPENDING";

/// Long enough that a slow container is not a failure, short enough that a
/// wedged worker fails this test rather than the lane's whole timeout.
const REQUEST_DEADLINE: Duration = Duration::from_secs(5);

/// Serialises every test that touches the shared outbound stream.
///
/// A `tokio` mutex rather than a `std` one because the guard is held across
/// awaits for the whole body of a test.
pub(crate) static OUTBOUND_LANE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The lane's Redis, with the outbound stream emptied and its group recreated.
pub(crate) struct OutboundHarness {
    redis: Redis,
    pub(crate) queue: OutboundQueue,
}

impl OutboundHarness {
    /// Connects and leaves `connector:outbound` holding nothing.
    ///
    /// `DEL` removes the stream AND its consumer groups, so the `ensure_group`
    /// that follows rebuilds both — which is what makes each test's pending
    /// list its own rather than the previous test's leftovers.
    pub(crate) async fn reset() -> Self {
        let redis = Self::connect().await;

        Self::drop_stream(&redis).await;
        let queue = OutboundQueue::new(redis.clone());
        queue
            .ensure_group()
            .await
            .expect("the consumer group must be creatable on an empty stream");
        Self { redis, queue }
    }

    /// Connects and leaves the stream absent, with no group on it.
    ///
    /// What a deployment that has never delivered an answer looks like, and
    /// what `Worker::run`'s own `ensure_group` has to heal.
    pub(crate) async fn reset_without_group() -> Self {
        let redis = Self::connect().await;

        Self::drop_stream(&redis).await;
        let queue = OutboundQueue::new(redis.clone());
        Self { redis, queue }
    }

    /// Writes an entry no reader can decode, as a foreign writer would.
    ///
    /// Deliberately NOT through `enqueue`, which cannot produce this: every
    /// field it writes is one `decode` requires. The states worth covering are
    /// the ones `enqueue` is not the author of — operator tooling, another
    /// daemon, a format that drifted — and the queue is a shared key the Zig
    /// writes to as well, so a foreign writer is a real deployment, not a
    /// contrivance.
    pub(crate) async fn poison(&self) -> String {
        let mut cmd = redis::cmd("XADD");
        cmd.arg(OUTBOUND_STREAM_KEY)
            .arg("*")
            .arg("provider")
            .arg("slack");
        let id: String = self
            .redis
            .command("XADD", OUTBOUND_STREAM_KEY, &cmd)
            .await
            .expect("the fixture entry is writable");
        id
    }

    /// Opens the shared handle, installing the subscriber on the way.
    async fn connect() -> Redis {
        install_subscriber();
        Redis::connect(&Self::config())
            .await
            .expect("the lane's Redis must be reachable")
    }

    /// Removes the stream and every group on it.
    async fn drop_stream(redis: &Redis) {
        let mut cmd = redis::cmd(CMD_DEL);
        cmd.arg(OUTBOUND_STREAM_KEY);
        let _removed: i64 = redis
            .command(CMD_DEL, OUTBOUND_STREAM_KEY, &cmd)
            .await
            .expect("the outbound stream must be removable");
    }

    /// The configuration the lane hands this suite.
    pub(crate) fn config() -> RedisConfig {
        let url = std::env::var(URL_KNOB).unwrap_or_else(|_| {
            panic!("{URL_KNOB} is unset — run these through `make test-integration-rustd`")
        });
        RedisConfig::from_url(RedisRole::Default, url)
            .with_ca_cert_file(std::env::var(CA_KNOB).ok().map(Into::into))
            .with_request_timeout(REQUEST_DEADLINE)
    }

    /// A reader on its own socket, under the consumer name production uses.
    pub(crate) async fn reader(&self) -> OutboundReader {
        let connection = Dedicated::connect(&Self::config())
            .await
            .expect("a dedicated connection must be openable");
        OutboundReader::new(connection, outbound_consumer())
    }

    /// Everything the group has handed out and not had acknowledged.
    async fn pending(&self) -> redis::streams::StreamPendingReply {
        let mut cmd = redis::cmd(CMD_XPENDING);
        cmd.arg(OUTBOUND_STREAM_KEY).arg(OUTBOUND_CONSUMER_GROUP);
        self.redis
            .command(CMD_XPENDING, OUTBOUND_STREAM_KEY, &cmd)
            .await
            .expect("XPENDING must answer on a group this harness created")
    }

    /// How many entries the group is still waiting to have acknowledged.
    pub(crate) async fn pending_count(&self) -> usize {
        self.pending().await.count()
    }

    /// Which consumers hold those entries.
    ///
    /// Dimension 5.2's claim is about WHICH consumer holds a re-queued entry,
    /// not only that one is held: a count alone would pass if the entry were
    /// stranded under a name nothing ever comes back to, which is precisely the
    /// failure the stable consumer name exists to prevent.
    ///
    /// The wildcard arm is `StreamPendingReply` being `#[non_exhaustive]`, not
    /// a case being ignored — a variant this redis version does not have yet
    /// reads as "no consumer holds anything", which fails the caller's
    /// assertion loudly rather than passing it quietly.
    pub(crate) async fn pending_consumers(&self) -> Vec<String> {
        match self.pending().await {
            redis::streams::StreamPendingReply::Data(data) => data
                .consumers
                .into_iter()
                .map(|consumer| consumer.name)
                .collect(),
            redis::streams::StreamPendingReply::Empty | _ => Vec::new(),
        }
    }
}

/// Installs a subscriber so event macros evaluate their fields.
///
/// `tracing::warn!` asks whether its callsite is enabled BEFORE evaluating the
/// fields inside it, so with no subscriber every field expression in every
/// diagnostic is skipped — the events are not merely unrecorded, their
/// arguments never run. The worker's failure paths are mostly diagnostics, so
/// without this a test proves the branch is reached and never proves the line
/// reporting it works. Output goes to a sink; the point is evaluation, not
/// readership. `afd_redis/tests/support/redis_harness.rs` learned this first.
pub(crate) fn install_subscriber() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let subscriber = tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_writer(std::io::sink)
            .finish();
        let _ = tracing::subscriber::set_global_default(subscriber);
    });
}
