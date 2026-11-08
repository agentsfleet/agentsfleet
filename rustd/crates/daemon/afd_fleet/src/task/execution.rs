//! One live execution proof shared by task publications and recovery mutations.

use super::{TaskLease, TaskRefusal, TaskScope, TaskSnapshot, guard, read, sql};
use crate::{Result, error::query};
use af_core::clock;
use af_core::id::Uuid7;
use af_wire::task::{DecisionTaskVersion, TaskState};
use sqlx::PgConnection;

pub(super) struct Owned {
    pub current: guard::LeaseRow,
    pub scope: TaskScope,
    pub snapshot: TaskSnapshot,
}

pub(super) async fn load(
    connection: &mut PgConnection,
    lease: TaskLease<'_>,
    expected: &DecisionTaskVersion,
) -> Result<Owned> {
    let current = guard::lease(connection, lease).await?;
    on_current(connection, current, expected).await
}

pub(super) async fn on_current(
    connection: &mut PgConnection,
    current: guard::LeaseRow,
    expected: &DecisionTaskVersion,
) -> Result<Owned> {
    if current.task_id.as_ref().map(Uuid7::as_str) != Some(expected.task_id.as_str()) {
        return Err(TaskRefusal::StaleState.into());
    }
    let owned = attached(connection, current).await?;
    if owned.snapshot.context.generation != expected.generation {
        return Err(TaskRefusal::StaleState.into());
    }
    Ok(owned)
}

pub(super) async fn attached(
    connection: &mut PgConnection,
    current: guard::LeaseRow,
) -> Result<Owned> {
    if current.fleet_status != crate::lease::sql::FLEET_STATUS_ACTIVE {
        return Err(TaskRefusal::StaleState.into());
    }
    let scope = TaskScope {
        fleet_id: current.fleet_id.clone(),
        workspace_id: current.workspace_id.clone(),
    };
    let id = guard::id(
        current
            .task_id
            .as_ref()
            .ok_or(TaskRefusal::NotFound)?
            .as_str(),
        "task_id",
    )?;
    let _held: (String,) = sqlx::query_as(sql::LOCK_TASK)
        .bind(&id)
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query("lock selected task execution"))?
        .ok_or(TaskRefusal::NotFound)?;
    let snapshot = read::on(connection, &scope, &id).await?;
    if current.task_generation != i64::try_from(snapshot.context.generation).ok()
        || current.task_stop_generation != i64::try_from(snapshot.context.stop_generation).ok()
        || snapshot.context.state != TaskState::Running
        || snapshot.context.expires_at <= clock::now()
    {
        return Err(TaskRefusal::StaleState.into());
    }
    if snapshot.context.usage.model_nanos
        >= snapshot
            .context
            .limits
            .model_nanos
            .saturating_sub(snapshot.context.usage.reserved_model_nanos)
        || snapshot.context.usage.running_nanos
            >= u64::from(snapshot.context.limits.running_seconds) * 1_000_000_000
    {
        return Err(TaskRefusal::LimitExhausted.into());
    }
    Ok(Owned {
        current,
        scope,
        snapshot,
    })
}
