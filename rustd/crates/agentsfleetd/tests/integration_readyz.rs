//! Dimension 7.3 — `/readyz` goes red for a dependency, `/healthz` never does.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`, which supplies `AFD_TEST_DATABASE_URL` and
//! `AFD_TEST_REDIS_TLS_URL`.
//!
//! # "Stopped Postgres", without stopping Postgres
//!
//! The dimension asks what happens when Postgres is unreachable. A test cannot
//! stop the compose container — every other test in the lane is using it — so
//! the pool is CLOSED instead, after it has answered once.
//!
//! That is a faithful stand-in rather than a convenient one. From this
//! instance's point of view, "Postgres stopped" is observed as "the pool will
//! not hand me a connection", which is exactly what a closed pool produces, and
//! it is the same failure every handler would hit. It is also deterministic,
//! where killing a container and waiting for the pool to notice is a race
//! against sqlx's own reconnect timing.
//!
//! What the substitution does NOT cover is the transition — the seconds during
//! which sqlx still holds sockets to a dead server. That belongs to §2's fault
//! lane, which owns a proxy that can break a live connection mid-flight, and it
//! is `test_pool_error_classes` there rather than a second copy here.
//!
//! # Why both probes, in one test
//!
//! `/healthz` is asserted at every step, not once at the end. The whole reason
//! `health.zig` keeps the two apart is that a liveness probe going red gets the
//! process KILLED, which does nothing about Postgres and drops every request
//! the instance was serving. A test that only checked liveness after recovery
//! would miss precisely the window that matters.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly, and a missing lane knob is one"
)]

mod support;

use afd_api::router::Dependencies as _;
use afd_core::env::MapEnv;
use afd_db::Db;
use afd_db::config::{DbRole, PoolConfig};
use afd_redis::Redis;
use afd_redis::config::{CA_CERT_FILE_KNOB, RedisConfig, RedisRole};
use agentsfleetd::probes::LiveDependencies;

use self::support::install_subscriber;

/// Where the lane publishes the Postgres it brought up.
const DATABASE_LANE_KNOB: &str = "AFD_TEST_DATABASE_URL";

/// Where the lane publishes the TLS Redis it brought up.
const REDIS_LANE_KNOB: &str = "AFD_TEST_REDIS_TLS_URL";

/// Where the lane extracted the Redis certificate authority to.
const REDIS_CA_LANE_KNOB: &str = "AFD_TEST_REDIS_TLS_CA_CERT";

/// Reads a lane knob, failing with the command that sets it.
fn lane(knob: &str) -> String {
    std::env::var(knob).unwrap_or_else(|_unset| {
        panic!("{knob} is unset — run these through `make test-integration-rustd`")
    })
}

/// A connected pool and Redis client, both proven to answer.
async fn connected() -> (Db, Redis) {
    install_subscriber();

    let db_env = MapEnv::from_pairs([(DbRole::Api.url_knob(), lane(DATABASE_LANE_KNOB).as_str())]);
    let db_config = PoolConfig::resolve(&db_env, DbRole::Api)
        .expect("the lane publishes a usable database URL");
    let database = Db::connect(&db_config)
        .await
        .expect("the lane's Postgres is up");

    let redis_env = MapEnv::from_pairs([
        (RedisRole::Api.url_knob(), lane(REDIS_LANE_KNOB).as_str()),
        (CA_CERT_FILE_KNOB, lane(REDIS_CA_LANE_KNOB).as_str()),
    ]);
    let redis_config = RedisConfig::resolve(&redis_env, RedisRole::Api)
        .expect("the lane publishes a usable Redis URL");
    let queue = Redis::connect(&redis_config)
        .await
        .expect("the lane's Redis is up");

    (database, queue)
}

/// The dimension: a dependency goes away, readiness follows, liveness does not.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn test_readyz_dependency_probe() {
    let (database, queue) = connected().await;

    // Both up: the instance takes traffic.
    let probes = LiveDependencies::new(database.clone(), queue.clone());
    let inputs = probes.probe().await;
    assert!(
        inputs.database,
        "the lane's Postgres answered at connect; it must answer a probe"
    );
    assert!(inputs.queue, "the lane's Redis answered at connect");
    assert!(
        afd_api::router::ready_decision(inputs),
        "both dependencies up is ready"
    );

    // Postgres goes away.
    database.close().await;

    let inputs = probes.probe().await;
    assert!(
        !inputs.database,
        "a pool that will not hand out a connection is a database that is down"
    );
    assert!(
        inputs.queue,
        "Redis is untouched — a red database and a red queue are different incidents, \
         and collapsing them means reading logs to learn which"
    );
    assert!(
        !afd_api::router::ready_decision(inputs),
        "one dependency down is not ready"
    );
}

/// The probe answers within its own deadline rather than hanging.
///
/// A `/readyz` that never answers reads to an orchestrator as a hung PROCESS,
/// and it restarts the instance over someone else's outage. The bound is the
/// difference between reporting a dependency outage and becoming one.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and Redis: make test-integration-rustd"]
async fn test_readyz_answers_within_its_deadline() {
    let (database, queue) = connected().await;
    database.close().await;

    let probes = LiveDependencies::new(database, queue);
    let answered =
        tokio::time::timeout(agentsfleetd::probes::PROBE_TIMEOUT * 2, probes.probe()).await;

    let inputs = answered.expect("the probe must answer inside twice its own budget");
    assert!(!inputs.database, "and the answer must be the honest one");
}
