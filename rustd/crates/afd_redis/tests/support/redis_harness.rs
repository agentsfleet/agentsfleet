//! A connection to the lane's Redis, and keys nothing else will touch.
//!
//! Shared by every integration target here. Keys are namespaced per test rather
//! than the database being flushed between them: the lane's Redis is one
//! server, cargo runs these targets in parallel, and a flush would delete
//! another test's stream mid-read.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use afd_redis::Redis;
use afd_redis::config::{RedisConfig, RedisRole};

const URL_KNOB: &str = "AFD_TEST_REDIS_TLS_URL";
const CA_KNOB: &str = "AFD_TEST_REDIS_TLS_CA_CERT";

/// Distinguishes keys minted by one process.
static SEQUENCE: AtomicU32 = AtomicU32::new(0);

/// The lane's Redis, plus a name nothing else in the suite uses.
pub(crate) struct RedisHarness {
    pub(crate) redis: Redis,
    prefix: String,
}

impl RedisHarness {
    /// Connects, with a deadline short enough that a hung server fails a test
    /// rather than the whole lane's timeout.
    pub(crate) async fn connect() -> Self {
        install_subscriber();
        let config = Self::config();
        let redis = Redis::connect(&config)
            .await
            .expect("the lane's Redis must be reachable");
        Self {
            redis,
            prefix: format!(
                "afdt{}_{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ),
        }
    }

    /// The configuration the lane hands this suite.
    pub(crate) fn config() -> RedisConfig {
        let url = std::env::var(URL_KNOB).unwrap_or_else(|_| {
            panic!("{URL_KNOB} is unset — run these through `make test-integration-rustd`")
        });
        RedisConfig::from_url(RedisRole::Default, url)
            .with_ca_cert_file(std::env::var(CA_KNOB).ok().map(Into::into))
            .with_request_timeout(Duration::from_secs(5))
    }

    /// A name unique to this harness, so parallel tests never collide.
    pub(crate) fn name(&self, suffix: &str) -> String {
        format!("{}_{suffix}", self.prefix)
    }
}

/// Installs a subscriber so event macros actually run.
///
/// `tracing::warn!` asks whether its callsite is enabled BEFORE it evaluates
/// the fields inside it. With no subscriber, every field expression in every
/// diagnostic in this workspace is skipped — the events are not merely
/// unrecorded, their arguments never execute. A test that exercises a failure
/// path therefore proves the path runs but never proves the line that reports
/// it does, and the first sign of a panicking `Display` in a log field would be
/// production.
///
/// Output goes to a sink: the point is that the fields are evaluated, not that
/// anybody reads them.
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
