//! Human controls share the fleet thread and retain all cumulative usage.

use af_core::id::Uuid7;
use af_wire::durable_task::DurableTaskState;
use sqlx::PgConnection;

use super::{
    DurableTaskControl, DurableTaskRefusal, DurableTaskScope, DurableTaskSnapshot, DurableTasks,
    continuation::Continuation, guard, read, row, sql,
};
use crate::error::{Result, query};

const CONTEXT: &str = "control durable task";

#[derive(sqlx::FromRow)]
struct Origin {
    source_turn_id: Uuid7,
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
    tasks: &DurableTasks,
    scope: &DurableTaskScope,
    durable_task_id: &Uuid7,
    expected_tag: &str,
    expected_generation: u64,
    control: DurableTaskControl,
) -> Result<DurableTaskSnapshot> {
    let continuation = if control == DurableTaskControl::Resume {
        Some(prepare_resume(tasks, scope, durable_task_id).await?)
    } else {
        None
    };
    let prepared = match continuation.as_ref() {
        Some(wake) => Some(tasks.turns.prepare(wake.turn()).await?),
        None => None,
    };
    // Cancellation closes the socket, so a blocked statement holds no lock.
    let mut connection = super::transaction::acquire(tasks.runs.pool()).await?;
    let mut transaction = super::transaction::begin(&mut connection).await?;
    guard::scope(&mut transaction, scope).await?;
    let (changed, previous) = transition(
        &mut transaction,
        scope,
        durable_task_id,
        expected_tag,
        expected_generation,
        control,
    )
    .await?;
    if changed {
        super::waits::cancel(&mut transaction, durable_task_id).await?;
        super::continuation::retire(&mut transaction, durable_task_id).await?;
        queue(tasks, &mut transaction, durable_task_id, prepared, previous).await?;
    }
    let snapshot = read::on(&mut transaction, scope, durable_task_id).await?;
    transaction.commit().await.map_err(query(CONTEXT))?;
    super::transaction::recycle(connection);
    if changed {
        publish(
            tasks,
            scope,
            durable_task_id,
            &snapshot,
            control,
            continuation,
        )
        .await;
    }
    Ok(snapshot)
}

async fn transition(
    connection: &mut PgConnection,
    scope: &DurableTaskScope,
    durable_task_id: &Uuid7,
    expected_tag: &str,
    expected_generation: u64,
    control: DurableTaskControl,
) -> Result<(bool, DurableTaskState)> {
    let locked: Locked = sqlx::query_as(sql::LOCK_DURABLE_TASK)
        .bind(durable_task_id)
        .bind(&scope.fleet_id)
        .bind(&scope.workspace_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?
        .ok_or(DurableTaskRefusal::NotFound)?;
    let tag = read::on(connection, scope, durable_task_id).await?.etag();
    if tag != expected_tag {
        return Err(crate::error::ErrorKind::DurableTaskPrecondition { current_etag: tag }.into());
    }
    let previous = row::state(&locked.state)?;
    if row::counter(locked.generation, "generation")? != expected_generation {
        return Err(conflict(previous));
    }
    let requested = control.requested();
    let changed = previous != requested;
    if changed {
        if previous.is_terminal()
            || (control == DurableTaskControl::Resume
                && !matches!(
                    previous,
                    DurableTaskState::Paused | DurableTaskState::Blocked
                ))
        {
            return Err(conflict(previous));
        }
        if control == DurableTaskControl::Resume && !locked.can_resume {
            return Err(DurableTaskRefusal::LimitExhausted.into());
        }
        update(
            connection,
            scope,
            durable_task_id,
            locked.resource_version,
            control,
        )
        .await?;
    }
    Ok((changed, previous))
}

async fn queue(
    tasks: &DurableTasks,
    connection: &mut PgConnection,
    durable_task_id: &Uuid7,
    prepared: Option<afd_turns::PreparedTurn<'_>>,
    previous: DurableTaskState,
) -> Result<()> {
    if let Some(prepared) = prepared {
        let admitted = tasks
            .turns
            .stage(&mut *connection, prepared)
            .await
            .map_err(|source| crate::error::ErrorKind::DurableTaskControlTurn {
                state: previous,
                source,
            })?;
        sqlx::query(sql::CONTROL_TURN)
            .bind(durable_task_id)
            .bind(&admitted.stored.id)
            .execute(&mut *connection)
            .await
            .map_err(query(CONTEXT))?;
    }
    Ok(())
}

async fn publish(
    tasks: &DurableTasks,
    scope: &DurableTaskScope,
    durable_task_id: &Uuid7,
    snapshot: &DurableTaskSnapshot,
    control: DurableTaskControl,
    continuation: Option<Continuation>,
) {
    tasks
        .publish_changed(scope, durable_task_id, snapshot.resource_version)
        .await;
    if let Some(wake) = continuation {
        wake.confirm(tasks).await;
    }
    tracing::info!(
        event = "dt_transition",
        durable_task_id = durable_task_id.as_str(),
        state = control.requested().as_str(),
        reason = "human_control"
    );
}

async fn prepare_resume(
    tasks: &DurableTasks,
    scope: &DurableTaskScope,
    durable_task_id: &Uuid7,
) -> Result<Continuation> {
    let mut connection = tasks.runs.pool().acquire().await?;
    let origin: Origin = sqlx::query_as(sql::CONTROL_ORIGIN)
        .bind(durable_task_id)
        .bind(&scope.fleet_id)
        .bind(&scope.workspace_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(CONTEXT))?
        .ok_or(DurableTaskRefusal::NotFound)?;
    let generation = row::counter(origin.generation, "generation")?
        .checked_add(1)
        .ok_or(DurableTaskRefusal::LimitExhausted)?;
    Continuation::new(scope, durable_task_id, generation, &origin.source_turn_id).with_reply(
        origin.reply_provider.as_deref(),
        origin.reply_address.as_deref(),
    )
}

async fn update(
    connection: &mut PgConnection,
    scope: &DurableTaskScope,
    durable_task_id: &Uuid7,
    version: i64,
    control: DurableTaskControl,
) -> Result<()> {
    // `transition` holds the row's lock, so the version it read is still current.
    sqlx::query_scalar::<_, Uuid7>(sql::CONTROL)
        .bind(durable_task_id)
        .bind(&scope.fleet_id)
        .bind(&scope.workspace_id)
        .bind(control.requested().as_str())
        .bind(i64::from(control != DurableTaskControl::Resume))
        .bind(control == DurableTaskControl::Cancel)
        .bind(version)
        .fetch_one(connection)
        .await
        .map_err(query(CONTEXT))?;
    Ok(())
}

fn conflict(state: DurableTaskState) -> crate::Error {
    crate::error::ErrorKind::DurableTaskControlConflict { state }.into()
}
