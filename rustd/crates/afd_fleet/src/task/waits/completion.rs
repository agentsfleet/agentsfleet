//! Early completions are durable; duplicate signed payloads cannot create a wake.

use super::{CompletionScope, VerifiedCompletion};
use crate::{Result, error::query, task::Tasks};

pub(in crate::task) async fn record(tasks: &Tasks, input: &VerifiedCompletion) -> Result<u64> {
    let Ok(input) = garde::Unvalidated::new(input.clone()).validate() else {
        tracing::debug!(event = "dt_completion_ignored", reason = "payload_bounds");
        return Ok(0);
    };
    let (workspace, fleet) = match &input.scope {
        CompletionScope::Workspace(workspace) => (workspace, None),
        CompletionScope::Fleet(scope) => (&scope.workspace_id, Some(&scope.fleet_id)),
    };
    let id = tasks.leases.entropy().uuid7(afd_core::clock::now())?;
    let mut connection = tasks.leases.pool().acquire().await?;
    let inserted: i64 = sqlx::query_scalar(include_str!("sql/completion.sql"))
        .bind(id.as_str())
        .bind(workspace.as_str())
        .bind(fleet.map(afd_core::id::Uuid7::as_str))
        .bind(super::super::row::sql_counter(
            input.installation_id,
            "installation_id",
        )?)
        .bind(super::super::row::sql_counter(
            input.repository_id,
            "repository_id",
        )?)
        .bind(&input.head)
        .bind(super::super::row::sql_counter(
            input.workflow_id,
            "workflow_id",
        )?)
        .bind(super::super::row::sql_counter(input.run_id, "run_id")?)
        .bind(super::super::row::sql_small_counter(
            input.run_attempt,
            "run_attempt",
        )?)
        .bind(&input.conclusion)
        .bind(&input.body_digest)
        .bind("installation_id")
        .bind("repository_id")
        .bind("head")
        .bind(super::super::row::sql_counter(
            input.run_number,
            "run_number",
        )?)
        .fetch_one(&mut *connection)
        .await
        .map_err(query(
            "retain verified completion and its bounded fanout cursor",
        ))?;
    tracing::info!(event = "dt_completion_retained", inserted);
    Ok(u64::try_from(inserted).unwrap_or(0))
}
