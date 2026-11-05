//! Network observations finish before the report's financial transaction starts.

use super::PreparedReport;
use crate::{
    Result,
    error::query,
    task::{TaskLease, Tasks, continuation::Continuation, decision, execution, guard, transaction},
};
use afd_wire::{
    report::ReportRequest,
    task::{TaskDisposition, TaskTarget},
};

pub(super) async fn prepare(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    request: &ReportRequest<'_>,
) -> Result<Option<PreparedReport>> {
    let Some(input) = &request.task else {
        return Ok(None);
    };
    if garde::Validate::validate(input).is_err() {
        return Ok(None);
    }
    let prepared = snapshot(tasks, lease, input).await;
    let (owned, source, target) = match prepared {
        Ok(value) => value,
        Err(fault) if fault.is_datastore_unavailable() => return Err(fault),
        Err(fault) => {
            tracing::warn!(event = "dt_report_evidence_refused", lease_id = lease.lease_id.as_str(), error_code = %fault.code());
            return Ok(None);
        }
    };
    let dependency = if matches!(
        input.disposition,
        TaskDisposition::Waiting | TaskDisposition::Complete
    ) {
        match &target {
            Some(target) => {
                let Some(github) = &tasks.dependencies.github else {
                    return Ok(None);
                };
                let session = match github.read_session(lease, &target.repository).await {
                    Ok(session) => session,
                    Err(fault) => {
                        tracing::warn!(event = "dt_report_read_authority_refused", lease_id = lease.lease_id.as_str(), error_code = %fault.code());
                        return Ok(None);
                    }
                };
                match session
                    .with_deadline(
                        owned
                            .current
                            .lease_expires_at
                            .min(owned.current.slot_expires_at),
                    )
                    .status(target)
                    .await
                {
                    Ok(evidence) => Some(evidence),
                    Err(fault) => {
                        tracing::warn!(event = "dt_report_dependency_unavailable", lease_id = lease.lease_id.as_str(), error_code = %fault.code());
                        return Ok(None);
                    }
                }
            }
            None if input.disposition == TaskDisposition::Waiting => return Ok(None),
            None => None,
        }
    } else {
        None
    };
    let wake = if input.disposition == TaskDisposition::Continue {
        Some(Continuation::new(
            &owned.scope,
            &guard::id(&input.task.task_id, "task_id")?,
            owned
                .snapshot
                .context
                .generation
                .checked_add(1)
                .ok_or(crate::task::TaskRefusal::LimitExhausted)?,
            &source,
        ))
    } else {
        None
    };
    Ok(Some(PreparedReport {
        tasks: tasks.clone(),
        lease_id: lease.lease_id.clone(),
        runner_id: lease.runner_id.to_string(),
        scope: owned.scope,
        snapshot: owned.snapshot,
        input: input.clone(),
        target,
        dependency,
        wake,
    }))
}

async fn snapshot(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    input: &afd_wire::task::TaskReport,
) -> Result<(
    execution::Owned,
    afd_core::id::Uuid7,
    Option<crate::task::github::BoundPr>,
)> {
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let owned = execution::load(&mut tx, lease, &input.task).await?;
    decision::goal(&mut tx, lease, input, &owned.snapshot.context).await?;
    let source = guard::id(&owned.current.source_admission_id, "source_admission_id")?;
    let target = if matches!(owned.snapshot.context.target, TaskTarget::ApiOnly) {
        None
    } else {
        let target: sqlx::types::Json<crate::task::github::BoundPr> =
            sqlx::query_scalar(include_str!("../sql/native_target.sql"))
                .bind(&input.task.task_id)
                .bind(owned.scope.fleet_id.as_str())
                .bind(owned.scope.workspace_id.as_str())
                .fetch_one(&mut *tx)
                .await
                .map_err(query("read native report target"))?;
        for revision in [
            Some(owned.snapshot.context.accepted_revision_id.as_ref()),
            None,
        ] {
            let config = crate::lease::Leases::stored_config_on(
                &mut tx,
                owned.scope.fleet_id.as_str(),
                revision,
            )
            .await?
            .ok_or(crate::task::TaskRefusal::InvalidTarget)?;
            target.within(&config)?;
        }
        crate::task::Github::hold_authority(&mut tx, &owned.scope, target.installation_id).await?;
        Some(target.0)
    };
    tx.commit()
        .await
        .map_err(query("finish native report preparation"))?;
    transaction::recycle(connection);
    Ok((owned, source, target))
}
