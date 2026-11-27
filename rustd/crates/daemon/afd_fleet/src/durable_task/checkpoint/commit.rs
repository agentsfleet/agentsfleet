//! Accepted uploads and complete pages precede the single recovery-pointer transition.

use super::{
    guard,
    page::Page,
    row::{Pin, Plan},
};
use crate::{
    Result,
    durable_task::{
        DurableTaskRefusal, DurableTaskRun, DurableTasks, guard as durable_task_guard,
        row::{counter, small_counter, sql_counter},
        transaction,
    },
    error::query,
};
use af_core::id::Uuid7;
use af_wire::durable_task::{
    CheckpointCommitInput, CheckpointMode, CheckpointPage, CheckpointPlan,
    CheckpointTransferPurpose,
};
use futures_util::TryStreamExt as _;
use sqlx::PgConnection;

struct Manifest {
    pages: Vec<(String, String)>,
    history: Vec<Uuid7>,
}

#[derive(sqlx::FromRow)]
struct History {
    id: Uuid7,
    plaintext_digest: String,
    plaintext_size: i64,
}

pub(in crate::durable_task) async fn publish(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    checkpoint: &Uuid7,
    input: &garde::Valid<CheckpointCommitInput>,
) -> Result<CheckpointPlan> {
    let mut connection = transaction::acquire(tasks.runs.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let current = durable_task_guard::run(&mut tx, run).await?;
    let task = current
        .durable_task_id
        .clone()
        .filter(|task| *task == input.durable_task.durable_task_id)
        .ok_or(DurableTaskRefusal::NotFound)?;
    let previous = Plan::load(&mut tx, checkpoint, &task).await?;
    if previous.ready {
        if previous.manifest_root.as_deref() != Some(input.manifest_root.as_str())
            || previous.history_digest.as_deref() != Some(input.history_digest.as_str())
            || counter(previous.covered_input_sequence, "checkpoint input cursor")?
                != input.covered_input_sequence
            || counter(previous.effect_watermark, "checkpoint effect cursor")?
                != input.effect_watermark
        {
            return Err(DurableTaskRefusal::ReplayConflict.into());
        }
        let pin: Pin = sqlx::query_as(include_str!("sql/pin.sql"))
            .bind(durable_task_guard::wire(&input.pin_id)?)
            .bind(checkpoint)
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
        run,
        &input.durable_task,
        checkpoint,
        &durable_task_guard::id(&input.pin_id, "pin_id")?,
        CheckpointTransferPurpose::Upload,
    )
    .await?;
    // A delta publishes only over the generation it was planned against.
    let parent = plan.parent_id.as_ref().map(Uuid7::as_str);
    if input.covered_input_sequence != owned.snapshot.context.covered_input_sequence
        || input.effect_watermark > owned.snapshot.context.confirmed_effect_watermark
        || parent.is_some() && owned.snapshot.context.checkpoint_id.as_deref() != parent
    {
        return Err(DurableTaskRefusal::StaleState.into());
    }
    accept(&mut tx, &plan, &input.uploaded).await?;
    verify_manifest(&mut tx, &plan, input).await?;
    sqlx::query(include_str!("sql/plan_ready.sql"))
        .bind(&plan.id)
        .bind(&input.manifest_root)
        .bind(&input.history_digest)
        .bind(sql_counter(input.covered_input_sequence, "input cursor")?)
        .bind(sql_counter(input.effect_watermark, "effect cursor")?)
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
        sqlx::query(include_str!("sql/durable_task_checkpoint.sql"))
            .bind(&plan.durable_task_id)
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
    let plan = Plan::load(&mut tx, checkpoint, &task).await?;
    let response = plan.wire(&pin, false)?;
    tx.commit()
        .await
        .map_err(query("commit saved generation and durable references"))?;
    transaction::recycle(connection);
    if plan.mode()? == CheckpointMode::Incremental {
        tasks
            .publish_changed(
                &owned.scope,
                &plan.durable_task_id,
                owned.snapshot.resource_version.saturating_add(1),
            )
            .await;
    }
    Ok(response)
}

/// Records the uploads the runner saw accepted, under the workspace's storage
/// lock, before the manifest's completeness is proved.
async fn accept(connection: &mut PgConnection, plan: &Plan, uploaded: &[String]) -> Result<()> {
    let uploaded = uploaded
        .iter()
        .map(|id| durable_task_guard::id(id, "uploaded"))
        .collect::<Result<Vec<_>>>()?;
    super::lock_storage(connection, &plan.workspace_id).await?;
    super::accept(
        connection,
        &plan.workspace_id,
        (&plan.id, &plan.owner_run_id, plan.owner_fence),
        &uploaded,
    )
    .await
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
        return Err(DurableTaskRefusal::CheckpointIncompatible.into());
    }
    let history = history(connection, plan, &manifest.history).await?;
    if history != input.history_digest {
        return Err(DurableTaskRefusal::CheckpointIncompatible.into());
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
        return Err(DurableTaskRefusal::CheckpointIncompatible.into());
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
            return Err(DurableTaskRefusal::CheckpointIncompatible.into());
        }
    }
    if plan.parent_id.is_some() {
        super::chain::pages(connection, &plan.id, |page| tree.ancestor(page)).await?;
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
        small_counter(plan.entry_count, "checkpoint entry_count")?,
        counter(plan.logical_bytes, "checkpoint logical_bytes")?,
    )?;
    Ok(manifest)
}

