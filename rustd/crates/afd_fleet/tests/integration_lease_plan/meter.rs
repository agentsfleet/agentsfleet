//! Renewal and settlement find one admission even at the fleet backlog limit.

use super::{relation_scans, run};
use crate::report_seed::Held;
use afd_fleet::lease::sql::meter::METER_LEASE;
use sqlx::PgConnection;

const BACKLOG: &str = "INSERT INTO fleet.admissions \
    (id,fleet_id,workspace_id,producer,producer_key,payload_digest,actor,event_type, \
    request_json,event_created_at,created_at,updated_at) \
    SELECT uuidv7(),f.id,f.workspace_id,'steer','meter-plan-'||n,'fixture','fixture', \
    'chat','{}',now(),now(),now() FROM fleet.fleets f,generate_series(1,$2::integer) n \
    WHERE f.id=$1::uuid";
const PREPARE: &str = "PREPARE ledger_meter \
    (text,text,bigint,text,boolean,interval,interval,text,boolean,text,bigint) AS ";

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
        for settles in [true, false] {
            run(connection, "SAVEPOINT meter_ending").await;
            let plan = meter_plan(connection, held, settles).await;
            assert_indexed_admissions(&plan, size, settles);
            let settled: bool = sqlx::query_scalar(
                "SELECT settled_at IS NOT NULL FROM fleet.admissions WHERE id=$1::uuid",
            )
            .bind(&held.event_id)
            .fetch_one(&mut *connection)
            .await
            .expect("metering reached the admitted row");
            assert_eq!(settled, settles, "backlog {size}, ending {settles}");
            run(connection, "ROLLBACK TO SAVEPOINT meter_ending").await;
        }
        run(connection, "ROLLBACK TO SAVEPOINT meter_backlog").await;
    }
    run(connection, "DEALLOCATE ledger_meter").await;
    run(connection, "ANALYZE fleet.admissions").await;
}

fn assert_indexed_admissions(plan: &serde_json::Value, size: i32, settles: bool) {
    let scans: Vec<_> = relation_scans(plan, "admissions")
        .into_iter()
        .filter(|node| {
            node["Node Type"]
                .as_str()
                .is_some_and(|kind| kind.ends_with("Scan"))
        })
        .collect();
    assert_eq!(scans.len(), 2, "guard and settlement: {plan}");
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
    assert_eq!(executed, if settles { 2 } else { 1 }, "{plan}");
}

async fn meter_plan(
    connection: &mut PgConnection,
    held: &Held,
    settles: bool,
) -> serde_json::Value {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "EXPLAIN (ANALYZE, FORMAT JSON) EXECUTE ledger_meter \
        ('{lease}','{runner}',{ms},'{active}',{settles},'{ttl} seconds','{cap} seconds','{reported}',true,'{work}',{seq})",
        lease = held.lease_key.as_str(),
        runner = held.runner.as_str(),
        ms = afd_billing::nanos::MS_PER_SEC,
        active = afd_fleet::lease::sql::LEASE_STATUS_ACTIVE,
        ttl = afd_core::timing::LEASE_TTL.as_secs_f64(),
        cap = afd_core::timing::MAX_RUNTIME.as_secs_f64(),
        reported = afd_fleet::lease::sql::LEASE_STATUS_REPORTED,
        work = afd_billing::sql::work::FLEET_LEASE,
        seq = afd_wire::usage::DAEMON_SEQ_BASE,
    )))
    .fetch_one(connection)
    .await
    .expect("prepared production metering plan")
}
