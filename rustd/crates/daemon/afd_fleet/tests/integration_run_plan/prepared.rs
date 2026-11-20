//! Execute both queue statements under PostgreSQL's actual prepared-plan cache.

use af_core::id::Uuid7;
use afd_fleet::run::sql::{
    self,
    queue::{ASSIGN_HEAD, SELECT_CANDIDATES},
};
use sqlx::{AssertSqlSafe, PgConnection};

use super::run;

pub(super) async fn prepare(connection: &mut PgConnection) {
    run(connection, &format!("PREPARE gate_candidates (text,uuid,bigint,interval,text,uuid[],uuid,text,text,text,text[],boolean,timestamptz,uuid,boolean,text) AS {SELECT_CANDIDATES}")).await;
    run(
        connection,
        &format!("PREPARE gate_assignment (uuid,uuid,interval,interval,text,text,text,text,text,text,text[],text,text,boolean,text) AS {ASSIGN_HEAD}"),
    )
    .await;
}

pub(super) async fn release(connection: &mut PgConnection) {
    run(connection, "DEALLOCATE gate_candidates").await;
    run(connection, "DEALLOCATE gate_assignment").await;
}

pub(super) async fn candidates(connection: &mut PgConnection, runner: &Uuid7) -> serde_json::Value {
    let active = sql::FLEET_STATUS_ACTIVE;
    let runner_active = sql::RUNNER_STATUS_ACTIVE;
    let offline = af_core::timing::RUNNER_OFFLINE_AFTER.as_secs();
    let task = task_parameters();
    let model = afd_credential::provider::PLATFORM_MODEL_REF;
    explain(connection, &format!("gate_candidates('{active}','{runner}',64,'{offline} seconds','{runner_active}','{{}}',NULL,{task},false,NULL,NULL,true,'{model}')")).await
}

pub(super) async fn assign(
    connection: &mut PgConnection,
    fleet: &Uuid7,
    runner: &Uuid7,
) -> serde_json::Value {
    let runner_active = sql::RUNNER_STATUS_ACTIVE;
    let active = sql::RUN_STATUS_ACTIVE;
    let expired = sql::RUN_STATUS_EXPIRED;
    let ttl = af_core::timing::RUN_TTL.as_secs();
    let offline = af_core::timing::RUNNER_OFFLINE_AFTER.as_secs();
    let task = task_parameters();
    let fleet_active = sql::FLEET_STATUS_ACTIVE;
    let received = af_core::turn::status::RECEIVED;
    let model = afd_credential::provider::PLATFORM_MODEL_REF;
    explain(connection, &format!("gate_assignment('{fleet}','{runner}','{ttl} seconds','{offline} seconds','{runner_active}','{active}','{expired}',{task},'{fleet_active}','{received}',true,'{model}')")).await
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

/// Only canonical native constants enter these prepared EXECUTE literals.
pub(super) fn task_parameters() -> String {
    let capability = af_wire::task::CAPABILITY;
    let ready = af_wire::task::TaskState::Ready.as_str();
    let running = af_wire::task::TaskState::Running.as_str();
    let path = afd_fleet::task::DECISION_POLICY_PATH.join(",");
    format!("'{capability}','{ready}','{running}','{{{path}}}'")
}
