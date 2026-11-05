//! The claim drives from the readiness index, whatever the estate's size.

use super::{
    DEALLOCATE, FORCE_GENERIC, Fixtures, PREPARE, claim_plan, read_scans, relation_scans, run,
};
use afd_db::config::DbRole;
use afd_fleet::lease::sql::queue::CLAIM_HEAD;
use sqlx::AssertSqlSafe;
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
/// The API role the daemon's claim runs under.
const API_ROLE: &str = "SET LOCAL ROLE api_runtime";
/// The partial index the claim's oldest-first walk drives from.
const READY_INDEX: &str = "idx_fleet_slots_ready_at_lease_expires_at";
/// Fleets with work waiting in the backlog case.
const WAITING: i32 = 5_000;
/// Of those, the oldest, which a lease holds.
const WAITING_LEASED: i32 = 2_000;
/// One admission for each copy numbered `$2`..`$3`, made `$4` ago, from
/// producer `$5` as event type `$6`. The trigger
/// gives each copy a free slot, ready at that instant.
const ADMIT: &str = "INSERT INTO fleet.admissions \
    (id,fleet_id,workspace_id,producer,producer_key,payload_digest,actor,event_type, \
     request_json,event_created_at,created_at,updated_at) \
    SELECT uuidv7(),f.id,f.workspace_id,$5,f.id::text,'fixture','fixture',$6,'{}', \
           now()-$4::interval,now()-$4::interval,now()-$4::interval \
    FROM fleet.fleets f \
    WHERE f.workspace_id=(SELECT workspace_id FROM fleet.fleets WHERE id=$1::uuid) \
      AND f.name IN (SELECT 'estate-'||n FROM generate_series($2,$3) n)";
/// A live lease on the slots of copies `$2`..`$3`.
const LEASE: &str = "UPDATE runner.fleet_slots s \
    SET lease_expires_at=now()+interval '1 hour', fencing_token=1 \
    FROM fleet.fleets f \
    WHERE f.id=s.fleet_id \
      AND f.workspace_id=(SELECT workspace_id FROM fleet.fleets WHERE id=$1::uuid) \
      AND f.name IN (SELECT 'estate-'||n FROM generate_series($2,$3) n)";

// Value: protects=claim cost bound by ready fleets, not estate size;
// fails_when=the claim scans every fleet or slot; why_new=the scan this
// replaced read every active fleet; seam=none; no_claim=every plan shape.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_claim_plan_uses_ready_index() {
    let fixtures = Fixtures::create_isolated().await;
    let (fleet, _, _, [runner]) = crate::seed::seeded_waiting::<1>(&fixtures).await;
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
    // The claim runs as production runs it, under the API role's grants.
    run(&mut tx, API_ROLE).await;
    run(&mut tx, "SAVEPOINT claim_probe").await;
    let plan = claim_plan(&mut tx, runner.as_str()).await;
    run(&mut tx, "ROLLBACK TO SAVEPOINT claim_probe").await;
    tx.rollback().await.expect("discard estate");
    drop(connection);
    owner.close().await;

    assert_eq!(
        claimed(&plan),
        Some(1.0),
        "the claim took the seeded fleet's waiting work: {plan}"
    );
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

/// The backlog case on `tx`: [`WAITING`] copies of `fleet`, each with one
/// admission, the oldest [`WAITING_LEASED`] of them leased, then `ANALYZE`.
async fn seed_backlog(tx: &mut sqlx::PgConnection, fleet: &str) {
    sqlx::query(ESTATE)
        .bind(fleet)
        .bind(WAITING)
        .execute(&mut *tx)
        .await
        .expect("estate");
    // The leased fleets' work is the oldest, so the walk meets them first.
    for (from, through, age) in [
        (1, WAITING_LEASED, std::time::Duration::from_secs(7_200)),
        (
            WAITING_LEASED + 1,
            WAITING,
            std::time::Duration::from_secs(3_600),
        ),
    ] {
        sqlx::query(ADMIT)
            .bind(fleet)
            .bind(from)
            .bind(through)
            .bind(age)
            .bind(afd_admission::Producer::Steer.as_str())
            .bind(afd_wire::event::EventType::Chat.as_str())
            .execute(&mut *tx)
            .await
            .expect("waiting work");
    }
    sqlx::query(LEASE)
        .bind(fleet)
        .bind(1)
        .bind(WAITING_LEASED)
        .execute(&mut *tx)
        .await
        .expect("leased fleets");
    for table in ["fleet.fleets", "runner.fleet_slots", "fleet.admissions"] {
        run(tx, &format!("ANALYZE {table}")).await;
    }
}

