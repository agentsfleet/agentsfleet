//! Complete page verification precedes the single recovery-pointer transition.

use super::{
    guard,
    page::Page,
    row::{Pin, Plan},
};
use crate::{
    Result,
    error::query,
    task::{TaskLease, TaskRefusal, Tasks, guard as task_guard, row::counter, transaction},
};
use af_core::id::Uuid7;
use af_wire::task::{
    CheckpointCommitInput, CheckpointMode, CheckpointPage, CheckpointPlan,
    CheckpointTransferPurpose,
};
use futures_util::TryStreamExt as _;
use sqlx::PgConnection;

struct Manifest {
    pages: Vec<(String, String)>,
    history: Vec<String>,
}

#[derive(sqlx::FromRow)]
struct History {
    id: String,
    plaintext_digest: String,
    plaintext_size: i64,
}

pub(in crate::task) async fn publish(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    checkpoint: &Uuid7,
    input: &garde::Valid<CheckpointCommitInput>,
) -> Result<CheckpointPlan> {
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let current = task_guard::lease(&mut tx, lease).await?;
    if current.task_id.as_deref() != Some(input.task.task_id.as_str()) {
        return Err(TaskRefusal::NotFound.into());
    }
    let previous = Plan::load(&mut tx, checkpoint, &input.task.task_id).await?;
    if previous.ready {
        if previous.manifest_root.as_deref() != Some(input.manifest_root.as_str())
            || previous.history_digest.as_deref() != Some(input.history_digest.as_str())
            || counter(previous.covered_input_sequence, "checkpoint input cursor")?
                != input.covered_input_sequence
            || counter(previous.effect_watermark, "checkpoint effect cursor")?
                != input.effect_watermark
        {
            return Err(TaskRefusal::ReplayConflict.into());
        }
        let pin: Pin = sqlx::query_as(include_str!("sql/pin.sql"))
            .bind(&input.pin_id)
            .bind(checkpoint.as_str())
            .fetch_one(&mut *tx)
            .await
            .map_err(query("read committed checkpoint pin receipt"))?;
        let response = previous.wire(&pin, true)?;
        tx.commit()
            .await
            .map_err(query("read committed checkpoint replay"))?;
        transaction::recycle(connection);
        return Ok(response);
    }
    let (owned, plan, pin) = guard::owned(
        &mut tx,
        lease,
        &input.task,
        checkpoint,
        &task_guard::id(&input.pin_id, "pin_id")?,
        CheckpointTransferPurpose::Upload,
    )
    .await?;
    if input.covered_input_sequence != owned.snapshot.context.covered_input_sequence
        || input.effect_watermark > owned.snapshot.context.confirmed_effect_watermark
    {
        return Err(TaskRefusal::StaleState.into());
    }
    verify_manifest(&mut tx, &plan, input).await?;
    sqlx::query(include_str!("sql/plan_ready.sql"))
        .bind(&plan.id)
        .bind(&input.manifest_root)
        .bind(&input.history_digest)
        .bind(
            i64::try_from(input.covered_input_sequence)
                .map_err(|_overflow| TaskRefusal::StaleState)?,
        )
        .bind(i64::try_from(input.effect_watermark).map_err(|_overflow| TaskRefusal::StaleState)?)
        .execute(&mut *tx)
        .await
        .map_err(query("publish complete verified checkpoint manifest"))?;
    if plan.mode()? == CheckpointMode::RetainBase {
        sqlx::query(include_str!("sql/base_ready.sql"))
            .bind(&plan.base_id)
            .bind(&input.manifest_root)
            .execute(&mut *tx)
            .await
            .map_err(query("retain independently owned verified repository base"))?;
    } else {
        sqlx::query(include_str!("sql/task_checkpoint.sql"))
            .bind(&plan.task_id)
            .bind(&plan.id)
            .execute(&mut *tx)
            .await
            .map_err(query("advance the task's recovery pointer once"))?;
    }
    sqlx::query(include_str!("sql/pin_release.sql"))
        .bind(&pin.id)
        .execute(&mut *tx)
        .await
        .map_err(query(
            "convert live writer pin to durable manifest references",
        ))?;
    let plan = Plan::load(&mut tx, checkpoint, &input.task.task_id).await?;
    let response = plan.wire(&pin, false)?;
    tx.commit()
        .await
        .map_err(query("commit saved generation and durable references"))?;
    transaction::recycle(connection);
    if plan.mode()? == CheckpointMode::Incremental {
        tasks
            .publish_changed(
                &owned.scope,
                &task_guard::id(&plan.task_id, "task_id")?,
                owned.snapshot.resource_version.saturating_add(1),
            )
            .await;
    }
    Ok(response)
}

async fn verify_manifest(
    connection: &mut PgConnection,
    plan: &Plan,
    input: &CheckpointCommitInput,
) -> Result<()> {
    let manifest = scan(connection, plan).await?;
    if af_crypto::checkpoint_digest::manifest_root(
        manifest
            .pages
            .iter()
            .map(|(id, digest)| (id.as_str(), digest.as_str())),
    ) != input.manifest_root
    {
        return Err(TaskRefusal::CheckpointIncompatible.into());
    }
    let history = history(connection, plan, &manifest.history).await?;
    if history != input.history_digest {
        return Err(TaskRefusal::CheckpointIncompatible.into());
    }
    let complete: bool = sqlx::query_scalar(include_str!("sql/objects_complete.sql"))
        .bind(&plan.id)
        .bind(&plan.base_id)
        .bind(plan.mode()? == CheckpointMode::RetainBase)
        .bind(&plan.workspace_id)
        .fetch_one(&mut *connection)
        .await
        .map_err(query(
            "prove every referenced object verified and not tombstoned",
        ))?;
    if !complete {
        return Err(TaskRefusal::CheckpointIncompatible.into());
    }
    Ok(())
}

