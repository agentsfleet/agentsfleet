//! Execute both queue statements under PostgreSQL's actual prepared-plan cache.

use afd_fleet::lease::sql::{
    self,
    queue::{CLAIM_HEAD, SELECT_CANDIDATES},
};
use sqlx::{AssertSqlSafe, PgConnection};

use super::run;

pub(super) async fn prepare(connection: &mut PgConnection) {
    run(connection, &format!("PREPARE gate_candidates (text,text,bigint,interval,text,text[],text) AS {SELECT_CANDIDATES}")).await;
    run(
        connection,
        &format!("PREPARE gate_claim (text,text,interval,interval,text,text,text) AS {CLAIM_HEAD}"),
    )
    .await;
}

pub(super) async fn release(connection: &mut PgConnection) {
    run(connection, "DEALLOCATE gate_candidates").await;
    run(connection, "DEALLOCATE gate_claim").await;
}

pub(super) async fn candidates(connection: &mut PgConnection, runner: &str) -> serde_json::Value {
    let active = sql::FLEET_STATUS_ACTIVE;
    let runner_active = sql::RUNNER_STATUS_ACTIVE;
    let offline = afd_core::timing::RUNNER_OFFLINE_AFTER.as_secs();
    explain(connection, &format!("gate_candidates('{active}','{runner}',64,'{offline} seconds','{runner_active}','{{}}',NULL)")).await
}

pub(super) async fn claim(
    connection: &mut PgConnection,
    fleet: &str,
    runner: &str,
) -> serde_json::Value {
    let runner_active = sql::RUNNER_STATUS_ACTIVE;
    let active = sql::LEASE_STATUS_ACTIVE;
    let expired = sql::LEASE_STATUS_EXPIRED;
    let ttl = afd_core::timing::LEASE_TTL.as_secs();
    let offline = afd_core::timing::RUNNER_OFFLINE_AFTER.as_secs();
    explain(connection, &format!("gate_claim('{fleet}','{runner}','{ttl} seconds','{offline} seconds','{runner_active}','{active}','{expired}')")).await
}

async fn explain(connection: &mut PgConnection, invocation: &str) -> serde_json::Value {
    // Arguments are generated fixture identifiers and internal constants.
    sqlx::query_scalar(AssertSqlSafe(format!(
        "EXPLAIN (ANALYZE, BUFFERS, FORMAT JSON) EXECUTE {invocation}"
    )))
    .fetch_one(connection)
    .await
    .expect("production prepared plan")
}
