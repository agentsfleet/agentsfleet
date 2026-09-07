//! The cardinality lane, run small against the rig, and observed on a deployed profile.
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

use afd_bench::RunPrefix;
use afd_bench::lane::cardinality;
use afd_bench::profile::{Profile, Target};

use self::support::{LANE, datastores, measurement, series, swept};

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_cardinality_bench_reports_memory_per_fleet_across_the_ladder() {
    let _serial = LANE.lock().await;
    let stores = datastores().await;
    let prefix = RunPrefix::mint();
    let parameters = cardinality::Parameters { fleets: 200 };

    let report = swept(
        &stores,
        &prefix,
        cardinality::run(Profile::Rig, &Target::Rig, parameters, &stores, &prefix).await,
    )
    .await;

    let ladder = series(&report, "ladder_fleets");
    assert!(ladder.len() >= 2, "a ladder has rungs");
    for bytes in series(&report, "redis_bytes_per_fleet") {
        assert!(*bytes > 0.0, "every rung costs memory per fleet");
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_cardinality_bench_reports_hot_path_latency_under_cardinality() {
    let _serial = LANE.lock().await;
    let stores = datastores().await;
    let prefix = RunPrefix::mint();
    let parameters = cardinality::Parameters { fleets: 100 };

    let report = swept(
        &stores,
        &prefix,
        cardinality::run(Profile::Rig, &Target::Rig, parameters, &stores, &prefix).await,
    )
    .await;

    assert_eq!(
        series(&report, "peek_ms").len(),
        series(&report, "ladder_fleets").len(),
        "a peek latency per population"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_cardinality_bench_reports_postgres_cost_at_population() {
    let _serial = LANE.lock().await;
    let stores = datastores().await;
    let prefix = RunPrefix::mint();
    let parameters = cardinality::Parameters { fleets: 100 };

    let report = swept(
        &stores,
        &prefix,
        cardinality::run(Profile::Rig, &Target::Rig, parameters, &stores, &prefix).await,
    )
    .await;

    assert!(measurement(&report, "postgres_fleets_table_bytes") > 0.0);
    assert!(
        measurement(&report, "candidate_query_ms") > 0.0,
        "the real candidate query was explained"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_deployed_cardinality_run_creates_nothing() {
    let _serial = LANE.lock().await;
    let stores = datastores().await;
    let prefix = RunPrefix::mint();
    let deployed = Target::Deployed {
        address: "https://dev.example.invalid".to_owned(),
    };

    let report = cardinality::run(
        Profile::Dev,
        &deployed,
        cardinality::Parameters { fleets: 100 },
        &stores,
        &prefix,
    )
    .await
    .expect("observing is a supported run");

    assert!(!report.created, "a deployed profile observes and says so");
    assert_eq!(report.fixture.created, 0);
}
