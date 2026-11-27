//! Allocate a complete immutable capture plan before any transfer authorization.

use super::{
    guard,
    row::{Pin, Plan},
};
use crate::{
    Result,
    durable_task::{
        DurableTaskRefusal, DurableTaskRun, DurableTasks, execution,
        github::BoundPr,
        guard as durable_task_guard,
        row::{sql_counter, sql_small_counter},
        transaction,
    },
    error::query,
};
use af_core::clock;
use af_core::id::Uuid7;
use af_wire::durable_task::{CheckpointMode, CheckpointPlan, CheckpointPlanInput};
use sqlx::{PgConnection, types::Json};

/// A task's allocation receipt: the plan the operation created and the request
/// that created it.
#[derive(sqlx::FromRow)]
struct Receipt {
    checkpoint_id: Uuid7,
    request_digest: String,
}

pub(in crate::durable_task) async fn create(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    input: &garde::Valid<CheckpointPlanInput>,
) -> Result<CheckpointPlan> {
    // Replay and plan allocation need no remote round trip; upload activation
    // is proved before coding admission and again before the first signed PUT.
    let _configured = guard::storage(tasks)?;
    let digest = request_digest(input)?;
    let mut connection = transaction::acquire(tasks.runs.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let current = durable_task_guard::run(&mut tx, run).await?;
    let task = current
        .durable_task_id
        .clone()
        .filter(|task| *task == input.durable_task.durable_task_id)
        .ok_or(DurableTaskRefusal::NotFound)?;
    let scope = super::receipt_scope(&current.workspace_id, &task);
    let existing: Option<Receipt> = sqlx::query_as(include_str!("sql/plan_replay.sql"))
        .bind(&scope)
        .bind(durable_task_guard::wire(&input.operation_id)?)
        .fetch_optional(&mut *tx)
        .await
        .map_err(query("read immutable checkpoint plan replay"))?;
    if let Some(receipt) = existing {
        if receipt.request_digest != digest {
            return Err(DurableTaskRefusal::ReplayConflict.into());
        }
        let id = receipt.checkpoint_id;
        let plan = Plan::load(&mut tx, &id, &task).await?;
        let pin: Pin = sqlx::query_as(include_str!("sql/plan_pin.sql"))
            .bind(&id)
            .bind("writer")
            .fetch_one(&mut *tx)
            .await
            .map_err(query("read original checkpoint writer pin"))?;
        let response = plan.wire(&pin, true)?;
        tx.commit()
            .await
            .map_err(query("finish checkpoint plan replay"))?;
        transaction::recycle(connection);
        return Ok(response);
    }
    let owned = execution::on_current(&mut tx, current, &input.durable_task).await?;
    let (repository, commit) = target(
        &mut tx,
        &task,
        &owned.current.fleet_id,
        input.commit_oid.as_deref(),
    )
    .await?;
    if input.mode == CheckpointMode::RetainBase && input.base_id.is_some() {
        return Err(DurableTaskRefusal::CheckpointIncompatible.into());
    }
    parent(&mut tx, input, &owned, &task).await?;
    super::lock_storage(&mut tx, &owned.current.workspace_id).await?;
    sqlx::query(include_str!("sql/abandon_capture.sql"))
        .bind(&task)
        .bind(run.run_id)
        .bind(owned.current.fence)
        .execute(&mut *tx)
        .await
        .map_err(query("abandon only stale checkpoint capture ownership"))?;
    sqlx::query(include_str!("sql/retire_unfinished_base.sql"))
        .bind(&owned.current.workspace_id)
        .bind(&repository)
        .execute(&mut *tx)
        .await
        .map_err(query("retire abandoned unfinished repository bases"))?;
    let id = allocate(
        tasks,
        &mut tx,
        run,
        input,
        &owned,
        (&digest, &scope),
        (repository, commit),
    )
    .await?;
    let plan = Plan::load(&mut tx, &id, &task).await?;
    let pin = super::pins::insert(tasks, &mut tx, &owned, &plan, run, "writer").await?;
    let response = plan.wire(&pin, false)?;
    tx.commit()
        .await
        .map_err(query("commit checkpoint plan and writer pin"))?;
    transaction::recycle(connection);
    Ok(response)
}

/// A delta is applied over the task's current checkpoint, over the same
/// retained base, and over nothing else; a retained base is always full.
async fn parent(
    connection: &mut PgConnection,
    input: &CheckpointPlanInput,
    owned: &execution::Owned,
    task: &Uuid7,
) -> Result<()> {
    let Some(parent) = &input.parent else {
        return Ok(());
    };
    if input.mode != CheckpointMode::Incremental
        || owned.snapshot.context.checkpoint_id.as_deref() != Some(parent.as_str())
    {
        return Err(DurableTaskRefusal::CheckpointIncompatible.into());
    }
    let base: Option<Uuid7> = sqlx::query_scalar(include_str!("sql/parent_base.sql"))
        .bind(durable_task_guard::wire(parent)?)
        .bind(task)
        .bind(CheckpointMode::Incremental.as_str())
        .fetch_optional(connection)
        .await
        .map_err(query("read the retained base a delta's parent overlays"))?
        .ok_or(DurableTaskRefusal::CheckpointIncompatible)?;
    let named = input
        .base_id
        .as_deref()
        .map(durable_task_guard::wire)
        .transpose()?;
    if base != named {
        return Err(DurableTaskRefusal::CheckpointIncompatible.into());
    }
    Ok(())
}

fn request_digest(input: &CheckpointPlanInput) -> Result<String> {
    Ok(af_core::etag::sha256_hex(
        &serde_json::to_vec(&(
            &input.durable_task,
            &input.operation_id,
            input.mode,
            &input.parent,
            &input.base_id,
            &input.commit_oid,
            input.format_version,
            &input.toolbox_digest,
            &input.sandbox,
            input.page_count,
            input.upload_budget_bytes,
            input.logical_bytes,
            input.entry_count,
        ))
        .map_err(af_core::json::Unreadable::from)?,
    ))
}

async fn allocate(
    tasks: &DurableTasks,
    connection: &mut PgConnection,
    run: DurableTaskRun<'_>,
    input: &CheckpointPlanInput,
    owned: &execution::Owned,
    (digest, scope): (&str, &str),
    repository_target: (String, Option<String>),
) -> Result<Uuid7> {
    let (repository, commit) = repository_target;
    let task = owned
        .current
        .durable_task_id
        .as_ref()
        .ok_or(DurableTaskRefusal::NotFound)?;
    // A `u32` always fits the `usize` of every target this daemon builds for.
    let page_count = usize::try_from(input.page_count).unwrap_or(usize::MAX);
    let mut pages = std::iter::repeat_with(|| tasks.runs.entropy().uuid7(clock::now()))
        .take(page_count)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let base = super::base::select(
        tasks,
        connection,
        input,
        &owned.current.workspace_id,
        &repository,
        commit.as_deref(),
        &mut pages,
    )
    .await?;
    let id = tasks.runs.entropy().uuid7(clock::now())?;
    let ready_base = base
        .as_ref()
        .filter(|base| input.mode == CheckpointMode::RetainBase && base.ready);
    let (engine, guest_agent) = super::sandbox::columns(input.sandbox.as_ref());
    sqlx::query(include_str!("sql/plan_insert.sql"))
        .bind(&id)
        .bind(task)
        .bind(&owned.current.fleet_id)
        .bind(&owned.current.workspace_id)
        .bind(sql_counter(
            input.durable_task.generation,
            "checkpoint task generation",
        )?)
        .bind(sql_counter(
            owned.snapshot.context.stop_generation,
            "checkpoint stop generation",
        )?)
        .bind(durable_task_guard::wire(&input.operation_id)?)
        .bind(digest)
        .bind(input.mode.as_str())
        .bind(repository)
        .bind(base.as_ref().map(|base| &base.id))
        .bind(commit)
        .bind(sql_small_counter(
            input.format_version,
            "checkpoint format_version",
        )?)
        .bind(&input.toolbox_digest)
        .bind(&pages)
        .bind(sql_counter(
            input.upload_budget_bytes,
            "checkpoint upload_budget_bytes",
        )?)
        .bind(ready_base.map_or(
            sql_counter(input.logical_bytes, "checkpoint logical_bytes")?,
            |base| base.logical_bytes,
        ))
        .bind(ready_base.map_or(
            sql_small_counter(input.entry_count, "checkpoint entry_count")?,
            |base| base.entry_count,
        ))
        .bind(run.run_id)
        .bind(owned.current.fence)
        .bind(tasks.times().transfer_seconds())
        .bind(ready_base.and_then(|base| base.manifest_root.as_deref()))
        .bind(scope)
        .bind(<&str>::from(guard::storage(tasks)?.keys().cipher()))
        .bind(
            input
                .parent
                .as_deref()
                .map(durable_task_guard::wire)
                .transpose()?,
        )
        .bind(engine)
        .bind(guest_agent)
        .execute(&mut *connection)
        .await
        .map_err(query("persist native checkpoint plan"))?;
    if input.mode == CheckpointMode::Incremental {
        sqlx::query(crate::durable_task::sql::PAGES_ALLOCATE)
            .bind(&id)
            .bind(&pages)
            .execute(&mut *connection)
            .await
            .map_err(query("allocate immutable checkpoint page identities"))?;
    }
    Ok(id)
}

async fn target(
    connection: &mut PgConnection,
    task: &Uuid7,
    fleet: &Uuid7,
    commit: Option<&str>,
) -> Result<(String, Option<String>)> {
    let Json(target): Json<serde_json::Value> =
        sqlx::query_scalar(include_str!("sql/durable_task_target.sql"))
            .bind(task)
            .fetch_one(connection)
            .await
            .map_err(query("read native recovery repository authority"))?;
    if target.get("kind").and_then(serde_json::Value::as_str) == Some("api_only") {
        if commit.is_some() {
            return Err(DurableTaskRefusal::InvalidTarget.into());
        }
        return Ok((format!("api:{fleet}"), None));
    }
    let target: BoundPr =
        serde_json::from_value(target).map_err(af_core::json::Unreadable::from)?;
    if commit != Some(target.head.as_str()) {
        return Err(DurableTaskRefusal::InvalidTarget.into());
    }
    Ok((
        format!("github:{}", target.repository_id),
        commit.map(str::to_owned),
    ))
}
