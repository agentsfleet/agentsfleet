//! A connection to the lane's Redis.
//!
//! No key namespacing, unlike the copy in `afd_redis`: every suite here works
//! through a service that MINTS its own identifiers, so two tests running in
//! parallel cannot collide on a key without one of them having minted the
//! other's version 7 identifier. Nothing is flushed between tests either — the
//! lane's Redis is one server shared by every test binary, and a flush would
//! delete another suite's session mid-handshake.

use std::time::Duration;

use afd_redis::Redis;
use afd_redis::config::{RedisConfig, RedisRole};

const URL_KNOB: &str = "TEST_REDIS_URL";
const CA_KNOB: &str = "TEST_REDIS_CA_CERT";

/// The lane's Redis.
pub(crate) struct RedisHarness {
    pub(crate) redis: Redis,
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
        Self { redis }
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
