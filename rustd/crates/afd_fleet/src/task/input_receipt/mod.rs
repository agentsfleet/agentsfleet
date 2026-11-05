//! Native acknowledgement never changes accepted input meaning or task authority.

mod answer;
mod row;

use afd_wire::task::{TaskInputMode, TaskInputReceiptInput, TaskInputReceiptResult};
use super::{TaskLease, TaskRefusal, Tasks, execution, guard, transaction};
use crate::{Result, error::query};

pub(super) async fn accept(tasks: &Tasks, lease: TaskLease<'_>, input: &garde::Valid<TaskInputReceiptInput>) -> Result<TaskInputReceiptResult> {
    let digest = afd_core::etag::sha256_hex(&serde_json::to_vec(&(&input.source_admission_id, &input.plan_digest,
        &input.item_id, &input.task, input.mode, &input.response_text)).map_err(afd_core::json::Unreadable::from)?);
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let current = guard::lease(&mut tx, lease).await?;
    let item = row::item(&mut tx, &current, input).await?;
    if let Some(saved) = row::replay(&mut tx, &current, input).await? {
        if saved != digest { return Err(TaskRefusal::ReplayConflict.into()); }
        let result = row::result(input, &item, true)?;
        tx.commit().await.map_err(query("read committed input receipt"))?;
        transaction::recycle(connection);
        return Ok(result);
    }
    row::current(&item, input)?;
    let response = match input.mode {
        TaskInputMode::ApplyHere => {
            if input.response_text.is_some() { return Err(TaskRefusal::ReplayConflict.into()); }
            let version = input.task.as_ref().ok_or(TaskRefusal::InvalidTarget)?;
            execution::load(&mut tx, lease, version).await?;
            let sequence = item.sequence.ok_or(TaskRefusal::InvalidTarget)?;
            let applied = sqlx::query(include_str!("sql/apply.sql")).bind(&version.task_id).bind(sequence)
                .execute(&mut *tx).await.map_err(query("advance exactly one accepted input boundary"))?;
            if applied.rows_affected() != 1 { return Err(TaskRefusal::StaleState.into()); }
            None
        }
        TaskInputMode::Queued => {
            if input.response_text.is_some() || item.task_id.is_none() { return Err(TaskRefusal::InvalidTarget.into()); }
            if item.state.as_deref().map(super::row::state).transpose()?.is_some_and(afd_wire::task::TaskState::is_terminal) {
                return Err(TaskRefusal::StaleState.into());
            }
            Some(format!("Queued: {}", item.title.as_deref().ok_or(TaskRefusal::NotFound)?))
        }
        TaskInputMode::Answered => Some(input.response_text.clone().ok_or(TaskRefusal::InputAmbiguous)?),
    };
    if response.is_some() && row::has_answer(&mut tx, input).await? { return Err(TaskRefusal::ReplayConflict.into()); }
    sqlx::query(include_str!("sql/insert.sql")).bind(&input.item_id).bind(&current.fleet_id).bind(&current.workspace_id)
        .bind(input.mode.as_str()).bind(&input.operation_id).bind(lease.lease_id.as_str()).bind(current.fence)
        .bind(digest).bind(&response).execute(&mut *tx).await.map_err(query("retain immutable input outcome"))?;
    if response.is_some() {
        sqlx::query(include_str!("sql/answered.sql")).bind(&input.item_id).execute(&mut *tx).await
            .map_err(query("retain fulfilled input reply obligation"))?;
    }
    let completion = if input.source_admission_id != current.source_admission_id && response.is_some() {
        answer::finish(tasks, &mut tx, &current, input).await?
    } else { None };
    let item = row::item(&mut tx, &current, input).await?;
    let result = row::result(input, &item, false)?;
    tx.commit().await.map_err(query("commit accepted input and reply obligation"))?;
    transaction::recycle(connection);
    if let Some(completion) = completion { completion.announce(tasks).await; }
    if let Some(task) = item.task_id {
        let scope = super::TaskScope { fleet_id: guard::id(&current.fleet_id, "fleet_id")?, workspace_id: guard::id(&current.workspace_id, "workspace_id")? };
        let id = guard::id(&task, "task_id")?;
        if let Ok(snapshot) = super::read::read(tasks, &scope, &id).await { tasks.publish_changed(&scope, &id, snapshot.resource_version).await; }
    }
    Ok(result)
}
