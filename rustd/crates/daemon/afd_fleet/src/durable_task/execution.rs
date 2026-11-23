//! One live execution proof shared by task publications and recovery mutations.

use super::{
    DurableTaskRefusal, DurableTaskRun, DurableTaskScope, DurableTaskSnapshot, guard, read, sql,
};
use crate::{Result, error::query};
use af_core::clock;
use af_core::id::Uuid7;
use af_wire::durable_task::{DecisionDurableTaskVersion, DurableTaskState};
use sqlx::PgConnection;

pub(super) struct Owned {
    pub current: guard::RunRow,
    pub scope: DurableTaskScope,
    pub snapshot: DurableTaskSnapshot,
}

pub(super) async fn load(
    connection: &mut PgConnection,
    run: DurableTaskRun<'_>,
    expected: &DecisionDurableTaskVersion,
) -> Result<Owned> {
    let current = guard::run(connection, run).await?;
    on_current(connection, current, expected).await
}

pub(super) async fn on_current(
    connection: &mut PgConnection,
    current: guard::RunRow,
    expected: &DecisionDurableTaskVersion,
) -> Result<Owned> {
    if current.durable_task_id.as_ref().map(Uuid7::as_str)
        != Some(expected.durable_task_id.as_str())
    {
        return Err(DurableTaskRefusal::StaleState.into());
    }
    let owned = attached(connection, current).await?;
    if owned.snapshot.context.generation != expected.generation {
        return Err(DurableTaskRefusal::StaleState.into());
    }
    Ok(owned)
}

pub(super) async fn attached(
    connection: &mut PgConnection,
    current: guard::RunRow,
) -> Result<Owned> {
    if current.fleet_status != crate::run::sql::FLEET_STATUS_ACTIVE {
        return Err(DurableTaskRefusal::StaleState.into());
    }
    let scope = DurableTaskScope {
        fleet_id: current.fleet_id.clone(),
        workspace_id: current.workspace_id.clone(),
    };
    let id = current
        .durable_task_id
        .clone()
        .ok_or(DurableTaskRefusal::NotFound)?;
    let _held: (String,) = sqlx::query_as(sql::LOCK_DURABLE_TASK)
        .bind(&id)
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query("lock selected task execution"))?
        .ok_or(DurableTaskRefusal::NotFound)?;
    let snapshot = read::on(connection, &scope, &id).await?;
    if current.durable_task_generation != i64::try_from(snapshot.context.generation).ok()
        || current.durable_task_stop_generation
            != i64::try_from(snapshot.context.stop_generation).ok()
        || snapshot.context.state != DurableTaskState::Running
        || snapshot.context.expires_at <= clock::now()
    {
        return Err(DurableTaskRefusal::StaleState.into());
    }
    if snapshot.context.usage.model_nanos >= snapshot.context.limits.model_nanos
        || snapshot.context.usage.running_nanos
            >= u64::from(snapshot.context.limits.running_seconds) * 1_000_000_000
    {
        return Err(DurableTaskRefusal::LimitExhausted.into());
    }
    Ok(Owned {
        current,
        scope,
        snapshot,
    })
}
