//! Allocate a complete immutable capture plan before any transfer authorization.

use super::{
    guard, identity,
    row::{Pin, Plan},
};
use crate::{
    Result,
    error::query,
    task::{
        TaskLease, TaskRefusal, Tasks, execution,
        github::BoundPr,
        guard as task_guard,
        row::{sql_counter, sql_small_counter},
        transaction,
    },
};
use af_core::clock;
use af_wire::task::{CheckpointMode, CheckpointPlan, CheckpointPlanInput};
use sqlx::{PgConnection, types::Json};

#[derive(sqlx::FromRow)]
struct Base {
    id: String,
    page_ids: Vec<String>,
    manifest_root: Option<String>,
    logical_bytes: i64,
    entry_count: i32,
    ready: bool,
}

pub(in crate::task) async fn create(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    input: &garde::Valid<CheckpointPlanInput>,
) -> Result<CheckpointPlan> {
    // Replay and plan allocation need no remote round trip; upload activation
    // is proved before coding admission and again before the first signed PUT.
    let _configured = guard::storage(tasks)?;
    let digest = request_digest(input)?;
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let current = task_guard::lease(&mut tx, lease).await?;
    if current.task_id.as_deref() != Some(input.task.task_id.as_str()) {
        return Err(TaskRefusal::NotFound.into());
    }
    let existing: Option<String> = sqlx::query_scalar(include_str!("sql/plan_replay.sql"))
        .bind(&input.task.task_id)
        .bind(&input.operation_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(query("read immutable checkpoint plan replay"))?;
    if let Some(id) = existing {
        let plan = Plan::load(
            &mut tx,
            &task_guard::id(&id, "checkpoint_id")?,
            &input.task.task_id,
        )
        .await?;
        if plan.request_digest != digest {
            return Err(TaskRefusal::ReplayConflict.into());
        }
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
    let owned = execution::on_current(&mut tx, current, &input.task).await?;
    let (repository, commit) = target(
        &mut tx,
        &input.task.task_id,
        &owned.current.fleet_id,
        input.commit_oid.as_deref(),
    )
    .await?;
    if input.mode == CheckpointMode::RetainBase && input.base_id.is_some()
        || input.upload_budget_bytes
            > match input.mode {
                CheckpointMode::RetainBase => super::BASE_BYTES,
                CheckpointMode::Incremental => super::INCREMENTAL_BYTES,
            }
    {
        return Err(TaskRefusal::CheckpointIncompatible.into());
    }
    identity::quota(&mut tx, &owned.current.workspace_id).await?;
    sqlx::query(include_str!("sql/abandon_capture.sql"))
        .bind(&input.task.task_id)
        .bind(lease.lease_id.as_str())
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
        lease,
        input,
        &owned,
        &digest,
        (repository, commit),
    )
    .await?;
    let plan = Plan::load(&mut tx, &id, &input.task.task_id).await?;
    let pin = super::pins::insert(tasks, &mut tx, &owned, &plan, lease, "writer").await?;
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
            &input.task,
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
    tasks: &Tasks,
    connection: &mut PgConnection,
    lease: TaskLease<'_>,
    input: &CheckpointPlanInput,
    owned: &execution::Owned,
    digest: &str,
    repository_target: (String, Option<String>),
) -> Result<af_core::id::Uuid7> {
    let (repository, commit) = repository_target;
    let mut pages = (0..input.page_count)
        .map(|_page| {
            tasks
                .leases
                .entropy()
                .uuid7(clock::now())
                .map(|id| id.as_str().to_owned())
        })
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
    let id = tasks.leases.entropy().uuid7(clock::now())?;
    let ready_base = base
        .as_ref()
        .filter(|base| input.mode == CheckpointMode::RetainBase && base.ready);
    sqlx::query(include_str!("sql/plan_insert.sql"))
        .bind(id.as_str())
        .bind(&input.task.task_id)
        .bind(&owned.current.fleet_id)
        .bind(&owned.current.workspace_id)
        .bind(i64::try_from(input.task.generation).map_err(|_overflow| TaskRefusal::StaleState)?)
        .bind(owned.snapshot.context.accepted_revision_id.as_ref())
        .bind(
            i64::try_from(owned.snapshot.context.stop_generation)
                .map_err(|_overflow| TaskRefusal::StaleState)?,
        )
        .bind(&input.operation_id)
        .bind(digest)
        .bind(input.mode.as_str())
        .bind(repository)
        .bind(base.as_ref().map(|base| base.id.as_str()))
        .bind(commit)
        .bind(
            i32::try_from(input.format_version)
                .map_err(|_overflow| TaskRefusal::CheckpointIncompatible)?,
        )
        .bind(&input.toolbox_digest)
        .bind(i32::try_from(pages.len()).map_err(|_overflow| TaskRefusal::CheckpointIncompatible)?)
        .bind(&pages)
        .bind(
            i64::try_from(input.upload_budget_bytes)
                .map_err(|_overflow| TaskRefusal::LimitExhausted)?,
        )
        .bind(ready_base.map_or(
            i64::try_from(input.logical_bytes).map_err(|_overflow| TaskRefusal::LimitExhausted)?,
            |base| base.logical_bytes,
        ))
        .bind(ready_base.map_or(
            i32::try_from(input.entry_count).map_err(|_overflow| TaskRefusal::LimitExhausted)?,
            |base| base.entry_count,
        ))
        .bind(lease.lease_id.as_str())
        .bind(owned.current.fence)
        .bind(super::TRANSFER_SECONDS)
        .bind(ready_base.and_then(|base| base.manifest_root.as_deref()))
        .execute(&mut *connection)
        .await
        .map_err(query("persist native checkpoint plan"))?;
    if input.mode == CheckpointMode::Incremental {
        sqlx::query(include_str!("sql/pages_allocate.sql"))
            .bind(id.as_str())
            .bind(&pages)
            .execute(&mut *connection)
            .await
            .map_err(query("allocate immutable checkpoint page identities"))?;
    }
    Ok(id)
}

async fn target(
    connection: &mut PgConnection,
    task: &str,
    fleet: &str,
    commit: Option<&str>,
) -> Result<(String, Option<String>)> {
    let Json(target): Json<serde_json::Value> =
        sqlx::query_scalar(include_str!("sql/task_target.sql"))
            .bind(task)
            .fetch_one(connection)
            .await
            .map_err(query("read native recovery repository authority"))?;
    if target.get("kind").and_then(serde_json::Value::as_str) == Some("api_only") {
        if commit.is_some() {
            return Err(TaskRefusal::InvalidTarget.into());
        }
        return Ok((format!("api:{fleet}"), None));
    }
    let target: BoundPr =
        serde_json::from_value(target).map_err(af_core::json::Unreadable::from)?;
    if commit != Some(target.head.as_str()) {
        return Err(TaskRefusal::InvalidTarget.into());
    }
    Ok((
        format!("github:{}", target.repository_id),
        commit.map(str::to_owned),
    ))
}

async fn base(
    tasks: &Tasks,
    connection: &mut PgConnection,
    input: &CheckpointPlanInput,
    workspace: &str,
    repository: &str,
    commit: Option<&str>,
    pages: &mut Vec<String>,
) -> Result<Option<Base>> {
    if let Some(base) = &input.base_id {
        // The retained base can precede publication; the checkpoint stores the
        // current head separately and report/checkpoint.sql validates that head.
        let base: Base = sqlx::query_as(include_str!("sql/base_selected.sql"))
            .bind(base)
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
            .ok_or(TaskRefusal::CheckpointIncompatible)?;
        if input.mode != CheckpointMode::Incremental || !base.ready {
            return Err(TaskRefusal::CheckpointIncompatible.into());
        }
        return Ok(Some(base));
    }
    if commit.is_none() {
        if input.mode != CheckpointMode::Incremental {
            return Err(TaskRefusal::CheckpointIncompatible.into());
        }
        return Ok(None);
    }
    if input.mode != CheckpointMode::RetainBase {
        return Err(TaskRefusal::CheckpointIncompatible.into());
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
            return Err(TaskRefusal::StorageUnavailable.into());
        }
        // Checkout metadata may differ despite an identical Git commit. Reuse
        // the immutable base dimensions; the next overlay describes this tree.
        pages.clone_from(&base.page_ids);
        return Ok(Some(base));
    }
    let id = tasks.leases.entropy().uuid7(clock::now())?;
    sqlx::query(include_str!("sql/base_insert.sql"))
        .bind(id.as_str())
        .bind(workspace)
        .bind(repository)
        .bind(commit)
        .bind(sql_small_counter(
            input.format_version,
            "checkpoint format_version",
        )?)
        .bind(&input.toolbox_digest)
        .bind(sql_small_counter(
            input.page_count,
            "checkpoint page_count",
        )?)
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
        .bind(id.as_str())
        .bind(&*pages)
        .execute(connection)
        .await
        .map_err(query("allocate immutable base page identities"))?;
    Ok(Some(Base {
        id: id.as_str().to_owned(),
        page_ids: pages.clone(),
        manifest_root: None,
        logical_bytes: sql_counter(input.logical_bytes, "checkpoint logical_bytes")?,
        entry_count: sql_small_counter(input.entry_count, "checkpoint entry_count")?,
        ready: false,
    }))
}
