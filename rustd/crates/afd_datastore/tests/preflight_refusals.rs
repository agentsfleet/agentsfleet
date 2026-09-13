//! Dimension 5.1 — every datastore this daemon must not accept work on is
//! refused before any work is accepted.
//!
//! Four bad inputs, each refused for its own reason and each naming what an
//! operator would change: a seed that is not a cluster, a server without
//! sharded pub/sub, a primary that evicts, and a URL that is not a URL. The
//! fifth — a certificate authority that does not verify the lane's server —
//! needs a real TLS handshake and is proven live in `integration_tls_trust`.
//!
//! Against the fake server rather than the cluster, because what is under
//! test is the REFUSAL: a live cluster is by construction none of these
//! things, and a test that had to break one to prove the check would be
//! testing the lane's teardown.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

use std::time::Duration;

use afd_datastore::config::{RedisConfig, RedisRole};
use afd_datastore::{Redis, preflight};

use crate::fake_redis::{FakeRedis, Reply, install_subscriber};

/// How long anything crossing the fake is given before the test fails.
const BUDGET: Duration = Duration::from_secs(10);

/// `COMMAND INFO SSUBSCRIBE` from a server that knows it, and from one that
/// does not. A nil entry is how the protocol spells "never heard of it".
const COMMAND_KNOWN: &str = "*1\r\n*2\r\n$10\r\nssubscribe\r\n:-2\r\n";
const COMMAND_UNKNOWN: &str = "*1\r\n_\r\n";

/// An `INFO memory` payload from a primary that keeps every key.
const INFO_RETAINS: &str = "# Memory\r\nused_memory:1024\r\nmaxmemory_policy:noeviction\r\n";

/// The same, from one configured to discard keys when it fills.
const INFO_EVICTS: &str = "# Memory\r\nused_memory:1024\r\nmaxmemory_policy:allkeys-lru\r\n";

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

/// A server that answers every preflight question the right way, so the
/// refusals below are refusing what they name and not something incidental.
async fn healthy_server() -> FakeRedis {
    FakeRedis::spawn(&[
        ("PING", Reply::Raw("+PONG\r\n")),
        ("COMMAND", Reply::Raw(COMMAND_KNOWN)),
        ("INFO", Reply::Bulk(INFO_RETAINS)),
    ])
    .await
}

async fn refuse(redis: &Redis) -> afd_datastore::Error {
    tokio::time::timeout(BUDGET, preflight::refuse_unsuitable_datastore(redis))
        .await
        .expect("the fake answers every preflight command")
        .expect_err("this datastore must be refused")
}

/// Every bad input refuses boot, each in the class that names its remedy.
#[tokio::test(flavor = "multi_thread")]
async fn test_datastore_preflight_refuses_invalid_configuration() {
    // The control. A datastore that is a cluster, speaks sharded pub/sub and
    // retains every key passes, which is what makes each refusal below
    // attributable to the one thing that was changed.
    let server = healthy_server().await;
    let redis = connect(&server).await;
    tokio::time::timeout(BUDGET, preflight::refuse_unsuitable_datastore(&redis))
        .await
        .expect("the fake answers every preflight command")
        .expect("a cluster that speaks SSUBSCRIBE and retains every key is served");

    // A standalone seed. Refused for what the datastore IS, and the refusal
    // carries what the server reported so an operator is not left guessing
    // which half of the check failed.
    server.set_reply("INFO CLUSTER", Reply::NotACluster);
    let not_a_cluster = refuse(&redis).await;
    assert!(
        not_a_cluster.is_unsuitable_datastore(),
        "a standalone seed is refused for what it is: {not_a_cluster}"
    );
    let rendered = not_a_cluster.to_string();
    assert!(
        rendered.contains("cluster_enabled:0"),
        "the refusal quotes what the server reported: {rendered}"
    );
    server.set_reply("INFO CLUSTER", Reply::InCluster);

    // A server without sharded pub/sub. The same class, because the remedy
    // is the same shape — a different or newer datastore — and a different
    // message, because the thing to change is not.
    server.set_reply("COMMAND", Reply::Raw(COMMAND_UNKNOWN));
    let no_sharded_pubsub = refuse(&redis).await;
    assert!(
        no_sharded_pubsub.is_unsuitable_datastore(),
        "a server without sharded pub/sub is refused: {no_sharded_pubsub}"
    );
    let rendered = no_sharded_pubsub.to_string();
    assert!(
        rendered.contains("SSUBSCRIBE"),
        "the refusal names the command that is missing: {rendered}"
    );
    server.set_reply("COMMAND", Reply::Raw(COMMAND_KNOWN));

    // An evicting primary. Its OWN class, kept apart on purpose: this is a
    // datastore the daemon could serve, configured so that it must not, and
    // the fix is a setting on the node rather than a different node.
    server.set_reply("INFO", Reply::Bulk(INFO_EVICTS));
    let evicting = refuse(&redis).await;
    assert!(
        evicting.is_unsafe_eviction(),
        "an evicting primary is its own class: {evicting}"
    );
    assert!(
        !evicting.is_unsuitable_datastore(),
        "a configured-wrong datastore is not an unsuitable one: {evicting}"
    );
    assert!(
        evicting
            .to_string()
            .contains("maxmemory_policy=allkeys-lru"),
        "the refusal names the setting to change: {evicting}"
    );
}

/// A seed that is not a URL is refused before a socket is opened at all.
///
/// The cheapest refusal, and the one that proves the ladder starts before
/// the network: nothing here spawns a server.
#[tokio::test(flavor = "multi_thread")]
async fn a_seed_that_is_not_a_url_is_refused_without_dialling() {
    install_subscriber();
    let config = RedisConfig::from_url(RedisRole::Default, "not-a-url".to_owned());
    let refused = tokio::time::timeout(BUDGET, Redis::connect(&config))
        .await
        .expect("a malformed seed is refused without waiting on a socket")
        .expect_err("a seed that is not a URL cannot be connected to");
    assert!(
        refused.is_config(),
        "a malformed seed is a configuration fault, not an outage: {refused}"
    );
}
