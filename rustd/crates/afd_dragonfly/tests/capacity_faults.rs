//! What this client does when the server is full, or admits it would rather
//! evict than fill.
//!
//! Neither can be produced by the lane's own cluster on demand — it is sized
//! never to fill and configured never to evict — so both come from the fake,
//! which is the only way to reach the two branches an operator is paged by.
//! These need no live service, so they are not `#[ignore]`d.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_core::error_code;
use afd_dragonfly::config::{RedisConfig, RedisRole};
use afd_dragonfly::streams::FleetStreams;
use afd_dragonfly::{Redis, preflight};

use crate::fake_redis::{FakeRedis, Reply, install_subscriber};

/// Short enough that a hang fails the test rather than the lane's timeout.
const BUDGET: Duration = Duration::from_secs(10);

/// The reply a node past its memory limit gives every write.
const OOM_REPLY: &str = "-OOM command not allowed when used memory > 'maxmemory'.\r\n";

/// `INFO memory` from a node that keeps every key, and from one that does
/// not — as bulk strings, which is how `INFO` answers.
const INFO_NO_EVICTION: &str = "# Memory\r\nused_memory:1024\r\nmaxmemory_policy:noeviction\r\n";
const INFO_CACHE_MODE: &str = "# Memory\r\nused_memory:1024\r\ncache_mode:cache\r\n";

fn config_for(server: &FakeRedis) -> RedisConfig {
    RedisConfig::from_url(RedisRole::Default, server.url())
        .with_request_timeout(Duration::from_secs(2))
}

async fn connect(server: &FakeRedis) -> Redis {
    install_subscriber();
    tokio::time::timeout(BUDGET, Redis::connect(&config_for(server)))
        .await
        .expect("the fake answers PING, so connect must not hang")
        .expect("a fake that answers PONG must be accepted")
}

/// `OOM` is the datastore refusing to GROW, and it must arrive as that class
/// — not as a generic command failure an operator would go and debug the
/// arguments of, and not as an outage they would go and restart a healthy
/// server for. The wire code is the one a producer already retries on.
#[tokio::test(flavor = "multi_thread")]
async fn a_full_datastore_is_its_own_class_and_invites_a_retry() {
    let server = FakeRedis::spawn(&[
        ("PING", Reply::Raw("+PONG\r\n")),
        ("XADD", Reply::Raw(OOM_REPLY)),
    ])
    .await;
    let redis = connect(&server).await;

    let error = tokio::time::timeout(
        BUDGET,
        FleetStreams::new(redis).append("fleet-1", &[("kind", "created")]),
    )
    .await
    .expect("the fake answers, so the append must not hang")
    .expect_err("a full datastore refuses the append");

    assert!(error.is_full(), "OOM is the full class: {error}");
    assert!(
        !error.is_command() && !error.is_unavailable(),
        "full is neither a bad command nor an outage: {error}"
    );
    assert_eq!(
        error.code(),
        error_code::INTERNAL_DB_UNAVAILABLE,
        "the producer is told to come back later"
    );
    assert!(
        error.to_string().contains("XADD"),
        "the failure names the command that was refused: {error}"
    );
}

/// A primary that keeps every key passes preflight; one in cache mode is
/// refused before any work is accepted, naming the node and the setting.
#[tokio::test(flavor = "multi_thread")]
async fn preflight_refuses_an_evicting_primary_and_passes_one_that_retains() {
    let server = FakeRedis::spawn(&[
        ("PING", Reply::Raw("+PONG\r\n")),
        ("INFO", Reply::Bulk(INFO_NO_EVICTION)),
    ])
    .await;
    let redis = connect(&server).await;
    tokio::time::timeout(BUDGET, preflight::refuse_eviction(&redis))
        .await
        .expect("the fake answers INFO")
        .expect("a primary under noeviction keeps every key");

    server.set_reply("INFO", Reply::Bulk(INFO_CACHE_MODE));
    let refused = tokio::time::timeout(BUDGET, preflight::refuse_eviction(&redis))
        .await
        .expect("the fake answers INFO")
        .expect_err("a primary in cache mode is refused");
    assert!(
        refused.is_unsafe_eviction(),
        "eviction is its own class: {refused}"
    );
    let rendered = refused.to_string();
    assert!(
        rendered.contains("cache_mode=cache"),
        "the refusal names the setting an operator changes: {rendered}"
    );
}
