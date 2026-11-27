//! The retained repository base a task plan overlays: an exact one named,
//! one reused for the same commit, or a new one allocated.

use crate::{
    Result,
    durable_task::{
        DurableTaskRefusal, DurableTasks,
        row::{sql_counter, sql_small_counter},
    },
    error::query,
};
use af_core::clock;
use af_core::id::Uuid7;
use af_wire::durable_task::{CheckpointMode, CheckpointPlanInput};
use sqlx::PgConnection;

/// A retained base as a plan reads it.
#[derive(sqlx::FromRow)]
pub(super) struct Base {
    pub(super) id: Uuid7,
    pub(super) page_ids: Vec<Uuid7>,
    pub(super) manifest_root: Option<String>,
    pub(super) logical_bytes: i64,
    pub(super) entry_count: i32,
    pub(super) ready: bool,
}

/// The retained base `input` overlays in `workspace`'s `repository`, when it
/// names or needs one; a new base's pages are allocated into `pages`.
///
/// # Errors
/// The base named is not the exact ready one, or the mode needs another.
pub(super) async fn select(
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
