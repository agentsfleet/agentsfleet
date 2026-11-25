//! A person's edit of a task's completion statements.
//!
//! The edit names the generation the person read, and the task's tag: either
//! moved refuses it as a failed precondition, and the task stays unchanged.
//! The new statements and their goal digest replace the old ones and the
//! generation moves on, so a goal check prepared against the old ones is
//! stale. A ready or waiting task is woken at the new generation, so its next
//! run is judged against the new statements; a running task's run finishes
//! its slice under the old generation, and a paused or blocked one waits for
//! a person's resume.

use af_core::id::Uuid7;
use af_wire::durable_task::DurableTaskState;

use super::{
    DurableTaskRefusal, DurableTaskScope, DurableTaskSnapshot, DurableTaskStatements, DurableTasks,
    continuation::Continuation, decision, guard, read, row, sql,
};
use crate::error::{ErrorKind, Result, query};

const CONTEXT: &str = "edit a task's completion statements";

/// [`sql::LOCK_DURABLE_TASK`]'s row, by column name.
#[derive(sqlx::FromRow)]
struct Locked {
    state: String,
    resource_version: i64,
    generation: i64,
}

pub(super) async fn apply(
    tasks: &DurableTasks,
    scope: &DurableTaskScope,
    durable_task_id: &Uuid7,
    expected_tag: &str,
    edit: &DurableTaskStatements,
) -> Result<DurableTaskSnapshot> {
    let parsed = decision::compile::statements(&edit.questions)?;
    tasks.dependencies.policy.statements(&parsed)?;
    let questions = decision::compile::questions(&parsed)?;
    let wake = Continuation::read_next(tasks, scope, durable_task_id, CONTEXT).await?;
    let prepared = tasks.turns.prepare(wake.turn()).await?;
    let mut connection = super::transaction::acquire(tasks.runs.pool()).await?;
    let mut transaction = super::transaction::begin(&mut connection).await?;
    guard::scope(&mut transaction, scope).await?;
    let locked: Locked = sqlx::query_as(sql::LOCK_DURABLE_TASK)
        .bind(durable_task_id)
        .bind(&scope.fleet_id)
        .bind(&scope.workspace_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(query(CONTEXT))?
        .ok_or(DurableTaskRefusal::NotFound)?;
    let before = read::on(&mut transaction, scope, durable_task_id).await?;
    let tag = before.etag();
    if tag != expected_tag
        || row::counter(locked.generation, "generation")? != edit.expected_generation
    {
        return Err(ErrorKind::DurableTaskPrecondition { current_etag: tag }.into());
    }
    let state = row::state(&locked.state)?;
    if state.is_terminal() {
        return Err(DurableTaskRefusal::StaleState.into());
    }
    let wakes = matches!(state, DurableTaskState::Ready | DurableTaskState::Waiting);
    let goal_digest = af_core::etag::sha256_hex(
        &serde_json::to_vec(&(before.context.objective.as_ref(), &questions))
            .map_err(af_core::json::Unreadable::from)?,
    );
    sqlx::query_scalar::<_, Uuid7>(include_str!("sql/edit_statements.sql"))
        .bind(durable_task_id)
        .bind(&scope.fleet_id)
        .bind(&scope.workspace_id)
        .bind(sqlx::types::Json(&questions))
        .bind(&goal_digest)
        .bind(locked.resource_version)
        .bind(wakes)
        .bind(DurableTaskState::Ready.as_str())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(query(CONTEXT))?
        .ok_or(DurableTaskRefusal::StaleState)?;
    if wakes {
        super::waits::cancel(&mut transaction, durable_task_id).await?;
        super::continuation::retire(&mut transaction, durable_task_id).await?;
        let admitted = tasks.turns.stage(&mut transaction, prepared).await?;
        sqlx::query(sql::BIND_TURN)
            .bind(durable_task_id)
            .bind(&admitted.stored.id)
            .execute(&mut *transaction)
            .await
            .map_err(query(CONTEXT))?;
    }
    let snapshot = read::on(&mut transaction, scope, durable_task_id).await?;
    transaction.commit().await.map_err(query(CONTEXT))?;
    super::transaction::recycle(connection);
    tasks
        .publish_changed(scope, durable_task_id, snapshot.resource_version)
        .await;
    if wakes {
        wake.confirm(tasks).await;
    }
    tracing::info!(
        event = "dt_statements_edited",
        durable_task_id = durable_task_id.as_str(),
        generation = snapshot.context.generation
    );
    Ok(snapshot)
}
