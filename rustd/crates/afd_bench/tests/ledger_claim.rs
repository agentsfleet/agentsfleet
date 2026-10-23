//! The measured queue must drain real admissions before publishing its costs.
#![expect(
    clippy::expect_used,
    reason = "a failed fixture precondition fails the benchmark proof"
)]

use afd_core::env::EnvSource as _;
use std::time::Duration;

use afd_bench::datastores::Datastores;
use afd_bench::fixture::RunPrefix;
use afd_bench::lane::{lease, sweep};
use afd_bench::profile::Profile;
use afd_bench::report::{Lane, Provenance, Report};
use afd_db::{Migrator, config::DbRole, test_util::TestDatabase};

const FLEETS: u64 = 200;
const RUNNERS: u64 = 8;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "needs the exclusive benchmark rig; run after make bench-lease"]
async fn test_ledger_claim_lane_reports_round_trips() {
    let lane = database().await;
    let queue = afd_core::env::ProcessEnv
        .get("TEST_DRAGONFLY_URL")
        .expect("test cluster URL");
    let stores = Datastores::open(&lane.url(), &queue, None)
        .await
        .expect("the benchmark rig answers");
    let prefix = RunPrefix::mint();
    let parameters = lease::Parameters {
        fleets: FLEETS,
        runners: RUNNERS,
        window: Duration::from_secs(30),
    };
    let provenance = Provenance {
        revision: "integration-proof".to_owned(),
        datastore_image: "test-cluster".to_owned(),
        owned: true,
    };
    let measured = lease::run_both(Profile::Rig, provenance, parameters, &stores, &prefix).await;
    let swept = sweep::everything(&stores.database, &prefix)
        .await
        .expect("all prefixed fixtures are swept even on a failure");
    let report = measured.expect("the full lease and report path drains");
    assert_eq!(report.fixture.created, swept);
    verify(&report);
    rejects_incomplete(&stores, parameters).await;
    stores.database.close().await;
    lane.cleanup().await;
}

async fn database() -> TestDatabase {
    let lane = TestDatabase::create().await;
    let migrator = lane.open(DbRole::Migrator, &[]).await;
    Migrator::new().run(&migrator).await.expect("real schema");
    migrator.close().await;
    lane
}

/// A real event-write refusal must not publish an empty drain as a cheap one.
async fn rejects_incomplete(stores: &Datastores, parameters: lease::Parameters) {
    let mut connection = stores.database.acquire().await.expect("owned connection");
    sqlx::raw_sql(
        "CREATE FUNCTION fleet.refuse_bench_event() RETURNS trigger LANGUAGE plpgsql AS \
         $$ BEGIN RAISE EXCEPTION 'injected event-write refusal'; END $$; \
         CREATE TRIGGER refuse_bench_event BEFORE INSERT ON fleet.events \
         FOR EACH ROW EXECUTE FUNCTION fleet.refuse_bench_event()",
    )
    .execute(&mut *connection)
    .await
    .expect("failure injection on the actual event table in this owned database");
    drop(connection);
    let prefix = RunPrefix::mint();
    let mut report = Report::new(
        Lane::Lease,
        Profile::Rig,
        Provenance {
            revision: "failure-proof".to_owned(),
            datastore_image: "test-cluster".to_owned(),
            owned: true,
        },
    );
    let measured = lease::drain::run(Profile::Rig, parameters, stores, &prefix, &mut report).await;
    let swept = sweep::everything(&stores.database, &prefix)
        .await
        .expect("refused fixtures swept");
    let refused = measured.expect_err("an incomplete drain refuses measurement");
    assert!(
        refused.to_string().contains("lease drain incomplete"),
        "{refused}"
    );
    assert!(
        !refused.is_pre_flight(),
        "the refusal happened after staging"
    );
    assert!(
        report.measurements.is_empty(),
        "no zero cost for work that never ran"
    );
    assert_eq!(report.fixture.created, swept);
}

fn verify(report: &Report) {
    let measurements = &report.measurements;
    assert_eq!(measurements["drain_events"], 200.0);
    assert_eq!(measurements["drain_event_rows"], 200.0);
    assert_eq!(measurements["drain_events_processed"], 200.0);
    assert_eq!(measurements["drain_leases"], 200.0);
    assert_eq!(measurements["drain_failures"], 0.0);
    assert!(measurements["drain_leases_per_second"] > 0.0);
    assert!(measurements["roundtrips_per_lease"] > 0.0);
    assert!(measurements["drain_statements_per_lease"] > 0.0);
    assert!(measurements["idle_statements_per_poll"] > 0.0);
    assert!(report.abort.is_none());
}
