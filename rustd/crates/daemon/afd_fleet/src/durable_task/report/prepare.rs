//! Network observations finish before the report's financial transaction starts.

use super::PreparedReport;
use crate::{
    Result,
    durable_task::{
        DurableTaskRun, DurableTasks, continuation::Continuation, decision, execution, guard,
        transaction,
    },
    error::query,
};
use af_wire::durable_task::{DurableTaskDisposition, DurableTaskReport, DurableTaskTarget};

pub(super) async fn prepare(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    input: &DurableTaskReport,
) -> Result<Option<PreparedReport>> {
    if garde::Validate::validate(input).is_err() {
        return Ok(None);
    }
    let prepared = snapshot(tasks, run, input).await;
    let (owned, source, target) = match prepared {
        Ok(value) => value,
        Err(fault) if fault.is_datastore_unavailable() => return Err(fault),
        Err(fault) => {
            tracing::warn!(event = "dt_report_evidence_refused", run_id = run.run_id.as_str(), error_code = %fault.code());
            return Ok(None);
        }
    };
    let dependency = if matches!(
        input.disposition,
        DurableTaskDisposition::Waiting | DurableTaskDisposition::Complete
    ) {
        match &target {
            Some(target) => {
                let Some(github) = &tasks.dependencies.github else {
                    return Ok(None);
                };
                let session = match github.read_session(run, &target.repository).await {
                    Ok(session) => session,
                    Err(fault) => {
                        tracing::warn!(event = "dt_report_read_authority_refused", run_id = run.run_id.as_str(), error_code = %fault.code());
                        return Ok(None);
                    }
                };
                match session
                    .with_deadline(
                        owned
                            .current
                            .run_expires_at
                            .min(owned.current.thread_expires_at),
                    )
                    .status(target)
                    .await
                {
                    Ok(evidence) => Some(evidence),
                    Err(fault) => {
                        tracing::warn!(event = "dt_report_dependency_unavailable", run_id = run.run_id.as_str(), error_code = %fault.code());
                        return Ok(None);
                    }
                }
            }
            None if input.disposition == DurableTaskDisposition::Waiting => return Ok(None),
            None => None,
        }
    } else {
        None
    };
    let needs_continuation = input.disposition == DurableTaskDisposition::Continue
        || input.disposition == DurableTaskDisposition::Waiting
            && dependency
                .as_ref()
                .zip(target.as_ref())
                .is_some_and(|(status, target)| status.checks_finished(target));
    // `execution::load` matched this id to the run's own task, so it parses.
    let durable_task_id = guard::id(&input.durable_task.durable_task_id, "durable_task_id")?;
    let wake = if needs_continuation {
        Some(Continuation::new(
            &owned.scope,
            &durable_task_id,
            owned
                .snapshot
                .context
                .generation
                .checked_add(1)
                .ok_or(crate::durable_task::DurableTaskRefusal::LimitExhausted)?,
            &source,
        ))
    } else {
        None
    };
    Ok(Some(PreparedReport {
        tasks: tasks.clone(),
        run_id: run.run_id.clone(),
        runner_id: run.runner_id.clone(),
        durable_task_id,
        scope: owned.scope,
        snapshot: owned.snapshot,
        input: input.clone(),
        target,
        dependency,
        wake,
    }))
}

async fn snapshot(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    input: &DurableTaskReport,
) -> Result<(
    execution::Owned,
    af_core::id::Uuid7,
    Option<crate::durable_task::github::BoundPr>,
)> {
    let mut connection = transaction::acquire(tasks.runs.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let owned = execution::load(&mut tx, run, &input.durable_task).await?;
    decision::goal(&mut tx, run, input, &owned.snapshot.context).await?;
    let source = owned.current.source_turn_id.clone();
    let target = if matches!(owned.snapshot.context.target, DurableTaskTarget::ApiOnly) {
        None
    } else {
        let target: sqlx::types::Json<crate::durable_task::github::BoundPr> =
            sqlx::query_scalar(include_str!("../sql/native_target.sql"))
                .bind(guard::wire(&input.durable_task.durable_task_id)?)
                .bind(&owned.scope.fleet_id)
                .bind(&owned.scope.workspace_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(query("read native report target"))?;
        let binding = crate::run::Runs::approved_binding_on(&mut tx, &owned.scope.fleet_id).await?;
        target.within(binding.as_ref())?;
        crate::durable_task::Github::hold_authority(&mut tx, &owned.scope, target.installation_id)
            .await?;
        Some(target.0)
    };
    tx.commit()
        .await
        .map_err(query("finish native report preparation"))?;
    transaction::recycle(connection);
    Ok((owned, source, target))
}
