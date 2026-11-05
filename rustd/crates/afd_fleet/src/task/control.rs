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
    reply_provider: Option<String>,
    reply_address: Option<String>,
    generation: i64,
}

#[derive(sqlx::FromRow)]
struct Locked {
    state: String,
    resource_version: i64,
    generation: i64,
    can_resume: bool,
}

pub(super) async fn apply(
    tasks: &Tasks,
    scope: &TaskScope,
    task_id: &Uuid7,
    expected_tag: &str,
    expected_generation: u64,
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
    let (changed, previous) = transition(
        &mut transaction,
        scope,
        task_id,
        expected_tag,
        expected_generation,
        control,
    )
    .await?;
    if changed {
        super::waits::cancel(&mut transaction, task_id.as_str()).await?;
        super::continuation::retire(&mut transaction, task_id.as_str()).await?;
        queue(tasks, &mut transaction, task_id, prepared, previous).await?;
    }
    let snapshot = read::on(&mut transaction, scope, task_id).await?;
    transaction.commit().await.map_err(query(CONTEXT))?;
    if changed {
        publish(tasks, scope, task_id, &snapshot, control, continuation).await;
    }
    Ok(snapshot)
}

async fn transition(
    connection: &mut PgConnection,
    scope: &TaskScope,
    task_id: &Uuid7,
    expected_tag: &str,
    expected_generation: u64,
    control: TaskControl,
) -> Result<(bool, TaskState)> {
    let locked: Locked = sqlx::query_as(sql::LOCK_TASK)
        .bind(task_id.as_str())
        .bind(scope.fleet_id.as_str())
        .bind(scope.workspace_id.as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?
        .ok_or(TaskRefusal::NotFound)?;
    let tag = read::on(connection, scope, task_id).await?.etag();
    if tag != expected_tag {
        return Err(crate::error::ErrorKind::TaskPrecondition { current_etag: tag }.into());
    }
    let previous = row::state(&locked.state)?;
    if row::counter(locked.generation, "generation")? != expected_generation {
        return Err(conflict(previous));
    }
    let requested = control.requested();
    let changed = previous != requested;
    if changed {
        if previous.is_terminal()
            || (control == TaskControl::Resume
                && !matches!(previous, TaskState::Paused | TaskState::Blocked))
        {
            return Err(conflict(previous));
        }
        if control == TaskControl::Resume && !locked.can_resume {
            return Err(TaskRefusal::LimitExhausted.into());
        }
        update(
            connection,
            scope,
            task_id,
            locked.resource_version,
            control,
            previous,
        )
        .await?;
    }
    Ok((changed, previous))
}

async fn queue(
    tasks: &Tasks,
    connection: &mut PgConnection,
    task_id: &Uuid7,
    prepared: Option<afd_admission::PreparedAdmission<'_>>,
    previous: TaskState,
) -> Result<()> {
    if let Some(prepared) = prepared {
        let admitted = tasks
            .admissions
            .stage(&mut *connection, prepared)
            .await
            .map_err(|source| crate::error::ErrorKind::TaskControlAdmission {
                state: previous,
                source,
            })?;
        sqlx::query(sql::CONTROL_ADMISSION)
            .bind(task_id.as_str())
            .bind(&admitted.stored.id)
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT))?;
    }
    Ok(())
}

async fn publish(
    tasks: &Tasks,
    scope: &TaskScope,
    task_id: &Uuid7,
    snapshot: &TaskSnapshot,
    control: TaskControl,
    continuation: Option<Continuation>,
) {
    tasks
        .publish_changed(scope, task_id, snapshot.resource_version)
        .await;
    if let Some(wake) = continuation {
        wake.confirm(tasks).await;
    }
    tracing::info!(
        event = "dt_transition",
        task_id = task_id.as_str(),
        state = control.requested().as_str(),
        reason = "human_control"
    );
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
    )
    .with_reply(
        origin.reply_provider.as_deref(),
        origin.reply_address.as_deref(),
    )?)
}

async fn update(
    connection: &mut PgConnection,
    scope: &TaskScope,
    task_id: &Uuid7,
    version: i64,
    control: TaskControl,
    previous: TaskState,
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
        .ok_or_else(|| conflict(previous))?;
    Ok(())
}

fn conflict(state: TaskState) -> crate::Error {
    crate::error::ErrorKind::TaskControlConflict { state }.into()
}
