//! Existing-task inputs retain meaning without granting another task's execution.

use afd_wire::task::{TaskInputAction, TaskState};
use sqlx::PgConnection;
use super::{prepare::Wake, source::Source};
use super::super::{PreparedItem, TaskBinding, TaskLease, TaskRefusal, TaskScope, Tasks, continuation::Continuation, fetch, guard, row};
use crate::{Result, error::query};

#[derive(sqlx::FromRow)]
struct Origin { source: String, generation: i64, state: String, queued: bool, reply_provider: Option<String>, reply_address: Option<String> }
#[derive(sqlx::FromRow)]
struct Locked { generation: i64, state: String, can_resume: bool }

pub(super) async fn wake(tasks: &Tasks, scope: &TaskScope, item: &PreparedItem) -> Result<Option<Wake>> {
    if !matches!(item.action(), TaskInputAction::Steer | TaskInputAction::Refresh) { return Ok(None); }
    let id = item.existing_task().ok_or(TaskRefusal::InvalidTarget)?;
    let origin: Origin = fetch::rows(tasks.leases.pool(), sqlx::query_as(include_str!("sql/input_origin.sql"))
        .bind(id.as_str()).bind(scope.fleet_id.as_str()).bind(scope.workspace_id.as_str()), None, "prepare human-input continuation")
        .await?.into_iter().next().ok_or(TaskRefusal::NotFound)?;
    if origin.queued || !matches!(row::state(&origin.state)?, TaskState::Waiting | TaskState::Blocked) { return Ok(None); }
    let generation = row::counter(origin.generation, "generation")?;
    Ok(Some(Wake { generation, continuation: Continuation::new(scope, id,
        generation.checked_add(1).ok_or(TaskRefusal::LimitExhausted)?, &guard::id(&origin.source, "source_admission_id")?).with_reply(origin.reply_provider.as_deref(), origin.reply_address.as_deref())? }))
}

pub(super) async fn accept(connection: &mut PgConnection, current: &guard::LeaseRow, lease: TaskLease<'_>,
    item: &PreparedItem, binding: &mut TaskBinding, source: &Source, wake: Option<&Wake>) -> Result<bool> {
    let Some(id) = &binding.task_id else { return Ok(false); };
    let locked: Locked = sqlx::query_as(super::super::sql::LOCK_TASK).bind(id.as_str()).bind(&current.fleet_id).bind(&current.workspace_id)
        .fetch_optional(&mut *connection).await.map_err(query("lock accepted input target"))?.ok_or(TaskRefusal::NotFound)?;
    let state = row::state(&locked.state)?;
    if state.is_terminal() && matches!(item.action(), TaskInputAction::Steer | TaskInputAction::Refresh) {
        return Err(TaskRefusal::StaleState.into());
    }
    let queue = wake.is_some();
    if let Some(wake) = wake {
        if row::counter(locked.generation, "generation")? != wake.generation || !matches!(state, TaskState::Waiting | TaskState::Blocked) {
            return Err(TaskRefusal::StaleState.into());
        }
        if !locked.can_resume { return Err(TaskRefusal::LimitExhausted.into()); }
    }
    if queue { super::super::waits::cancel(connection, id.as_str()).await?; }
    let (sequence, generation): (i64, i64) = sqlx::query_as(include_str!("sql/input_cursor.sql"))
        .bind(id.as_str()).bind(queue).bind(TaskState::Ready.as_str()).fetch_one(&mut *connection).await.map_err(query("allocate accepted input sequence"))?;
    if queue { super::super::continuation::retire(connection, id.as_str()).await?; }
    let message = afd_wire::event::message_of(&source.request_json);
    let span = item.span().start.unsigned_abs() as usize..item.span().end.unsigned_abs() as usize;
    sqlx::query(include_str!("sql/input.sql")).bind(binding.item_id.as_str()).bind(id.as_str())
        .bind(&current.fleet_id).bind(&current.workspace_id).bind(sequence).bind(message.get(span).ok_or(TaskRefusal::InputAmbiguous)?)
        .bind(item.interpretation()).bind(lease.lease_id.as_str()).bind(current.fence).bind(generation)
        .execute(connection).await.map_err(query("retain accepted text and input interpretation"))?;
    binding.applies_here = current.task_id.as_deref() == Some(id.as_str()) && state == TaskState::Running;
    binding.input_sequence = Some(row::counter(sequence, "input sequence")?);
    binding.task_generation = Some(row::counter(generation, "generation")?);
    Ok(queue)
}
