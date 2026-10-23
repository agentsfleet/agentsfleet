//! A claim finds a prior narrative by its fleet and logical identity.

use super::{Fixtures, relation_scans, run};
use crate::report_seed::held_in;
use afd_db::config::DbRole;
use afd_fleet::lease::sql::queue::CLAIM_HEAD;
use sqlx::{Connection as _, PgConnection};

const RETAINED: &str = "WITH events AS (INSERT INTO fleet.events \
    (fleet_id,workspace_id,event_id,actor,event_type,status,request_json,created_at,updated_at) \
    SELECT l.fleet_id,l.workspace_id,'retained-'||n,'fixture','chat','processed','{}',now(),now() \
    FROM runner.leases l,generate_series(1,4000) n WHERE l.id=$1::uuid RETURNING id) \
    INSERT INTO runner.leases (id,runner_id,fleet_id,workspace_id,tenant_id,fleet_event_id, \
    event_created_at,posture,provider,model,last_metered_at,fencing_token,lease_expires_at, \
    status,posted_message_count,created_at,updated_at) \
    SELECT uuidv7(),l.runner_id,l.fleet_id,l.workspace_id,l.tenant_id,e.id,l.event_created_at, \
    l.posture,l.provider,l.model,l.last_metered_at,0,l.lease_expires_at,$2,0,now(),now() \
    FROM runner.leases l CROSS JOIN events e WHERE l.id=$1::uuid";
const FRESH: &str = "WITH ended AS (UPDATE fleet.admissions SET settled_at=now() \
    WHERE fleet_id=$1::uuid RETURNING fleet_id,workspace_id) \
    INSERT INTO fleet.admissions (id,fleet_id,workspace_id,producer,producer_key,payload_digest, \
    actor,event_type,request_json,event_created_at,created_at,updated_at) \
    SELECT uuidv7(),fleet_id,workspace_id,'steer','fresh-plan','fixture','fixture','chat','{}',now(),now(),now() FROM ended";

// Value: protects=indexed lease-transition lookups; fails_when=identity lookup
// scans a backlog; why_new=small histories hide unbounded index keys; seam=none;
// no_claim=full claim or discovery cost is constant in fleet population.
#[tokio::test]
#[ignore = "needs live Postgres: make test-integration-rustd"]
async fn prior_lease_lookup_uses_both_event_index_keys_for_fresh_and_reclaimed_heads() {
    let held = held_in(Fixtures::create_isolated_with_queue().await).await;
    let owner = held.fixtures.open(DbRole::Migrator).await;
    let mut connection = owner.acquire().await.expect("plan owner");
    let mut tx = connection.begin().await.expect("plan fixture");
    super::meter::assert_point_lookups(&mut tx, &held).await;
    sqlx::query(RETAINED)
        .bind(held.lease_key.as_str())
        .bind(afd_fleet::lease::sql::LEASE_STATUS_REPORTED)
        .execute(&mut *tx)
        .await
        .expect("retained history");
    for table in ["fleet.events", "runner.leases"] {
        run(&mut tx, &format!("ANALYZE {table}")).await;
    }
    sqlx::query("UPDATE runner.fleet_slots SET lease_expires_at=now()-interval '1 second' WHERE fleet_id=$1::uuid")
        .bind(&held.fleet).execute(&mut *tx).await.expect("expired slot");
    run(&mut tx, super::FORCE_GENERIC).await;
    for fresh in [false, true] {
        run(&mut tx, "SAVEPOINT claim_probe").await;
        if fresh {
            sqlx::query(FRESH)
                .bind(&held.fleet)
                .execute(&mut *tx)
                .await
                .expect("fresh head");
        }
        let plan = claim_plan(&mut tx, &held.fleet, held.runner.as_str()).await;
        let scans = relation_scans(&plan, "events");
        assert_eq!(scans.len(), 1, "fresh={fresh}: {plan}");
        let condition = scans.first().expect("prior event lookup")["Index Cond"]
            .as_str()
            .unwrap_or_default();
        assert!(
            condition.contains("fleet_id") && condition.contains("event_id"),
            "fresh={fresh}: {plan}"
        );
        run(&mut tx, "ROLLBACK TO SAVEPOINT claim_probe").await;
    }
    tx.rollback().await.expect("discard plan fixture");
    drop(connection);
    owner.close().await;
    held.fixtures.cleanup().await;
}

async fn claim_plan(connection: &mut PgConnection, fleet: &str, runner: &str) -> serde_json::Value {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "EXPLAIN (ANALYZE, FORMAT JSON) {CLAIM_HEAD}"
    )))
    .bind(fleet)
    .bind(runner)
    .bind(afd_core::timing::LEASE_TTL)
    .bind(afd_core::timing::RUNNER_OFFLINE_AFTER)
    .bind(afd_fleet::lease::sql::RUNNER_STATUS_ACTIVE)
    .bind(afd_fleet::lease::sql::LEASE_STATUS_ACTIVE)
    .bind(afd_fleet::lease::sql::LEASE_STATUS_EXPIRED)
    .fetch_one(connection)
    .await
    .expect("prepared production claim plan")
}
