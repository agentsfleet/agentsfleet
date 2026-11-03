//! Human controls share the fleet slot and retain all cumulative usage.

use afd_core::id::Uuid7;
use afd_wire::task::TaskState;
use sqlx::{Acquire as _, PgConnection};

use super::{
    TaskControl, TaskRefusal, TaskScope, TaskSnapshot, Tasks, continuation::Continuation, guard,
    read, row, sql,
};
use crate::error::{Result, query};

const CONTEXT: &str = "control durable task";

#[derive(sqlx::FromRow)]
struct Origin {
    source_admission_id: String,
    generation: i64,
}

#[derive(sqlx::FromRow)]
struct Locked {
    state: String,
    resource_version: i64,
    can_resume: bool,
}

pub(super) async fn apply(
    tasks: &Tasks,
    scope: &TaskScope,
    task_id: &Uuid7,
    expected_version: u64,
    control: TaskControl,
) -> Result<TaskSnapshot> {
    let continuation = if control == TaskControl::Resume {
        Some(prepare_resume(tasks, scope, task_id).await?)
    } else {
        None
    };
    let prepared = match continuation.as_ref() {
        Some(wake) => Some(tasks.admissions.prepare(wake.admission()).await?),
        None => None,
    };
    let mut connection = tasks.leases.pool().acquire().await?;
    let mut transaction = connection.begin().await.map_err(query(CONTEXT))?;
    guard::scope(&mut transaction, scope).await?;
    let locked: Locked = sqlx::query_as(sql::LOCK_TASK)
        .bind(task_id.as_str())
        .bind(scope.fleet_id.as_str())
        .bind(scope.workspace_id.as_str())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(query(CONTEXT))?
        .ok_or(TaskRefusal::NotFound)?;
    if row::counter(locked.resource_version, "resource_version")? != expected_version {
        return Err(TaskRefusal::StaleState.into());
    }
    let previous = row::state(&locked.state)?;
    let requested = control.requested();
    if previous != requested {
        if previous.is_terminal()
            || (control == TaskControl::Resume
                && !matches!(previous, TaskState::Paused | TaskState::Blocked))
        {
            return Err(TaskRefusal::StaleState.into());
        }
        if control == TaskControl::Resume && !locked.can_resume {
            return Err(TaskRefusal::LimitExhausted.into());
        }
        update(
            &mut transaction,
            scope,
            task_id,
            locked.resource_version,
            control,
        )
        .await?;
        if let Some(prepared) = prepared {
            let admitted = tasks.admissions.stage(&mut transaction, prepared).await?;
            sqlx::query(sql::CONTROL_ADMISSION)
                .bind(task_id.as_str())
                .bind(&admitted.stored.id)
                .execute(&mut *transaction)
                .await
                .map_err(query(CONTEXT))?;
        }
    }
    let snapshot = read::on(&mut transaction, scope, task_id).await?;
    transaction.commit().await.map_err(query(CONTEXT))?;
    if previous != requested {
        tasks
            .publish_changed(scope, task_id, snapshot.resource_version)
            .await;
        if let Some(wake) = continuation {
            wake.confirm(tasks).await;
        }
        tracing::info!(
            event = "dt_transition",
            task_id = task_id.as_str(),
            state = requested.as_str(),
            reason = "human_control"
        );
    }
    Ok(snapshot)
}

async fn prepare_resume(tasks: &Tasks, scope: &TaskScope, task_id: &Uuid7) -> Result<Continuation> {
    let mut connection = tasks.leases.pool().acquire().await?;
    let origin: Origin = sqlx::query_as(sql::CONTROL_ORIGIN)
        .bind(task_id.as_str())
        .bind(scope.fleet_id.as_str())
        .bind(scope.workspace_id.as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?
        .ok_or(TaskRefusal::NotFound)?;
    let generation = row::counter(origin.generation, "generation")?
        .checked_add(1)
        .ok_or(TaskRefusal::LimitExhausted)?;
    Ok(Continuation::new(
        scope,
        task_id,
        generation,
        &guard::id(&origin.source_admission_id, "admission_id")?,
    ))
}

async fn update(
    connection: &mut PgConnection,
    scope: &TaskScope,
    task_id: &Uuid7,
    version: i64,
    control: TaskControl,
) -> Result<()> {
    sqlx::query_scalar::<_, String>(sql::CONTROL)
        .bind(task_id.as_str())
        .bind(scope.fleet_id.as_str())
        .bind(scope.workspace_id.as_str())
        .bind(control.requested().as_str())
        .bind(i64::from(control != TaskControl::Resume))
        .bind(control == TaskControl::Cancel)
        .bind(version)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?
        .ok_or(TaskRefusal::StaleState)?;
    Ok(())
}
