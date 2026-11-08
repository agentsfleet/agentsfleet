//! A connection to the lane's Dragonfly, and keys nothing else will touch.
//!
//! Shared by every integration target here. Keys are namespaced per test rather
//! than the database being flushed between them: the lane's Dragonfly is one
//! server, cargo runs these targets in parallel, and a flush would delete
//! another test's stream mid-read.

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use af_core::env::{EnvSource, ProcessEnv};
use af_core::id::{ENTROPY_LEN, Uuid7};
use afd_dragonfly::Dragonfly;
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};

use af_core::test_util::trace::install_subscriber;

const URL_KNOB: &str = "TEST_DRAGONFLY_URL";
const CA_KNOB: &str = "TEST_DRAGONFLY_CA_CERT";

/// The same server over TLS, for the cases whose subject is the certificate.
const TLS_URL_KNOB: &str = "TEST_DRAGONFLY_TLS_URL";

/// Distinguishes keys minted by one process.
static SEQUENCE: AtomicU32 = AtomicU32::new(0);

/// Distinguishes identifiers minted by one process.
static ID_SEQUENCE: AtomicU32 = AtomicU32::new(0);

/// The lane's Dragonfly, plus a name nothing else in the suite uses.
pub(crate) struct DragonflyHarness {
    pub(crate) redis: Dragonfly,
    prefix: String,
}

impl DragonflyHarness {
    /// Connects through the crate's own admission gate, not around it.
    ///
    /// `connect_live` is what every other crate's lane harness calls, and this
    /// crate owning it is not a reason to skip it. The gate serializes the
    /// handshake and retries a lapsed budget, which matters here for the same
    /// reason it matters there: the whole cost of a lane connection is the
    /// rustls handshake against an RSA-2048 certificate, redone per connection
    /// with no session resumption. That is CPU work competing with the suite
    /// that asked for it, so under load the budget lapses on a Dragonfly that is
    /// perfectly healthy. `Dragonfly::connect` stays the right call for the
    /// fault-injection suites next door, which point at private endpoints and
    /// want the raw failure.
    pub(crate) async fn connect() -> Self {
        install_subscriber();
        let config = Self::config();
        let redis = afd_dragonfly::test_util::connect_live(&config)
            .await
            .expect("the lane's Dragonfly must be reachable");
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
    pub(crate) fn tls_config() -> DragonflyConfig {
        let url = ProcessEnv.get(TLS_URL_KNOB).unwrap_or_else(|| {
            panic!("{TLS_URL_KNOB} is unset — run these through `make test-integration-rustd`")
        });
        DragonflyConfig::from_url(DragonflyRole::Default, url)
            .with_request_timeout(Duration::from_secs(5))
    }

    /// The configuration the lane hands this suite.
    pub(crate) fn config() -> DragonflyConfig {
        let url = ProcessEnv.get(URL_KNOB).unwrap_or_else(|| {
            panic!("{URL_KNOB} is unset — run these through `make test-integration-rustd`")
        });
        DragonflyConfig::from_url(DragonflyRole::Default, url)
            .with_ca_cert_file(ProcessEnv.get(CA_KNOB).map(Into::into))
            .with_request_timeout(Duration::from_secs(5))
    }

    /// A name unique to this harness, so parallel tests never collide.
    pub(crate) fn name(&self, suffix: &str) -> String {
        format!("{}_{suffix}", self.prefix)
    }
}

/// A fresh `UUIDv7`, the only identifier shape an outbound job carries.
///
/// Stamped with the current instant, with this process and a per-process
/// counter as its random half, so two binaries of one lane never share one and
/// a later run never reuses one an earlier run left in a shared stream. Entropy
/// bytes 0 and 2 lose bits to the version and variant, so the distinguishing
/// values sit after them.
pub(crate) fn mint_id() -> Uuid7 {
    let mut entropy = [0_u8; ENTROPY_LEN];
    entropy[3..7].copy_from_slice(&std::process::id().to_be_bytes());
    let sequence = ID_SEQUENCE.fetch_add(1, Ordering::Relaxed).to_be_bytes();
    entropy[7..].copy_from_slice(&sequence[1..]);
    Uuid7::encode(jiff::Timestamp::now(), entropy).expect("the clock reads after the Unix epoch")
}