async fn scan_pages(
    connection: &mut PgConnection,
    plan: &Plan,
    base: bool,
    ids: &[Uuid7],
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
            return Err(DurableTaskRefusal::CheckpointIncompatible.into());
        }
        let bytes = page
            .body
            .ok_or(DurableTaskRefusal::CheckpointIncompatible)?;
        *manifest_bytes = manifest_bytes
            .checked_add(bytes.len())
            .ok_or(DurableTaskRefusal::LimitExhausted)?;
        let digest = page
            .digest
            .ok_or(DurableTaskRefusal::CheckpointIncompatible)?;
        if af_core::etag::sha256_hex(&bytes) != digest {
            return Err(DurableTaskRefusal::CheckpointIncompatible.into());
        }
        let contents: CheckpointPage =
            serde_json::from_slice(&bytes).map_err(af_core::json::Unreadable::from)?;
        for entry in contents.entries {
            if last_path
                .as_deref()
                .is_some_and(|last| last >= entry.path())
            {
                return Err(DurableTaskRefusal::CheckpointIncompatible.into());
            }
            last_path = Some(entry.path().to_owned());
            tree.apply(entry)?;
        }
        manifest.pages.push((page.id.into(), digest));
        let history = contents
            .history_objects
            .iter()
            .map(|object| Uuid7::parse(object))
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|_text| DurableTaskRefusal::CheckpointIncompatible)?;
        manifest.history.extend(history);
    }
    if manifest.pages.len() != ids.len() {
        return Err(DurableTaskRefusal::CheckpointIncompatible.into());
    }
    Ok(manifest)
}

async fn history(connection: &mut PgConnection, plan: &Plan, ids: &[Uuid7]) -> Result<String> {
    let rows: Vec<History> = sqlx::query_as(include_str!("sql/history.sql"))
        .bind(ids)
        .bind(&plan.workspace_id)
        .fetch_all(connection)
        .await
        .map_err(query("verify exact ordered history chunk identities"))?;
    if rows.len() != ids.len() {
        return Err(DurableTaskRefusal::CheckpointIncompatible.into());
    }
    let bytes = rows
        .iter()
        .try_fold(0_i64, |sum, row| sum.checked_add(row.plaintext_size))
        .ok_or(DurableTaskRefusal::LimitExhausted)?;
    if bytes > 64 * 1024 * 1024 {
        return Err(DurableTaskRefusal::LimitExhausted.into());
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
