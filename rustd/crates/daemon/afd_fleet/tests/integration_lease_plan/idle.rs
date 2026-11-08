//! An empty ledger avoids probing every active fleet on each idle poll.

use super::{Fixtures, head_plan, relation_scans, run};
use afd_db::config::DbRole;
use sqlx::Connection as _;

const POPULATION: i32 = 20_000;
const IDLE_FLEETS: &str = "INSERT INTO fleet.fleets \
    (id,workspace_id,tenant_id,name,status,required_tags,memory_reads_workspace, \
     memory_publishes_workspace,resource_version,created_at,updated_at) \
    SELECT uuidv7(),workspace_id,tenant_id,'idle-'||n,status,required_tags,false,false,1,now(),now() \
    FROM fleet.fleets CROSS JOIN generate_series(1,$2) n WHERE id=$1::uuid";

// Value: protects=empty-ledger idle cost; fails_when=discovery visits every idle
// fleet; why_new=backlog tests do not grow idle fleet population; seam=none;
// no_claim=constant server work when unsettled admissions exist.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn an_empty_ledger_skips_fleet_scans_at_twenty_thousand_idle_fleets() {
    let fixtures = Fixtures::create_isolated().await;
    let (fleet, _, _, [runner]) = crate::seed::seeded_parts::<1>(&fixtures).await;
    let owner = fixtures.open(DbRole::Migrator).await;
    let mut connection = owner.acquire().await.expect("plan owner");
    let mut tx = connection.begin().await.expect("private fleet population");
    let inserted = sqlx::query(IDLE_FLEETS)
        .bind(&fleet)
        .bind(POPULATION)
        .execute(&mut *tx)
        .await
        .expect("idle population");
    assert_eq!(inserted.rows_affected(), POPULATION as u64);
    for table in ["fleet.fleets", "fleet.admissions"] {
        run(&mut tx, &format!("ANALYZE {table}")).await;
    }
    let plan = head_plan(&mut tx, runner.as_str()).await;
    let scans = relation_scans(&plan, "fleets");
    eprintln!(
        "idle fleets={POPULATION}, execution_ms={}, fleet_scans={scans:?}",
        plan.get(0)
            .and_then(|root| root.get("Execution Time"))
            .expect("execution time")
    );
    assert!(
        !scans.is_empty(),
        "the production plan contains a fleet scan"
    );
    for scan in scans {
        assert_eq!(scan["Actual Loops"].as_f64(), Some(0.0), "{plan}");
    }
    tx.rollback().await.expect("discard population");
    drop(connection);
    owner.close().await;
    fixtures.cleanup().await;
}
