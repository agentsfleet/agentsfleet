//! Dimension 7.3 — `/readyz` goes red for a dependency, `/healthz` never does.
//!
//! Marked `#[ignore]` like the rest of the live-service suite; run by
//! `make test-integration-rustd`, which supplies `TEST_DATABASE_URL` and
//! `TEST_DRAGONFLY_URL`.
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
//! the two probes stay apart is that a liveness probe going red gets the
//! process KILLED, which does nothing about Postgres and drops every request
//! the instance was serving. A test that only checked liveness after recovery
//! would miss precisely the window that matters.
#![cfg(feature = "test-util")]
#![expect(
    clippy::expect_used,
    clippy::panic,
    reason = "test target: an unmet precondition should fail the test loudly, and a missing lane knob is one"
)]

use af_core::env::{EnvSource as _, MapEnv, ProcessEnv};
use afd_api::router::Dependencies as _;
use afd_db::Db;
use afd_db::config::{DbRole, PoolConfig};
use agentsfleetd::probes::LiveDependencies;

use crate::support::install_subscriber;

/// Where the lane publishes the Postgres it brought up.
const DATABASE_LANE_KNOB: &str = "TEST_DATABASE_URL";

/// Where the lane publishes its NATS seeds.
const NATS_LANE_SEEDS: &str = "TEST_NATS_URL";

/// Reads a lane knob, failing with the command that sets it.
fn lane(knob: &str) -> String {
    ProcessEnv.get(knob).unwrap_or_else(|| {
        panic!("{knob} is unset — run these through `make test-integration-rustd`")
    })
}

/// A connected pool and event bus, both proven to answer.
async fn connected() -> (Db, afd_bus::Handle) {
    install_subscriber();
    let db_env = MapEnv::from_pairs([(DbRole::Api.url_knob(), lane(DATABASE_LANE_KNOB).as_str())]);
    let db_config = PoolConfig::resolve(&db_env, DbRole::Api)
        .expect("the lane publishes a usable database URL");
    let database = Db::connect(&db_config)
        .await
        .expect("the lane's Postgres is up");
    let bus = bus_at(&lane(NATS_LANE_SEEDS)).await;
    (database, bus)
}

/// A bus through the production boot path, on its own namespace.
async fn bus_at(seeds: &str) -> afd_bus::Handle {
    let namespace = format!("readyz_{}", std::process::id());
    let env = MapEnv::from_pairs([
        (afd_bus::nats::URL_ENV, seeds),
        (afd_bus::nats::NAMESPACE_ENV, namespace.as_str()),
    ]);
    let config = afd_bus::nats::Config::from_env(&env).expect("the lane publishes usable seeds");
    afd_bus::nats::connect(&config)
        .await
        .expect("the lane's NATS is up")
}

/// A relay to one seed that a test can cut, so every connection the bus holds
/// drops and every reconnect is refused.
struct Relay {
    seed: String,
    accept: tokio::task::JoinHandle<()>,
}

impl Relay {
    async fn before(seed: &str) -> Self {
        let (head, port) = seed.rsplit_once(':').expect("a seed has a port");
        let backend: u16 = port.parse().expect("a numeric port");
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("a free local port");
        let local = listener.local_addr().expect("a bound listener").port();
        let accept = tokio::spawn(async move {
            let mut relays = tokio::task::JoinSet::new();
            while let Ok((mut client, _)) = listener.accept().await {
                relays.spawn(async move {
                    if let Ok(mut server) =
                        tokio::net::TcpStream::connect(("127.0.0.1", backend)).await
                    {
                        let _ = tokio::io::copy_bidirectional(&mut client, &mut server).await;
                    }
                });
            }
        });
        Self {
            seed: format!("{head}:{local}"),
            accept,
        }
    }

    /// Drop every relayed connection and refuse new ones.
    fn cut(self) {
        // Aborting the accept task drops the listener and the JoinSet, which
        // aborts every relay it spawned.
        self.accept.abort();
    }
}

/// The dimension: a dependency goes away, readiness follows, liveness does not.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_readyz_dependency_probe() {
    let (database, queue) = connected().await;

    // Both up: the instance takes traffic.
    let probes = LiveDependencies::new(database.clone(), queue.clone());
    let inputs = probes.probe().await;
    assert!(
        inputs.database,
        "the lane's Postgres answered at connect; it must answer a probe"
    );
    assert!(inputs.queue, "the lane's NATS answered at connect");
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
        "the bus is untouched — a red database and a red bus are different incidents, \
         and collapsing them means reading logs to learn which"
    );
    assert!(
        !afd_api::router::ready_decision(inputs),
        "a database that is down is not ready"
    );
}

/// A bus that answers nothing leaves the instance ready (M244 §6.5).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn readyz_stays_ready_through_a_bus_outage() {
    let (database, _bus) = connected().await;
    let seeds = lane(NATS_LANE_SEEDS);
    let first = seeds.split(',').next().expect("the lane exports a seed");
    let relay = Relay::before(first).await;
    let bus = bus_at(&relay.seed).await;
    let probes = LiveDependencies::new(database, bus.clone());
    assert!(probes.probe().await.queue, "connected through the relay");

    relay.cut();
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut inputs = probes.probe().await;
    while inputs.queue && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        inputs = probes.probe().await;
    }
    assert!(inputs.database, "the lane's Postgres answers");
    assert!(
        !inputs.queue,
        "a bus whose every seed is gone is reported down"
    );
    assert!(
        afd_api::router::ready_decision(inputs),
        "Postgres backs every bus signal, so readiness holds"
    );
    let _ = bus.shutdown().await;
}

/// The probe answers within its own deadline rather than hanging.
///
/// A `/readyz` that never answers reads to an orchestrator as a hung PROCESS,
/// and it restarts the instance over someone else's outage. The bound is the
/// difference between reporting a dependency outage and becoming one.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs live Postgres and NATS: make test-integration-rustd"]
async fn test_readyz_answers_within_its_deadline() {
    let (database, queue) = connected().await;
    database.close().await;

    let probes = LiveDependencies::new(database, queue);
    let answered =
        tokio::time::timeout(agentsfleetd::probes::PROBE_TIMEOUT * 2, probes.probe()).await;

    let inputs = answered.expect("the probe must answer inside twice its own budget");
    assert!(!inputs.database, "and the answer must be the honest one");
}
