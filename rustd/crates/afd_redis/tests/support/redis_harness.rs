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

use crate::subscriber::install_subscriber;

const URL_KNOB: &str = "TEST_REDIS_URL";
const CA_KNOB: &str = "TEST_REDIS_CA_CERT";

/// The same server over TLS, for the cases whose subject is the certificate.
const TLS_URL_KNOB: &str = "TEST_REDIS_TLS_URL";

/// Distinguishes keys minted by one process.
static SEQUENCE: AtomicU32 = AtomicU32::new(0);

/// The lane's Redis, plus a name nothing else in the suite uses.
pub(crate) struct RedisHarness {
    pub(crate) redis: Redis,
    prefix: String,
}

impl RedisHarness {
    /// Connects through the crate's own admission gate, not around it.
    ///
    /// `connect_live` is what every other crate's lane harness calls, and this
    /// crate owning it is not a reason to skip it. The gate serializes the
    /// handshake and retries a lapsed budget, which matters here for the same
    /// reason it matters there: the whole cost of a lane connection is the
    /// rustls handshake against an RSA-2048 certificate, redone per connection
    /// with no session resumption. That is CPU work competing with the suite
    /// that asked for it, so under load the budget lapses on a Redis that is
    /// perfectly healthy. `Redis::connect` stays the right call for the
    /// fault-injection suites next door, which point at private endpoints and
    /// want the raw failure.
    pub(crate) async fn connect() -> Self {
        install_subscriber();
        let config = Self::config();
        let redis = afd_redis::test_util::connect_live(&config)
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

    /// The lane's TLS endpoint, for assertions whose SUBJECT is the certificate.
    ///
    /// The ordinary [`Self::config`] is plaintext, because a handshake
    /// re-proving an unchanging authority on every one of the lane's connects
    /// costs a 232 ms median against 0.1 ms. A trust anchor is meaningless on a
    /// plaintext URL and is correctly ignored there — so a test asserting that
    /// a MISSING or MALFORMED authority is refused has to ask over `rediss://`,
    /// or it asserts nothing and passes for the wrong reason.
    pub(crate) fn tls_config() -> RedisConfig {
        let url = std::env::var(TLS_URL_KNOB).unwrap_or_else(|_| {
            panic!("{TLS_URL_KNOB} is unset — run these through `make test-integration-rustd`")
        });
        RedisConfig::from_url(RedisRole::Default, url).with_request_timeout(Duration::from_secs(5))
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
