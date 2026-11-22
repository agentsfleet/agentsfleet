//! Early completions are durable; duplicate signed payloads cannot create a wake.

use super::{CompletionScope, VerifiedCompletion};
use crate::{Result, durable_task::DurableTasks, error::query};

pub(in crate::durable_task) async fn record(
    tasks: &DurableTasks,
    input: &VerifiedCompletion,
) -> Result<u64> {
    let Ok(input) = garde::Unvalidated::new(input.clone()).validate() else {
        tracing::debug!(event = "dt_completion_ignored", reason = "payload_bounds");
        return Ok(0);
    };
    let (workspace, fleet) = match &input.scope {
        CompletionScope::Workspace(workspace) => (workspace, None),
        CompletionScope::Fleet(scope) => (&scope.workspace_id, Some(&scope.fleet_id)),
    };
    let id = tasks.runs.entropy().uuid7(af_core::clock::now())?;
    let mut connection = tasks.runs.pool().acquire().await?;
    let inserted: i64 = sqlx::query_scalar(include_str!("sql/completion.sql"))
        .bind(id)
        .bind(workspace)
        .bind(fleet)
        .bind(
            i64::try_from(input.github_installation_id)
                .map_err(crate::error::durable_task_target_counter("installation_id"))?,
        )
        .bind(
            i64::try_from(input.github_repository_id)
                .map_err(crate::error::durable_task_target_counter("repository_id"))?,
        )
        .bind(&input.head)
        .bind(
            i64::try_from(input.github_workflow_id)
                .map_err(crate::error::durable_task_target_counter("workflow_id"))?,
        )
        .bind(
            i64::try_from(input.github_run_id)
                .map_err(crate::error::durable_task_target_counter("run_id"))?,
        )
        .bind(
            i32::try_from(input.run_attempt)
                .map_err(crate::error::durable_task_target_counter("run_attempt"))?,
        )
        .bind("installation_id")
        .bind("repository_id")
        .bind("head")
        .bind(
            i64::try_from(input.run_number)
                .map_err(crate::error::durable_task_target_counter("run_number"))?,
        )
        .fetch_one(&mut *connection)
        .await
        .map_err(query(
            "retain verified completion and its bounded fanout cursor",
        ))?;
    tracing::info!(event = "dt_completion_retained", inserted);
    Ok(u64::try_from(inserted).unwrap_or(0))
}
