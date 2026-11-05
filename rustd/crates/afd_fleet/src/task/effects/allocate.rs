//! Reserve exact immutable publication identity under the task's existing slot.

use super::row::Effect;
use crate::{
    Result,
    error::query,
    task::{TaskLease, TaskRefusal, Tasks, execution, github::BoundPr, guard, row, transaction},
};
use afd_core::clock;
use afd_wire::task::TaskEffectInput;
use sqlx::{PgConnection, types::Json};

#[derive(sqlx::FromRow)]
struct Target {
    target: Json<BoundPr>,
    input_sequence: i64,
    accepted_item: bool,
}

#[derive(serde::Serialize)]
struct Canonical<'a> {
    task: &'a afd_wire::task::DecisionTaskVersion,
    source_item_id: &'a str,
    covered_input_sequence: u64,
    expected_head: &'a str,
    tested_tree: &'a str,
    changes: &'a [afd_wire::task::TaskFileChange],
}

pub(super) async fn reserve(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    input: &garde::Valid<TaskEffectInput>,
) -> Result<(Effect, bool)> {
    let request = serde_json::to_string(&Canonical {
        task: &input.task,
        source_item_id: &input.source_item_id,
        covered_input_sequence: input.covered_input_sequence,
        expected_head: &input.expected_head,
        tested_tree: &input.tested_tree,
        changes: &input.changes,
    })
    .map_err(afd_core::json::Unreadable::from)?;
    let digest = afd_core::etag::sha256_hex(request.as_bytes());
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    // Replays remain readable after a stop, but never regain mutation authority.
    let current = guard::lease(&mut tx, lease).await?;
    if current.task_id.as_deref() != Some(input.task.task_id.as_str()) {
        return Err(TaskRefusal::NotFound.into());
    }
    if let Some(effect) = existing(&mut tx, &input.task.task_id, &input.operation_id).await? {
        if effect.request_digest != digest {
            return Err(TaskRefusal::ReplayConflict.into());
        }
        tx.commit()
            .await
            .map_err(query("read publication replay"))?;
        transaction::recycle(connection);
        return Ok((effect, true));
    }
    let owned = execution::on_current(&mut tx, current, &input.task).await?;
    let target: Target = sqlx::query_as(include_str!("sql/target.sql"))
        .bind(&input.task.task_id)
        .bind(&input.source_item_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(query("read native publication target"))?;
    if !target.accepted_item
        || row::counter(target.input_sequence, "input sequence")? != input.covered_input_sequence
        || owned.snapshot.context.covered_input_sequence != input.covered_input_sequence
        || target.target.head != input.expected_head
    {
        return Err(TaskRefusal::StaleState.into());
    }
    let config = crate::lease::Leases::stored_config_on(&mut tx, &owned.current.fleet_id, None)
        .await?
        .ok_or(TaskRefusal::InvalidTarget)?;
    target.target.within(&config)?;
    let accepted = crate::lease::Leases::stored_config_on(
        &mut tx,
        &owned.current.fleet_id,
        Some(owned.snapshot.context.accepted_revision_id.as_ref()),
    )
    .await?
    .ok_or(TaskRefusal::InvalidTarget)?;
    target.target.within(&accepted)?;
    let free: bool = sqlx::query_scalar(include_str!("sql/available.sql"))
        .bind(&input.task.task_id)
        .bind("prepared")
        .bind("issued")
        .bind("unknown")
        .fetch_one(&mut *tx)
        .await
        .map_err(query("check unresolved publication barrier"))?;
    if !free
        || owned.snapshot.context.usage.publications >= owned.snapshot.context.limits.publications
    {
        return Err(TaskRefusal::EffectUncertain.into());
    }
    let id = tasks.leases.entropy().uuid7(clock::now())?;
    let ordinal: i64 = sqlx::query_scalar(include_str!("sql/cursor.sql"))
        .bind(&input.task.task_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(query("allocate native effect ordinal"))?;
    sqlx::query(include_str!("sql/insert.sql"))
        .bind(id.as_str())
        .bind(&input.task.task_id)
        .bind(&owned.current.fleet_id)
        .bind(&owned.current.workspace_id)
        .bind(&input.source_item_id)
        .bind(lease.lease_id.as_str())
        .bind(lease.runner_id.as_str())
        .bind(owned.current.fence)
        .bind(owned.snapshot.context.generation as i64)
        .bind(owned.snapshot.context.stop_generation as i64)
        .bind(ordinal)
        .bind(&input.operation_id)
        .bind(target.input_sequence)
        .bind(&input.expected_head)
        .bind(&input.tested_tree)
        .bind(digest)
        .bind(super::PREPARATION_SECONDS)
        .bind(
            i64::try_from(target.target.repository_id)
                .map_err(|_overflow| TaskRefusal::InvalidTarget)?,
        )
        .bind(
            i64::try_from(target.target.installation_id)
                .map_err(|_overflow| TaskRefusal::InvalidTarget)?,
        )
        .execute(&mut *tx)
        .await
        .map_err(query("persist immutable publication intent"))?;
    sqlx::query(include_str!("sql/payload.sql"))
        .bind(id.as_str())
        .bind(&input.task.task_id)
        .bind(&owned.current.fleet_id)
        .bind(&owned.current.workspace_id)
        .bind(target.target)
        .bind(request)
        .execute(&mut *tx)
        .await
        .map_err(query("retain private publication target"))?;
    sqlx::query(include_str!("sql/prepare.sql"))
        .bind(id.as_str())
        .bind("prepared")
        .execute(&mut *tx)
        .await
        .map_err(query("prepare native publication"))?;
    let effect = existing(&mut tx, &input.task.task_id, &input.operation_id)
        .await?
        .ok_or(TaskRefusal::NotFound)?;
    tx.commit()
        .await
        .map_err(query("commit publication intent before remote writes"))?;
    transaction::recycle(connection);
    Ok((effect, false))
}

async fn existing(
    connection: &mut PgConnection,
    task: &str,
    operation: &str,
) -> Result<Option<Effect>> {
    sqlx::query_as(include_str!("sql/replay.sql"))
        .bind(task)
        .bind(operation)
        .fetch_optional(connection)
        .await
        .map_err(query("read immutable effect operation"))
}
