//! A renewal finds its one admission even at the fleet backlog limit.

use super::{relation_scans, run};
use crate::report_seed::Held;
use afd_fleet::lease::sql::meter::METER_LEASE;
use sqlx::PgConnection;

const BACKLOG: &str = "INSERT INTO fleet.admissions \
    (id,fleet_id,workspace_id,producer,producer_key,payload_digest,actor,event_type, \
    request_json,event_created_at,created_at,updated_at) \
    SELECT uuidv7(),f.id,f.workspace_id,'steer','meter-plan-'||n,'fixture','fixture', \
    'chat','{}',now(),now(),now() FROM fleet.fleets f,generate_series(1,$2::integer) n \
    WHERE f.id=$1";
const PREPARE: &str = "PREPARE ledger_meter \
    (uuid,uuid,bigint,text,interval,interval,text) AS ";

pub(super) async fn assert_point_lookups(connection: &mut PgConnection, held: &Held) {
    run(connection, super::FORCE_GENERIC).await;
    run(connection, &format!("{PREPARE}{METER_LEASE}")).await;
    for size in [1250, 2500, 5000, 10_000] {
        run(connection, "SAVEPOINT meter_backlog").await;
        sqlx::query(BACKLOG)
            .bind(&held.fleet)
            .bind(size - 1)
            .execute(&mut *connection)
            .await
            .expect("per-fleet backlog");
        run(connection, "ANALYZE fleet.admissions").await;
        let plan = meter_plan(connection, held).await;
        assert_indexed_admission(&plan, size);
        run(connection, "ROLLBACK TO SAVEPOINT meter_backlog").await;
    }
    run(connection, "DEALLOCATE ledger_meter").await;
    run(connection, "ANALYZE fleet.admissions").await;
}

fn assert_indexed_admission(plan: &serde_json::Value, size: i32) {
    let scans: Vec<_> = relation_scans(plan, "admissions")
        .into_iter()
        .filter(|node| {
            node["Node Type"]
                .as_str()
                .is_some_and(|kind| kind.ends_with("Scan"))
        })
        .collect();
    assert_eq!(scans.len(), 1, "the guard's one read: {plan}");
    let mut executed = 0;
    for scan in scans {
        assert_eq!(
            scan["Index Name"], "admissions_pkey",
            "backlog {size}: {plan}"
        );
        assert!(
            scan["Index Cond"].as_str().is_some_and(|condition| {
                condition.contains("id =") && condition.contains("event_id")
            }),
            "admission identity must bound the index: {plan}"
        );
        let loops = scan["Actual Loops"].as_u64().expect("scan loop count");
        let rows = scan["Actual Rows"].as_f64().expect("scan row count");
        assert!(loops <= 1 && rows <= 1.0, "backlog {size}: {plan}");
        executed += usize::from(loops == 1);
    }
    assert_eq!(executed, 1, "{plan}");
}

async fn meter_plan(connection: &mut PgConnection, held: &Held) -> serde_json::Value {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "EXPLAIN (ANALYZE, FORMAT JSON) EXECUTE ledger_meter \
        ('{lease}','{runner}',{ms},'{active}','{ttl} seconds','{cap} seconds','{running}')",
        lease = held.lease_key.as_str(),
        runner = held.runner.as_str(),
        ms = afd_billing::nanos::MS_PER_SEC,
        active = afd_fleet::lease::sql::LEASE_STATUS_ACTIVE,
        ttl = af_core::timing::LEASE_TTL.as_secs_f64(),
        cap = af_core::timing::MAX_RUNTIME.as_secs_f64(),
        running = af_wire::task::TaskState::Running.as_str(),
    )))
    .fetch_one(connection)
    .await
    .expect("prepared production metering plan")
}
