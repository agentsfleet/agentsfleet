//! The lease lane, run small against the rig.
//!
//! One binary per lane, so each holds its own serialising lock over one
//! process and cargo runs the binaries one after another — the readiness
//! index, the outbound consumer and Dragonfly's memory figure are all global to
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
use afd_bench::lane::lease::{self, drain};
use afd_bench::lane::sweep;
use afd_bench::profile::Profile;
use afd_bench::report::{Lane, Report};
use sqlx::Row as _;

use self::support::{LANE, datastores, measurement, swept};

/// A window long enough to lease a handful of fleets, short enough to run.
const WINDOW: Duration = Duration::from_secs(4);

/// The drain's population: the lane's own default, so the test drains what
/// `make bench-lease` drains.
const DRAIN_FLEETS: u64 = 200;

/// The drain's runners, as `make bench-lease` enrols them.
const DRAIN_RUNNERS: u64 = 8;

/// Long enough that the drain ends on its population, never on the clock.
const DRAIN_WINDOW: Duration = Duration::from_mins(2);

/// Ledger rows one settled event leaves: its receive charge and its run.
const LEDGER_ROWS_PER_EVENT: f64 = 2.0;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: cargo test -p afd_bench -- --ignored"]
async fn bench_lease_drains_through_report() {
    let _serial = LANE.lock().await;
    let stores = datastores().await;
    let prefix = RunPrefix::mint();
    let parameters = lease::Parameters {
        fleets: DRAIN_FLEETS,
        runners: DRAIN_RUNNERS,
        window: DRAIN_WINDOW,
    };
    let mut report = Report::new(Lane::Lease, Profile::Rig, support::provenance());

    let drained = drain::run(Profile::Rig, parameters, &stores, &prefix, &mut report).await;
    swept(&stores, &prefix, drained).await;
    // logging: a test prints the numbers it measured so the run that proved the drain also shows its cost.
    println!("{}", drain::summary(&report));

    let events = measurement(&report, "drain_events");
    assert!(
        (events - count(DRAIN_FLEETS)).abs() < f64::EPSILON,
        "one event seeded per fleet"
    );
    assert!(
        (measurement(&report, "drain_leases") - events).abs() < f64::EPSILON,
        "every event was leased once through the plane"
    );
    assert!(
        (measurement(&report, "drain_event_rows") - events).abs() < f64::EPSILON,
        "each event has exactly one row, so none was run twice"
    );
    assert!(
        (measurement(&report, "drain_events_processed") - events).abs() < f64::EPSILON,
        "every event reached its terminal status through the report"
    );
    assert!(
        (measurement(&report, "drain_ledger_rows_per_event") - LEDGER_ROWS_PER_EVENT).abs()
            < f64::EPSILON,
        "each event was charged once as received and once as run"
    );
    assert!(
        measurement(&report, "drain_ready_depth").abs() < f64::EPSILON,
        "the lease path cleared every drained fleet's mark itself"
    );
    assert!(
        measurement(&report, drain::IDLE_STATEMENTS_PER_POLL).is_finite(),
        "the idle cost after the drain is measured, not assumed"
    );
}

/// A count as the float a report holds it in.
fn count(value: u64) -> f64 {
    afd_bench::report::count(value)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: cargo test -p afd_bench -- --ignored"]
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
        lease::run(
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
#[ignore = "needs live datastores: cargo test -p afd_bench -- --ignored"]
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
        lease::run(
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
        measurement(&report, "roundtrips_per_lease") >= 1.0,
        "a lease costs at least the candidate query, read off the daemon's counter"
    );
    assert!(
        report.datastores.postgres.operations > 0,
        "the counter is where the number came from"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: cargo test -p afd_bench -- --ignored"]
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
        lease::run(
            Profile::Rig,
            support::provenance(),
            parameters,
            &stores,
            &prefix,
        )
        .await,
    )
    .await;

    assert!(measurement(&report, "wasted_claim_rate") > 0.0);
}

/// Once every fleet has drained through the lease and report verbs, a poll
/// issues no Postgres statement at all — with no mark cleared by hand.
///
/// The number a million idle fleets multiply. It used to hold only because
/// the lane force-cleared the index before measuring; a drained fleet's mark
/// was never cleared by anything else, so every poll that sampled one paid a
/// claim, a read and a release for a fleet with nothing to do.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: cargo test -p afd_bench -- --ignored"]
async fn bench_idle_poll_after_drain() {
    let _serial = LANE.lock().await;
    let stores = datastores().await;
    let prefix = RunPrefix::mint();
    let parameters = lease::Parameters {
        fleets: 8,
        runners: 2,
        window: WINDOW,
    };
    let mut report = Report::new(Lane::Lease, Profile::Rig, support::provenance());
    // The compose rig is owned, so the index is this test's to reset before it
    // seeds, as `make bench-lease` resets it.
    drain::reset_readiness(&stores)
        .await
        .expect("an owned rig's readiness index resets");

    let drained = drain::run(Profile::Rig, parameters, &stores, &prefix, &mut report).await;
    swept(&stores, &prefix, drained).await;

    assert!(
        measurement(&report, "drain_idle_dragonfly_calls_per_poll") > 0.0,
        "an idle poll still peeks"
    );
    assert!(
        measurement(&report, drain::IDLE_STATEMENTS_PER_POLL).abs() < f64::EPSILON,
        "an idle poll after the drain never reaches Postgres"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs live datastores: cargo test -p afd_bench -- --ignored"]
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

    let mut report = lease::run(
        Profile::Dev,
        support::provenance(),
        parameters,
        &stores,
        &prefix,
    )
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
#[ignore = "dials a port nothing listens on: cargo test -p afd_bench -- --ignored"]
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
