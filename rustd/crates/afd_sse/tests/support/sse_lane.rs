//! The lane's Redis, and channel names nothing else in the suite will touch.
//!
//! Keys are namespaced per harness rather than the database being flushed
//! between tests: the lane's Redis is one server, cargo runs these targets in
//! parallel, and a flush would delete another suite's stream mid-read. Same
//! contract `afd_redis/tests/support/redis_harness.rs` states; this is the
//! copy that lives where `afd_sse`'s own suites can reach it, because a
//! `#[path]` reaching into a sibling crate's test tree would make one crate's
//! test layout another crate's build dependency.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use afd_redis::Redis;
use afd_redis::config::{RedisConfig, RedisRole};

/// The knob `make test-integration-rustd` exports the lane's Redis under.
const URL_KNOB: &str = "TEST_REDIS_URL";

/// The knob carrying the lane's CA bundle, where the lane speaks TLS.
const CA_KNOB: &str = "TEST_REDIS_CA_CERT";

/// How long a frame may take to travel publisher → Redis → hub → tail.
///
/// Generous on purpose: this is the budget that decides a FAILURE, and a
/// tighter one would turn a loaded CI runner into a red suite. Every wait in
/// these tests polls, so a healthy run never spends it.
pub(crate) const DELIVERY_BUDGET: Duration = Duration::from_secs(5);

/// Distinguishes names minted by one process.
static SEQUENCE: AtomicU32 = AtomicU32::new(0);

/// A connection to the lane's Redis, plus a name prefix of this harness's own.
pub(crate) struct SseLane {
    pub(crate) redis: Redis,
    prefix: String,
}

impl SseLane {
    /// Connects, with a deadline short enough that a hung server fails one test
    /// rather than the whole lane's timeout.
    pub(crate) async fn connect() -> Self {
        install_subscriber();
        let redis = Redis::connect(&Self::config())
            .await
            .expect("the lane's Redis must be reachable");
        Self {
            redis,
            prefix: format!(
                "afdsse{}_{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ),
        }
    }

    /// The configuration the lane hands this suite.
    pub(crate) fn config() -> RedisConfig {
        let url = std::env::var(URL_KNOB).unwrap_or_else(|_unset| {
            panic!("{URL_KNOB} is unset — run these through `make test-integration-rustd`")
        });
        RedisConfig::from_url(RedisRole::Default, url)
            .with_ca_cert_file(std::env::var(CA_KNOB).ok().map(Into::into))
            .with_request_timeout(Duration::from_secs(5))
    }

    /// A fleet identifier unique to this harness.
    ///
    /// Not a UUID: nothing in the streaming path parses one, and a channel name
    /// is the only thing this suite builds from it. A name that collides with
    /// another test binary's is the failure mode that matters, and the process
    /// id plus a counter is what rules it out.
    pub(crate) fn fleet(&self, suffix: &str) -> String {
        format!("{}_{suffix}", self.prefix)
    }
}

/// Installs a subscriber so the event macros in the code under test actually
/// run.
///
/// `tracing::debug!` asks whether its callsite is enabled BEFORE evaluating the
/// fields inside it. With no subscriber, every field expression in every
/// diagnostic is skipped — so a test that closes a tail proves the close path
/// runs but never proves the line reporting it does. Output goes to a sink: the
/// point is that the fields are evaluated, not that anybody reads them.
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
