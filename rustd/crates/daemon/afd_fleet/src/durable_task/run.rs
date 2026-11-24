//! Task ownership joins ordinary run issuance; it never takes another thread.

use super::{DurableTaskScope, read};
use crate::{
    Result,
    error::query,
    run::envelope::{Acquired, HeadKind},
};
use af_core::id::Uuid7;
use af_wire::durable_task::{DurableTaskContext, DurableTaskState};
use sqlx::PgConnection;

// Kill holds FOR UPDATE. Recheck active state after that lock wait, before taking
// the host and thread, so a selected turn cannot survive Kill.
pub(crate) async fn parents(connection: &mut PgConnection, acquired: &Acquired) -> Result<bool> {
    let parent: Option<Uuid7> = sqlx::query_scalar(super::sql::LOCK_ISSUE_SCOPE)
        .bind(&acquired.fleet_id)
        .bind(&acquired.workspace_id)
        .bind(crate::run::sql::FLEET_STATUS_ACTIVE)
        .fetch_optional(connection)
        .await
        .map_err(query("hold surviving issue parents before host and thread"))?;
    Ok(parent.is_some())
}

#[derive(sqlx::FromRow)]
struct Selected {
    durable_task_id: Option<Uuid7>,
    turn_generation: Option<i64>,
    generation: Option<i64>,
    capable: bool,
}

/// Thread and run are locked by the ordinary insert before the task lock.
/// A failed proof rolls that entire issue transaction back, so nothing counts it acquired.
pub(crate) async fn attach(
    connection: &mut PgConnection,
    acquired: &Acquired,
    (runner, run): (&Uuid7, &Uuid7),
    slice_seconds: i32,
) -> Result<Option<(Option<DurableTaskContext<'static>>, jiff::Timestamp)>> {
    // A plain turn has no task to attach and nothing left to read.
    if acquired.opened.head == HeadKind::Plain {
        return Ok(Some((None, acquired.held_until)));
    }
    let selected: Option<Selected> = sqlx::query_as(include_str!("sql/issue_source.sql"))
        .bind(&acquired.turn_id)
        .bind(&acquired.fleet_id)
        .bind(runner)
        .bind(af_wire::durable_task::CAPABILITY)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query("read issued task source"))?;
    let Some(selected) = selected else {
        return Ok(None);
    };
    let Some(durable_task_id) = selected.durable_task_id else {
        return Ok(selected
            .turn_generation
            .is_none()
            .then_some((None, acquired.held_until)));
    };
    let Some(generation) = selected
        .turn_generation
        .filter(|turn| selected.capable && Some(*turn) == selected.generation)
    else {
        return Ok(None);
    };
    attach_durable_task(
        connection,
        acquired,
        run,
        (&durable_task_id, generation),
        slice_seconds,
    )
    .await
}

async fn attach_durable_task(
    connection: &mut PgConnection,
    acquired: &Acquired,
    run: &Uuid7,
    (durable_task_id, generation): (&Uuid7, i64),
    slice_seconds: i32,
) -> Result<Option<(Option<DurableTaskContext<'static>>, jiff::Timestamp)>> {
    let task = durable_task_id.clone();
    let _locked: Option<String> = sqlx::query_scalar(super::sql::LOCK_DURABLE_TASK)
        .bind(durable_task_id)
        .bind(&acquired.fleet_id)
        .bind(&acquired.workspace_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query("lock task for ordinary issue"))?;
    let deadline: Option<jiff_sqlx::Timestamp> =
        sqlx::query_scalar(include_str!("sql/issue_run.sql"))
            .bind(run)
            .bind(durable_task_id)
            .bind(DurableTaskState::Ready.as_str())
            .bind(DurableTaskState::Running.as_str())
            .bind(generation)
            .bind(slice_seconds)
            .bind(NANOS_PER_SECOND)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query("attach task to ordinary run"))?;
    let Some(deadline) = deadline else {
        return Ok(None);
    };
    let scope = DurableTaskScope {
        fleet_id: acquired.fleet_id.clone(),
        workspace_id: acquired.workspace_id.clone(),
    };
    let context = read::on(connection, &scope, &task).await?.context;
    tracing::info!(
        event = "dt_execution_issued",
        durable_task_id = task.as_str(),
        run_id = run.as_str(),
        generation = context.generation
    );
    Ok(Some((Some(context), deadline.to_jiff())))
}

/// A task's slice length, by default.
pub(crate) const SLICE: std::time::Duration = std::time::Duration::from_mins(10);
pub(super) const NANOS_PER_SECOND: i64 = 1_000_000_000;