/// The claim in `plan` took a fleet, read one ready slot with the leased ones
/// passed inside the index, and probed one head.
fn assert_one_ready_read(plan: &serde_json::Value) {
    assert_eq!(
        claimed(plan),
        Some(1.0),
        "a waiting fleet was taken: {plan}"
    );
    let walks = executed(plan, "fleet_slots", READY_INDEX);
    let walked: f64 = walks
        .iter()
        .map(|scan| scan["Actual Rows"].as_f64().unwrap_or(f64::MAX))
        .sum();
    assert!(
        (walked - 1.0).abs() < f64::EPSILON,
        "the walk read one ready slot: {plan}"
    );
    for walk in &walks {
        assert!(
            walk["Index Cond"]
                .as_str()
                .is_some_and(|cond| cond.contains("lease_expires_at"))
                && walk["Rows Removed by Filter"].as_f64().unwrap_or(0.0) < 0.5,
            "the leased slots were passed inside the index, never read and filtered: {walk}"
        );
    }
    let heads: f64 = executed(plan, "admissions", "idx_admissions_unsettled")
        .iter()
        .map(|scan| scan["Actual Loops"].as_f64().unwrap_or(f64::MAX))
        .sum();
    assert!(
        (heads - 1.0).abs() < f64::EPSILON,
        "one head probe for the one slot taken: {plan}"
    );
}

// Value: protects=one ready slot read per poll however many fleets wait;
// fails_when=the claim sorts every waiting fleet before taking one;
// why_new=the claim once probed each ready slot's fleet and head per poll;
// seam=none; no_claim=fleets the runner may not take older than the first.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_claim_reads_one_ready_slot_however_many_wait() {
    let fixtures = Fixtures::create_isolated().await;
    let (fleet, _, _, [runner]) = crate::seed::seeded_parts::<1>(&fixtures).await;
    let owner = fixtures.open(DbRole::Migrator).await;
    let mut connection = owner.acquire().await.expect("plan owner");
    let mut tx = connection.begin().await.expect("private backlog");
    seed_backlog(&mut tx, &fleet).await;
    run(&mut tx, API_ROLE).await;
    run(&mut tx, "SAVEPOINT claim_probe").await;
    let plan = claim_plan(&mut tx, runner.as_str()).await;
    run(&mut tx, "ROLLBACK TO SAVEPOINT claim_probe").await;
    tx.rollback().await.expect("discard backlog");
    drop(connection);
    owner.close().await;

    assert_one_ready_read(&plan);
    fixtures.cleanup().await;
}

// Value: protects=the same bound on the plan a cached statement runs;
// fails_when=the generic plan sorts the ready set for its `LIMIT`;
// why_new=production runs the claim as a prepared statement; seam=none;
// no_claim=plans for parameter values the generic plan never saw.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn test_the_cached_claim_plan_reads_one_ready_slot() {
    let fixtures = Fixtures::create_isolated().await;
    let (fleet, _, _, [runner]) = crate::seed::seeded_parts::<1>(&fixtures).await;
    let owner = fixtures.open(DbRole::Migrator).await;
    let mut connection = owner.acquire().await.expect("plan owner");
    let mut tx = connection.begin().await.expect("private backlog");
    seed_backlog(&mut tx, &fleet).await;
    run(&mut tx, API_ROLE).await;
    run(&mut tx, FORCE_GENERIC).await;
    run(&mut tx, &format!("{PREPARE}{CLAIM_HEAD}")).await;
    let plan: serde_json::Value = sqlx::query_scalar(AssertSqlSafe(generic_explain(&runner)))
        .fetch_one(&mut *tx)
        .await
        .expect("the cached claim explains");
    run(&mut tx, DEALLOCATE).await;
    tx.rollback().await.expect("discard backlog");
    drop(connection);
    owner.close().await;

    assert_one_ready_read(&plan);
    fixtures.cleanup().await;
}

/// The prepared claim run and explained for `runner`, every other value the
/// production constant the daemon binds.
fn generic_explain(runner: &afd_core::id::Uuid7) -> String {
    use afd_fleet::lease::sql::{
        FLEET_STATUS_ACTIVE, LEASE_STATUS_ACTIVE, LEASE_STATUS_EXPIRED, RUNNER_STATUS_ACTIVE,
    };
    format!(
        "EXPLAIN (ANALYZE, FORMAT JSON) EXECUTE ledger_claim ('{runner}', '{{}}', \
         '30 seconds', '90 seconds', '{RUNNER_STATUS_ACTIVE}', '{LEASE_STATUS_ACTIVE}', \
         '{LEASE_STATUS_EXPIRED}', '{FLEET_STATUS_ACTIVE}', '{}', '{}', NULL)",
        afd_core::event::status::RECEIVED,
        afd_admission::Producer::Steer.as_str(),
    )
}

/// The rows the claim answered: one when it took a fleet.
fn claimed(plan: &serde_json::Value) -> Option<f64> {
    plan.get(0)
        .and_then(|explained| explained.get("Plan"))
        .and_then(|root| root.get("Actual Rows"))
        .and_then(serde_json::Value::as_f64)
}

/// Every scan of `relation` through `index` that ran.
fn executed<'a>(
    plan: &'a serde_json::Value,
    relation: &str,
    index: &str,
) -> Vec<&'a serde_json::Value> {
    relation_scans(plan, relation)
        .into_iter()
        .filter(|scan| scan["Index Name"] == index)
        .filter(|scan| scan["Actual Loops"].as_f64() != Some(0.0))
        .collect()
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
