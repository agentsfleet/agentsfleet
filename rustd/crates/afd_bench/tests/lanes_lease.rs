//! The lease lane, run small against the rig.
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
use afd_bench::lane::{lease, sweep};
use afd_bench::profile::Profile;
use sqlx::Row as _;

use self::support::{LANE, datastores, measurement, swept};

/// A window long enough to lease a handful of fleets, short enough to run.
const WINDOW: Duration = Duration::from_secs(4);

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_lease_bench_reports_a_rate_and_a_p95() {
    let _serial = LANE.lock().await;
    let stores = datastores().await;
    let prefix = RunPrefix::mint();
    let parameters = lease::Parameters {
        fleets: 12,
        runners: 3,
        window: WINDOW,
    };

    let report = swept(
        &stores,
        &prefix,
        lease::run(Profile::Rig, parameters, &stores, &prefix).await,
    )
    .await;

    assert!(
        measurement(&report, "rate_per_second") > 0.0,
        "leases per second above zero"
    );
    assert!(measurement(&report, "p95_ms").is_finite());
    assert!(
        (measurement(&report, "leases") - 12.0).abs() < f64::EPSILON,
        "every seeded fleet was leased"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_lease_bench_reports_roundtrips_per_lease() {
    let _serial = LANE.lock().await;
    let stores = datastores().await;
    let prefix = RunPrefix::mint();
    let parameters = lease::Parameters {
        fleets: 8,
        runners: 2,
        window: WINDOW,
    };

    let report = swept(
        &stores,
        &prefix,
        lease::run(Profile::Rig, parameters, &stores, &prefix).await,
    )
    .await;

    assert!(
        measurement(&report, "roundtrips_per_lease") >= 1.0,
        "a lease costs at least the candidate query, read off the daemon's counter"
    );
    assert!(
        report.datastores.postgres.operations > 0,
        "the counter is where the number came from"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_lease_bench_reports_wasted_claim_rate() {
    let _serial = LANE.lock().await;
    let stores = datastores().await;
    let prefix = RunPrefix::mint();
    // Runners outnumber ready fleets, so most polls find another runner got there first.
    let parameters = lease::Parameters {
        fleets: 4,
        runners: 8,
        window: WINDOW,
    };

    let report = swept(
        &stores,
        &prefix,
        lease::run(Profile::Rig, parameters, &stores, &prefix).await,
    )
    .await;

    assert!(measurement(&report, "wasted_claim_rate") > 0.0);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_lease_bench_reports_idle_poll_cost() {
    let _serial = LANE.lock().await;
    let stores = datastores().await;
    let prefix = RunPrefix::mint();
    let parameters = lease::Parameters {
        fleets: 4,
        runners: 2,
        window: WINDOW,
    };

    let report = swept(
        &stores,
        &prefix,
        lease::run(Profile::Rig, parameters, &stores, &prefix).await,
    )
    .await;

    assert!(
        measurement(&report, "idle_redis_calls_per_poll") > 0.0,
        "an idle poll still peeks"
    );
    assert!(
        measurement(&report, "idle_roundtrips_per_poll").abs() < f64::EPSILON,
        "an idle poll never reaches Postgres"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_a_deployed_run_sweeps_everything_it_created() {
    let _serial = LANE.lock().await;
    let stores = datastores().await;
    let prefix = RunPrefix::mint();
    // Dev semantics -- small caps, fixture tenancy -- against the rig target.
    let parameters = lease::Parameters {
        fleets: 6,
        runners: 2,
        window: WINDOW,
    };

    let mut report = lease::run(Profile::Dev, parameters, &stores, &prefix)
        .await
        .expect("runs");
    report.fixture.swept = sweep::everything(&stores.database, &stores.queue, &prefix)
        .await
        .expect("sweeps");

    assert_eq!(
        report.fixture.created, report.fixture.swept,
        "the ledger balances"
    );
    let mut connection = stores
        .database
        .acquire()
        .await
        .expect("a pooled connection");
    let left: i64 = sqlx::query("SELECT count(*) FROM core.fleets WHERE name LIKE $1")
        .bind(format!("{}%", prefix.as_str()))
        .fetch_one(&mut *connection)
        .await
        .expect("the count runs")
        .try_get(0)
        .expect("count is a bigint");
    assert_eq!(
        left, 0,
        "nothing carrying the run prefix outlives the sweep"
    );
}

#[tokio::test]
#[ignore = "dials a port nothing listens on: make test-integration-rustd"]
async fn test_a_run_that_cannot_reach_its_datastore_writes_no_result() {
    let refused = afd_bench::datastores::Datastores::open(
        "postgres://nobody:nobody@127.0.0.1:1/nothing?sslmode=disable",
        "redis://127.0.0.1:1",
        None,
    )
    .await;

    assert!(
        refused.is_err(),
        "a datastore nobody listens on is refused, not measured"
    );
}
