//! The steer lane, run small against the rig.
//!
//! One binary per lane, so each holds its own serialising lock over one
//! process and cargo runs the binaries one after another — the readiness
//! index, the outbound consumer and Dragonfly's memory figure are all global to
//! the server, and two lanes measuring at once would read each other's work.
//!
//! Marked `#[ignore]` so `make test-unit-all` compiles and lints these without
//! datastores, and `make test-integration-rustd` — which runs `--ignored` and
//! nothing else — is the only lane that executes them.

mod support;

use core::time::Duration;

use afd_bench::RunPrefix;
use afd_bench::lane::steer;
use afd_bench::profile::Profile;

use self::support::{LANE, datastores, measurement, series, swept};

/// Postgres transactions one fresh steer commits: the admission insert, then
/// the receipt write once the entry is on the stream.
///
/// Per-steer transactions come in whole numbers, so the lane's ratio sits in
/// `[2, 3)`: at two for these writes, and above only by costs that are not
/// per steer — one prepare per statement per pooled connection, one backlog
/// sample per thousand admissions. Reaching three would mean ingress gained a
/// write.
const TRANSACTIONS_PER_FRESH_STEER: f64 = 2.0;

/// Dragonfly commands one fresh steer issues at the least: the append onto
/// the fleet's stream and the readiness mark. The backlog check before them,
/// and the lane's own `INFO` and depth samples, only add to it.
const STREAM_COMMANDS_PER_FRESH_STEER: f64 = 2.0;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_steer_bench_reports_a_rate_and_a_p95() {
    let _serial = LANE.lock().await;
    let stores = datastores().await;
    let prefix = RunPrefix::mint();
    let parameters = steer::Parameters {
        fleets: 5,
        concurrency: 2,
        window: Duration::from_secs(2),
    };

    let report = swept(
        &stores,
        &prefix,
        steer::run(
            Profile::Rig,
            support::provenance(),
            parameters,
            &stores,
            &prefix,
        )
        .await,
    )
    .await;

    assert!(measurement(&report, "rate_per_second") > 0.0);
    assert!(measurement(&report, "p95_ms").is_finite());
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_steer_bench_attributes_cost_between_datastores() {
    let _serial = LANE.lock().await;
    let stores = datastores().await;
    let prefix = RunPrefix::mint();
    let parameters = steer::Parameters {
        fleets: 5,
        concurrency: 2,
        window: Duration::from_secs(2),
    };

    let report = swept(
        &stores,
        &prefix,
        steer::run(
            Profile::Rig,
            support::provenance(),
            parameters,
            &stores,
            &prefix,
        )
        .await,
    )
    .await;

    let transactions = measurement(&report, "postgres_transactions_per_steer");
    assert!(
        transactions >= TRANSACTIONS_PER_FRESH_STEER,
        "every accepted steer committed its admission insert and its receipt \
         write: {transactions} transactions per steer"
    );
    assert!(
        transactions < TRANSACTIONS_PER_FRESH_STEER + 1.0,
        "no third transaction per steer — what sits above two is fixed cost \
         spread over the window: {transactions} transactions per steer"
    );
    let commands = measurement(&report, "dragonfly_calls_per_steer");
    assert!(
        commands >= STREAM_COMMANDS_PER_FRESH_STEER,
        "Dragonfly still carries the stream append and the readiness mark: \
         {commands} commands per steer"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_steer_bench_reports_readiness_depth_over_time() {
    let _serial = LANE.lock().await;
    let stores = datastores().await;
    let prefix = RunPrefix::mint();
    let parameters = steer::Parameters {
        fleets: 5,
        concurrency: 2,
        window: Duration::from_secs(2),
    };

    let report = swept(
        &stores,
        &prefix,
        steer::run(
            Profile::Rig,
            support::provenance(),
            parameters,
            &stores,
            &prefix,
        )
        .await,
    )
    .await;

    assert!(
        series(&report, "ready_depth").len() >= 2,
        "a depth series has shape, not one point"
    );
}
