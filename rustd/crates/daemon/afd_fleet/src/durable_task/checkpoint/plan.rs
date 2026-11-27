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

#[derive(sqlx::FromRow)]
struct Base {
    id: Uuid7,
    page_ids: Vec<Uuid7>,
    manifest_root: Option<String>,
    logical_bytes: i64,
    entry_count: i32,
    ready: bool,
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

fn request_digest(input: &CheckpointPlanInput) -> Result<String> {
    Ok(af_core::etag::sha256_hex(
        &serde_json::to_vec(&(
            &input.durable_task,
            &input.operation_id,
            input.mode,
            &input.base_id,
            &input.commit_oid,
            input.format_version,
            &input.toolbox_digest,
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
    let base = base(
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

async fn base(
    tasks: &DurableTasks,
    connection: &mut PgConnection,
    input: &CheckpointPlanInput,
    workspace: &Uuid7,
    repository: &str,
    commit: Option<&str>,
    pages: &mut Vec<Uuid7>,
) -> Result<Option<Base>> {
    if let Some(base) = &input.base_id {
        // The retained base can precede publication; the checkpoint stores the
        // current head separately and report/checkpoint.sql validates that head.
        let base: Base = sqlx::query_as(include_str!("sql/base_selected.sql"))
            .bind(crate::durable_task::guard::wire(base)?)
            .bind(workspace)
            .bind(repository)
            .bind(sql_small_counter(
                input.format_version,
                "checkpoint format_version",
            )?)
            .bind(&input.toolbox_digest)
            .fetch_optional(connection)
            .await
            .map_err(query("prove exact retained repository base"))?
            .ok_or(DurableTaskRefusal::CheckpointIncompatible)?;
        if input.mode != CheckpointMode::Incremental || !base.ready {
            return Err(DurableTaskRefusal::CheckpointIncompatible.into());
        }
        return Ok(Some(base));
    }
    if commit.is_none() {
        if input.mode != CheckpointMode::Incremental {
            return Err(DurableTaskRefusal::CheckpointIncompatible.into());
        }
        return Ok(None);
    }
    if input.mode != CheckpointMode::RetainBase {
        return Err(DurableTaskRefusal::CheckpointIncompatible.into());
    }
    let existing: Option<Base> = sqlx::query_as(include_str!("sql/base_existing.sql"))
        .bind(workspace)
        .bind(repository)
        .bind(commit)
        .bind(sql_small_counter(
            input.format_version,
            "checkpoint format_version",
        )?)
        .bind(&input.toolbox_digest)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query("read reusable exact repository base"))?;
    if let Some(base) = existing {
        if !base.ready {
            return Err(DurableTaskRefusal::StorageUnavailable.into());
        }
        // Checkout metadata may differ despite an identical Git commit. Reuse
        // the immutable base dimensions; the next overlay describes this tree.
        pages.clone_from(&base.page_ids);
        return Ok(Some(base));
    }
    let id = tasks.runs.entropy().uuid7(clock::now())?;
    sqlx::query(include_str!("sql/base_insert.sql"))
        .bind(&id)
        .bind(workspace)
        .bind(repository)
        .bind(commit)
        .bind(sql_small_counter(
            input.format_version,
            "checkpoint format_version",
        )?)
        .bind(&input.toolbox_digest)
        .bind(&*pages)
        .bind(sql_counter(
            input.logical_bytes,
            "checkpoint logical_bytes",
        )?)
        .bind(sql_small_counter(
            input.entry_count,
            "checkpoint entry_count",
        )?)
        .execute(&mut *connection)
        .await
        .map_err(query("allocate retained base identity"))?;
    sqlx::query(include_str!("sql/base_pages_allocate.sql"))
        .bind(&id)
        .bind(&*pages)
        .execute(connection)
        .await
        .map_err(query("allocate immutable base page identities"))?;
    Ok(Some(Base {
        id: id.clone(),
        page_ids: pages.clone(),
        manifest_root: None,
        logical_bytes: sql_counter(input.logical_bytes, "checkpoint logical_bytes")?,
        entry_count: sql_small_counter(input.entry_count, "checkpoint entry_count")?,
        ready: false,
    }))
}
