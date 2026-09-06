//! The steer lane, run small against the rig.
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
use afd_bench::lane::{steer, sweep};
use afd_bench::profile::Profile;

use self::support::{LANE, datastores, measurement, series};

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

    let report = steer::run(Profile::Rig, parameters, &stores, &prefix)
        .await
        .expect("runs");
    sweep::everything(&stores.database, &stores.queue, &prefix)
        .await
        .expect("sweeps");

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

    let report = steer::run(Profile::Rig, parameters, &stores, &prefix)
        .await
        .expect("runs");
    sweep::everything(&stores.database, &stores.queue, &prefix)
        .await
        .expect("sweeps");

    assert!(
        report.datastores.redis.operations > 0,
        "a steer is Redis commands"
    );
    assert!(
        measurement(&report, "postgres_transactions_per_steer") < 0.01,
        "ingress never reaches Postgres; the residue is pool keepalive"
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

    let report = steer::run(Profile::Rig, parameters, &stores, &prefix)
        .await
        .expect("runs");
    sweep::everything(&stores.database, &stores.queue, &prefix)
        .await
        .expect("sweeps");

    assert!(
        series(&report, "ready_depth").len() >= 2,
        "a depth series has shape, not one point"
    );
}
