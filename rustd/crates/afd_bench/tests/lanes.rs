//! Every lane, run small against the rig, asserting the shape of what it wrote.
//!
//! These are the tests the spec names, one per dimension. They prove a lane
//! REPORTS what it claims to — a rate, a tail, an attribution, a balanced
//! fixture — not that any number is large: the numbers are the lane's output,
//! and a threshold here would be a flake generator on a shared runner.
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
use afd_bench::lane::{cardinality, lease, outbound, steer, sweep};
use afd_bench::profile::{Profile, Target};
use sqlx::Row as _;

use self::support::{LANE, ca_cert, datastores, measurement, redis_url, series};

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

    let report = lease::run(Profile::Rig, parameters, &stores, &prefix)
        .await
        .expect("the lease lane runs on the rig");
    sweep::everything(&stores.database, &stores.queue, &prefix)
        .await
        .expect("sweeps");

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

    let report = lease::run(Profile::Rig, parameters, &stores, &prefix)
        .await
        .expect("runs");
    sweep::everything(&stores.database, &stores.queue, &prefix)
        .await
        .expect("sweeps");

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

    let report = lease::run(Profile::Rig, parameters, &stores, &prefix)
        .await
        .expect("runs");
    sweep::everything(&stores.database, &stores.queue, &prefix)
        .await
        .expect("sweeps");

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

    let report = lease::run(Profile::Rig, parameters, &stores, &prefix)
        .await
        .expect("runs");
    sweep::everything(&stores.database, &stores.queue, &prefix)
        .await
        .expect("sweeps");

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

    let report = outbound::run(
        Profile::Rig,
        parameters,
        &stores,
        &redis_url(),
        ca_cert(),
        &prefix,
    )
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

    let report = outbound::run(
        Profile::Rig,
        parameters,
        &stores,
        &redis_url(),
        ca_cert(),
        &prefix,
    )
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

    let report = outbound::run(
        Profile::Rig,
        parameters,
        &stores,
        &redis_url(),
        ca_cert(),
        &prefix,
    )
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

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: make test-integration-rustd"]
async fn test_cardinality_bench_reports_memory_per_fleet_across_the_ladder() {
    let _serial = LANE.lock().await;
    let stores = datastores().await;
    let prefix = RunPrefix::mint();
    let parameters = cardinality::Parameters { fleets: 200 };

    let report = cardinality::run(Profile::Rig, &Target::Rig, parameters, &stores, &prefix)
        .await
        .expect("runs");
    sweep::everything(&stores.database, &stores.queue, &prefix)
        .await
        .expect("sweeps");

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

    let report = cardinality::run(Profile::Rig, &Target::Rig, parameters, &stores, &prefix)
        .await
        .expect("runs");
    sweep::everything(&stores.database, &stores.queue, &prefix)
        .await
        .expect("sweeps");

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

    let report = cardinality::run(Profile::Rig, &Target::Rig, parameters, &stores, &prefix)
        .await
        .expect("runs");
    sweep::everything(&stores.database, &stores.queue, &prefix)
        .await
        .expect("sweeps");

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

#[tokio::test]
#[ignore = "dials a port nothing listens on: make test-integration-rustd"]
async fn test_a_run_that_cannot_reach_its_datastore_writes_no_result() {
    let path = afd_bench::report::Lane::Lease.result_path(Profile::Rig);
    let _ = std::fs::remove_file(&path);

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
    assert!(!path.exists(), "no connection, no result file");
}

#[test]
fn test_the_existing_loadgen_lane_is_unchanged() {
    let bench_mk = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../make/bench.mk"
    ))
    .expect("make/bench.mk is readable");
    assert!(
        bench_mk.contains("bench:  ## Run the Tier-2 hey HTTP loadgen gate."),
        "the pre-existing target keeps its recipe line"
    );
    assert!(
        bench_mk.contains("@$(MAKE) _bench-loadgen"),
        "and still runs the loadgen, untouched by the lanes"
    );
}
