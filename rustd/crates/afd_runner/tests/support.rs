//! Shared live-Dragonfly setup for the runner sweep executable.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration preconditions should fail the test loudly"
)]

use std::time::Duration;

use afd_dragonfly::Dragonfly;
use afd_dragonfly::config::{DragonflyConfig, DragonflyRole};

const DRAGONFLY_URL_KNOB: &str = "TEST_DRAGONFLY_URL";
const DRAGONFLY_CA_KNOB: &str = "TEST_DRAGONFLY_CA_CERT";

pub(crate) async fn connect_redis() -> Dragonfly {
    let url = std::env::var(DRAGONFLY_URL_KNOB).unwrap_or_else(|_| {
        panic!("{DRAGONFLY_URL_KNOB} is unset; use the integration make target")
    });
    let config = DragonflyConfig::from_url(DragonflyRole::Default, url)
        .with_ca_cert_file(std::env::var(DRAGONFLY_CA_KNOB).ok().map(Into::into))
        .with_connect_timeout(Duration::from_secs(5))
        .with_request_timeout(Duration::from_secs(5));
    afd_dragonfly::test_util::connect_live(&config)
        .await
        .expect("the lane's Dragonfly must be reachable")
}
