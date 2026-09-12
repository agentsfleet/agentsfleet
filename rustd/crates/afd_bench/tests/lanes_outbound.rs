//! The delivery lane, run small against the rig.
//!
//! One binary per lane, so each holds its own serialising lock over one
//! process and cargo runs the binaries one after another — the readiness
//! index, the outbound consumer and Redis's memory figure are all global to
//! the server, and two lanes measuring at once would read each other's work.
//!
//! Marked `#[ignore]` so `make test-unit-all` compiles and lints these without
//! datastores, and `make test-integration-rustd` — which runs `--ignored` and
//! nothing else — is the only lane that executes them.

#![expect(
    clippy::expect_used,
    reason = "test target: an unmet precondition should fail the test loudly"
)]

mod support;

use core::time::Duration;

use afd_bench::RunPrefix;
use afd_bench::lane::{outbound, sweep};
use afd_bench::profile::Profile;
use afd_redis::{OutboundJob, OutboundQueue, OutboundReader, outbound_consumer};

use self::support::{LANE, datastores, measurement};

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_benchmark_group_cannot_strand_a_foreign_production_delivery() {
    let _serial = LANE.lock().await;
    let stores = datastores().await;
    let prefix = RunPrefix::mint();
    let group = prefix.name("outbound-consumer-group");
    let production = OutboundQueue::new(stores.queue.clone());
    production
        .ensure_group()
        .await
        .expect("production group exists");
    let isolated = OutboundQueue::isolated_group(stores.queue.clone(), group.clone());
    isolated
        .ensure_group()
        .await
        .expect("isolated group starts at tail");

    let id = production
        .enqueue(OutboundJob {
            provider: "slack",
            workspace_id: "foreign-workspace",
            fleet_id: "foreign-fleet",
            event_id: "foreign-event",
            answer: "foreign-answer",
        })
        .await
        .expect("foreign producer appends after the benchmark group starts");
    let mut benchmark_reader = OutboundReader::isolated_group(
        stores
            .dedicated(Duration::from_secs(5))
            .await
            .expect("dedicated socket"),
        outbound_consumer(),
        group,
    );
    assert_eq!(
        benchmark_reader
            .read_blocking(500)
            .await
            .expect("benchmark read")
            .expect("foreign entry in isolated group")
            .id,
        id
    );

    let mut production_reader = OutboundReader::new(
        stores
            .dedicated(Duration::from_secs(5))
            .await
            .expect("dedicated socket"),
        outbound_consumer(),
    );
    assert_eq!(
        production_reader
            .read_blocking(500)
            .await
            .expect("production read")
            .expect("foreign entry remains visible in production group")
            .id,
        id
    );
    production.ack(&id).await.expect("production ack");
    let removed = sweep::outbound_stream(&stores.queue, &prefix)
        .await
        .expect("sweep group");
    assert_eq!(removed, 1, "the benchmark group is removed");
    sweep::outbound_entries(&stores.queue, &[id.as_str().to_owned()])
        .await
        .expect("remove the foreign test fixture");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_outbound_bench_reports_a_rate_and_a_p95() {
    let _serial = LANE.lock().await;
    let stores = datastores().await;
    let prefix = RunPrefix::mint();
    let parameters = outbound::Parameters {
        jobs: 16,
        slow_fraction: 0.0,
        retryable_fraction: 0.0,
        window: Duration::from_secs(20),
    };

    let report = outbound::run(Profile::Rig, parameters, &stores, &prefix)
        .await
        .expect("runs");
    sweep::outbound_stream(&stores.queue, &prefix)
        .await
        .expect("sweeps");

    assert!(measurement(&report, "rate_per_second") > 0.0);
    assert!(measurement(&report, "p95_ms").is_finite());
    assert!(
        (measurement(&report, "delivered") - 16.0).abs() < f64::EPSILON,
        "a fast stub delivers every job"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_outbound_bench_isolates_the_slow_destination_cost() {
    let _serial = LANE.lock().await;
    let stores = datastores().await;
    let prefix = RunPrefix::mint();
    let parameters = outbound::Parameters {
        jobs: 32,
        slow_fraction: 0.0625,
        retryable_fraction: 0.0,
        window: Duration::from_secs(30),
    };

    let report = outbound::run(Profile::Rig, parameters, &stores, &prefix)
        .await
        .expect("runs");
    sweep::outbound_stream(&stores.queue, &prefix)
        .await
        .expect("sweeps");

    assert!(
        measurement(&report, "others_p95_ms").is_finite(),
        "the OTHER destinations' latency is its own number"
    );
    assert!(measurement(&report, "slow_p95_ms").is_finite());
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_outbound_bench_reports_retry_occupancy() {
    let _serial = LANE.lock().await;
    let stores = datastores().await;
    let prefix = RunPrefix::mint();
    let parameters = outbound::Parameters {
        jobs: 16,
        slow_fraction: 0.0,
        retryable_fraction: 0.0625,
        window: Duration::from_secs(30),
    };

    let report = outbound::run(Profile::Rig, parameters, &stores, &prefix)
        .await
        .expect("runs");
    sweep::outbound_stream(&stores.queue, &prefix)
        .await
        .expect("sweeps");

    assert!(
        measurement(&report, "retry_occupancy") > 0.0,
        "a refusing destination costs the ladder"
    );
}