async fn scan(connection: &mut PgConnection, plan: &Plan) -> Result<Manifest> {
    let mut tree = super::effective::Tree::default();
    let mut manifest_bytes = 0;
    if plan.mode()? == CheckpointMode::Incremental && plan.base_id.is_some() {
        let base = scan_pages(
            connection,
            plan,
            true,
            &plan.base_page_ids,
            &mut tree,
            &mut manifest_bytes,
        )
        .await?;
        if Some(af_crypto::checkpoint_digest::manifest_root(
            base.pages
                .iter()
                .map(|(id, digest)| (id.as_str(), digest.as_str())),
        )) != plan.base_manifest_root
        {
            return Err(TaskRefusal::CheckpointIncompatible.into());
        }
    }
    let manifest = scan_pages(
        connection,
        plan,
        plan.mode()? == CheckpointMode::RetainBase,
        &plan.page_ids,
        &mut tree,
        &mut manifest_bytes,
    )
    .await?;
    tree.verify(
        u32::try_from(plan.entry_count).map_err(|_overflow| TaskRefusal::CheckpointIncompatible)?,
        u64::try_from(plan.logical_bytes)
            .map_err(|_overflow| TaskRefusal::CheckpointIncompatible)?,
    )?;
    Ok(manifest)
}

async fn scan_pages(
    connection: &mut PgConnection,
    plan: &Plan,
    base: bool,
    ids: &[String],
    tree: &mut super::effective::Tree,
    manifest_bytes: &mut usize,
) -> Result<Manifest> {
    let mut rows = sqlx::query_as::<_, Page>(include_str!("sql/pages_complete.sql"))
        .bind(&plan.id)
        .bind(&plan.base_id)
        .bind(base)
        .fetch(connection);
    let mut manifest = Manifest {
        pages: Vec::with_capacity(ids.len()),
        history: Vec::new(),
    };
    let mut last_path: Option<String> = None;
    while let Some(page) = rows
        .try_next()
        .await
        .map_err(query("stream ordered complete manifest pages"))?
    {
        let position = manifest.pages.len();
        if ids.get(position) != Some(&page.id)
            || usize::try_from(page.number).ok() != Some(position)
        {
            return Err(TaskRefusal::CheckpointIncompatible.into());
        }
        let bytes = page.body.ok_or(TaskRefusal::CheckpointIncompatible)?;
        *manifest_bytes = manifest_bytes
            .checked_add(bytes.len())
            .ok_or(TaskRefusal::LimitExhausted)?;
        if *manifest_bytes
            > af_wire::task::CHECKPOINT_PAGE_BYTES
                * usize::try_from(af_wire::task::CHECKPOINT_PAGES)
                    .map_err(|_overflow| TaskRefusal::LimitExhausted)?
        {
            return Err(TaskRefusal::LimitExhausted.into());
        }
        let digest = page.digest.ok_or(TaskRefusal::CheckpointIncompatible)?;
        if af_core::etag::sha256_hex(&bytes) != digest {
            return Err(TaskRefusal::CheckpointIncompatible.into());
        }
        let contents: CheckpointPage =
            serde_json::from_slice(&bytes).map_err(af_core::json::Unreadable::from)?;
        for entry in contents.entries {
            if last_path
                .as_deref()
                .is_some_and(|last| last >= entry.path())
            {
                return Err(TaskRefusal::CheckpointIncompatible.into());
            }
            last_path = Some(entry.path().to_owned());
            tree.apply(entry)?;
        }
        manifest.pages.push((page.id, digest));
        manifest.history.extend(contents.history_objects);
    }
    if manifest.pages.len() != ids.len() {
        return Err(TaskRefusal::CheckpointIncompatible.into());
    }
    Ok(manifest)
}

async fn history(connection: &mut PgConnection, plan: &Plan, ids: &[String]) -> Result<String> {
    let rows: Vec<History> = sqlx::query_as(include_str!("sql/history.sql"))
        .bind(ids)
        .bind(&plan.workspace_id)
        .fetch_all(connection)
        .await
        .map_err(query("verify exact ordered history chunk identities"))?;
    if rows.len() != ids.len() {
        return Err(TaskRefusal::CheckpointIncompatible.into());
    }
    let bytes = rows
        .iter()
        .try_fold(0_i64, |sum, row| sum.checked_add(row.plaintext_size))
        .ok_or(TaskRefusal::LimitExhausted)?;
    if bytes > 64 * 1024 * 1024 {
        return Err(TaskRefusal::LimitExhausted.into());
    }
    let chunks = rows
        .iter()
        .map(|row| {
            Ok((
                row.id.as_str(),
                row.plaintext_digest.as_str(),
                counter(row.plaintext_size, "checkpoint history plaintext size")?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(af_crypto::checkpoint_digest::history_root(chunks))
}
