//! The claim drives from the readiness index, whatever the estate's size.

use super::{Fixtures, claim_plan, read_scans, relation_scans, run};
use afd_db::config::DbRole;
use sqlx::Connection as _;

/// Fleets in the estate besides the seeded one.
const POPULATION: i32 = 9_999;
/// Of those, fleets running a lease right now.
const RUNNING: i32 = 2_000;
/// Then fleets that ran once and have nothing waiting.
const IDLE_WITH_SLOTS: i32 = 5_000;
/// Then fleets ready with no lease, beside the seeded fleet's own admission.
const READY_AND_FREE: i32 = 2;

/// The seeded fleet's row, copied `$2` times under `estate-<n>` names.
const ESTATE: &str = "INSERT INTO fleet.fleets \
    (id,workspace_id,tenant_id,name,status,required_tags,memory_reads_workspace, \
     memory_publishes_workspace,resource_version,created_at,updated_at) \
    SELECT uuidv7(),workspace_id,tenant_id,'estate-'||n,status,required_tags,false,false,1,now(),now() \
    FROM fleet.fleets CROSS JOIN generate_series(1,$2) n WHERE id=$1::uuid";
/// Slots for the copies numbered `$2`..`$3`: `$4` running (a live lease and a
/// ready head), else ready and free when `$5`, else idle.
const SLOTS: &str = "INSERT INTO runner.fleet_slots \
    (fleet_id,last_runner_id,fencing_token,lease_expires_at,hold_expires_at,ready_at,created_at,updated_at) \
    SELECT f.id,NULL,1, \
           CASE WHEN $4 THEN now()+interval '1 hour' ELSE now()-interval '1 minute' END, \
           NULL, CASE WHEN $4 OR $5 THEN now() END, now(), now() \
    FROM fleet.fleets f \
    WHERE f.workspace_id=(SELECT workspace_id FROM fleet.fleets WHERE id=$1::uuid) \
      AND f.name IN (SELECT 'estate-'||n FROM generate_series($2,$3) n)";
/// The partial index the claim must drive from.
const READY_INDEX: &str = "idx_fleet_slots_lease_expires_at_ready_at";

// Value: protects=claim cost bound by ready fleets, not estate size;
// fails_when=the claim scans every fleet or slot; why_new=the scan this
// replaced read every active fleet; seam=none; no_claim=every plan shape.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_claim_plan_uses_ready_index() {
    let fixtures = Fixtures::create_isolated().await;
    let (fleet, _, _, [runner]) = crate::seed::seeded_parts::<1>(&fixtures).await;
    let owner = fixtures.open(DbRole::Migrator).await;
    let mut connection = owner.acquire().await.expect("plan owner");
    let mut tx = connection.begin().await.expect("private estate");
    let inserted = sqlx::query(ESTATE)
        .bind(&fleet)
        .bind(POPULATION)
        .execute(&mut *tx)
        .await
        .expect("estate");
    assert_eq!(inserted.rows_affected(), POPULATION as u64);
    let idle_from = RUNNING + 1;
    let ready_from = RUNNING + IDLE_WITH_SLOTS + 1;
    for (from, through, running, ready) in [
        (1, RUNNING, true, false),
        (idle_from, RUNNING + IDLE_WITH_SLOTS, false, false),
        (ready_from, ready_from + READY_AND_FREE - 1, false, true),
    ] {
        sqlx::query(SLOTS)
            .bind(&fleet)
            .bind(from)
            .bind(through)
            .bind(running)
            .bind(ready)
            .execute(&mut *tx)
            .await
            .expect("slots");
    }
    for table in ["fleet.fleets", "runner.fleet_slots", "fleet.admissions"] {
        run(&mut tx, &format!("ANALYZE {table}")).await;
    }
    run(&mut tx, "SAVEPOINT claim_probe").await;
    let plan = claim_plan(&mut tx, runner.as_str()).await;
    run(&mut tx, "ROLLBACK TO SAVEPOINT claim_probe").await;
    tx.rollback().await.expect("discard estate");
    drop(connection);
    owner.close().await;

    let executed_seq_scans: Vec<_> = ["fleets", "fleet_slots"]
        .iter()
        .flat_map(|relation| read_scans(&plan, relation))
        .filter(|node| {
            node["Node Type"] == "Seq Scan" && node["Actual Loops"].as_f64() != Some(0.0)
        })
        .collect();
    assert!(
        executed_seq_scans.is_empty(),
        "no sequential scan of fleets or slots: {executed_seq_scans:?}\n{plan}"
    );
    assert!(
        uses_index(&plan, READY_INDEX),
        "the claim drives from {READY_INDEX}: {plan}"
    );
    assert!(
        !relation_scans(&plan, "fleet_slots").is_empty(),
        "the plan reads the slots at all: {plan}"
    );
    fixtures.cleanup().await;
}

/// Whether any node of `plan` reads through `index`.
fn uses_index(plan: &serde_json::Value, index: &str) -> bool {
    match plan {
        serde_json::Value::Object(fields) => {
            fields.get("Index Name").and_then(serde_json::Value::as_str) == Some(index)
                || fields.values().any(|child| uses_index(child, index))
        }
        serde_json::Value::Array(children) => children.iter().any(|child| uses_index(child, index)),
        _ => false,
    }
}
