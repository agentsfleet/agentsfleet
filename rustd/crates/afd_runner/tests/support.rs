//! Shared live-Redis setup for the runner sweep executable.
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "integration preconditions should fail the test loudly"
)]

use std::time::Duration;

use afd_redis::Redis;
use afd_redis::config::{RedisConfig, RedisRole};

const REDIS_URL_KNOB: &str = "TEST_REDIS_URL";
const REDIS_CA_KNOB: &str = "TEST_REDIS_CA_CERT";

pub(crate) async fn connect_redis() -> Redis {
    let url = std::env::var(REDIS_URL_KNOB)
        .unwrap_or_else(|_| panic!("{REDIS_URL_KNOB} is unset; use the integration make target"));
    let config = RedisConfig::from_url(RedisRole::Default, url)
        .with_ca_cert_file(std::env::var(REDIS_CA_KNOB).ok().map(Into::into))
        .with_connect_timeout(Duration::from_secs(5))
        .with_request_timeout(Duration::from_secs(5));
    afd_redis::test_util::connect_live(&config)
        .await
        .expect("the lane's Redis must be reachable")
}
