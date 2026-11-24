//! A steer joins a span of a person's message to an unfinished task without
//! granting the calling run the task's execution: a waiting or blocked task's
//! wait is cancelled and its continuation queued.

use super::super::{
    DurableTaskBinding, DurableTaskRefusal, DurableTaskScope, DurableTasks, PreparedItem,
    continuation::Continuation, fetch, guard, row,
};
use super::{prepare::Wake, source::Source};
use crate::{Result, error::query};
use af_core::id::Uuid7;
use af_wire::durable_task::DurableTaskState;
use sqlx::PgConnection;

/// What a tool's steer stores as its input's interpretation: the tool sends
/// none, so the accepted text is the input's whole meaning.
const NO_INTERPRETATION: &str = "";

#[derive(sqlx::FromRow)]
struct Origin {
    source: Uuid7,
    generation: i64,
    state: String,
    queued: bool,
    reply_provider: Option<String>,
    reply_address: Option<String>,
}
#[derive(sqlx::FromRow)]
struct Locked {
    generation: i64,
    state: String,
    can_resume: bool,
}

pub(super) async fn wake(
    tasks: &DurableTasks,
    scope: &DurableTaskScope,
    id: &Uuid7,
) -> Result<Option<Wake>> {
    let origin: Origin = fetch::rows(
        tasks.runs.pool(),
        sqlx::query_as(include_str!("sql/input_origin.sql"))
            .bind(id)
            .bind(&scope.fleet_id)
            .bind(&scope.workspace_id),
        None,
        "prepare human-input continuation",
    )
    .await?
    .into_iter()
    .next()
    .ok_or(DurableTaskRefusal::NotFound)?;
    if origin.queued
        || !matches!(
            row::state(&origin.state)?,
            DurableTaskState::Waiting | DurableTaskState::Blocked
        )
    {
        return Ok(None);
    }
    let generation = row::counter(origin.generation, "generation")?;
    Ok(Some(Wake {
        generation,
        continuation: Continuation::new(
            scope,
            id,
            generation
                .checked_add(1)
                .ok_or(DurableTaskRefusal::LimitExhausted)?,
            &origin.source,
        )
        .with_reply(
            origin.reply_provider.as_deref(),
            origin.reply_address.as_deref(),
        )?,
    }))
}

pub(super) async fn accept(
    connection: &mut PgConnection,
    current: &guard::RunRow,
    item: &PreparedItem,
    binding: &mut DurableTaskBinding,
    source: &Source,
    wake: Option<&Wake>,
) -> Result<bool> {
    let Some(id) = &binding.durable_task_id else {
        return Ok(false);
    };
    let locked: Locked = sqlx::query_as(super::super::sql::LOCK_DURABLE_TASK)
        .bind(id)
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query("lock accepted input target"))?
        .ok_or(DurableTaskRefusal::NotFound)?;
    let state = row::state(&locked.state)?;
    // A finished task takes no more input: it is no longer one a run may name.
    if state.is_terminal() {
        return Err(DurableTaskRefusal::NotFound.into());
    }
    let queue = wake.is_some();
    if let Some(wake) = wake {
        if row::counter(locked.generation, "generation")? != wake.generation
            || !matches!(state, DurableTaskState::Waiting | DurableTaskState::Blocked)
        {
            return Err(DurableTaskRefusal::StaleState.into());
        }
        if !locked.can_resume {
            return Err(DurableTaskRefusal::LimitExhausted.into());
        }
    }
    if queue {
        super::super::waits::cancel(connection, id).await?;
    }
    let (sequence, generation): (i64, i64) = sqlx::query_as(include_str!("sql/input_cursor.sql"))
        .bind(id)
        .bind(queue)
        .bind(DurableTaskState::Ready.as_str())
        .fetch_one(&mut *connection)
        .await
        .map_err(query("allocate accepted input sequence"))?;
    if queue {
        super::super::continuation::retire(connection, id).await?;
    }
    let message = source.message()?;
    let span = usize::try_from(item.span().start.unsigned_abs()).unwrap_or(usize::MAX)
        ..usize::try_from(item.span().end.unsigned_abs()).unwrap_or(usize::MAX);
    sqlx::query(include_str!("sql/input.sql"))
        .bind(&binding.item_id)
        .bind(id)
        .bind(&current.fleet_id)
        .bind(&current.workspace_id)
        .bind(sequence)
        .bind(
            message
                .get(span)
                .ok_or(DurableTaskRefusal::InputAmbiguous)?,
        )
        .bind(NO_INTERPRETATION)
        .execute(connection)
        .await
        .map_err(query("retain accepted text and input interpretation"))?;
    binding.applies_here =
        current.durable_task_id.as_ref() == Some(id) && state == DurableTaskState::Running;
    binding.input_sequence = Some(row::counter(sequence, "input sequence")?);
    binding.durable_task_generation = Some(row::counter(generation, "generation")?);
    Ok(queue)
}
