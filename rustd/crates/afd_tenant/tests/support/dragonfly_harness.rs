//! A connection to the lane's Dragonfly.
//!
//! No key namespacing, unlike the copy in `afd_dragonfly`: every suite here works
//! through a service that MINTS its own identifiers, so two tests running in
//! parallel cannot collide on a key without one of them having minted the
//! other's version 7 identifier. Nothing is flushed between tests either — the
//! lane's Dragonfly is one server shared by every test binary, and a flush would
//! delete another suite's session mid-handshake.

use std::time::Duration;

use afd_core::env::{EnvSource as _, ProcessEnv};
use afd_dragonfly::Dragonfly;
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};

const URL_KNOB: &str = "TEST_DRAGONFLY_URL";
const CA_KNOB: &str = "TEST_DRAGONFLY_CA_CERT";

/// The lane's Dragonfly.
pub(crate) struct DragonflyHarness {
    pub(crate) redis: Dragonfly,
}

impl DragonflyHarness {
    /// Connects, with a deadline short enough that a hung server fails a test
    /// rather than the whole lane's timeout.
    pub(crate) async fn connect() -> Self {
        afd_core::test_util::trace::install_subscriber();
        let config = Self::config();
        let redis = afd_dragonfly::test_util::connect_live(&config)
            .await
            .expect("the lane's Dragonfly must be reachable");
        Self { redis }
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
}
