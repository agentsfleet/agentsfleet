//! Native acknowledgement never changes accepted input meaning or task authority.

mod answer;
mod row;

use super::{DurableTaskRefusal, DurableTaskRun, DurableTasks, execution, guard, transaction};
use crate::{Result, error::query};
use af_core::id::Uuid7;
use af_wire::durable_task::{
    DurableTaskInputMode, DurableTaskInputReceiptInput, DurableTaskInputReceiptResult,
};

pub(super) async fn accept(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    input: &garde::Valid<DurableTaskInputReceiptInput>,
) -> Result<DurableTaskInputReceiptResult> {
    let digest = af_core::etag::sha256_hex(
        &serde_json::to_vec(&(
            &input.source_turn_id,
            &input.plan_digest,
            &input.item_id,
            &input.durable_task,
            input.mode,
            &input.response_text,
        ))
        .map_err(af_core::json::Unreadable::from)?,
    );
    let mut connection = transaction::acquire(tasks.runs.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let current = guard::run(&mut tx, run).await?;
    let item = row::item(&mut tx, &current, input).await?;
    if let Some(saved) = row::replay(&mut tx, &current, input).await? {
        if saved != digest {
            return Err(DurableTaskRefusal::ReplayConflict.into());
        }
        let result = row::result(input, &item, true)?;
        tx.commit()
            .await
            .map_err(query("read committed input receipt"))?;
        transaction::recycle(connection);
        return Ok(result);
    }
    row::current(&item, input)?;
    let response = response(&mut tx, run, input, &item).await?;
    if response.is_some() && row::has_answer(&mut tx, input).await? {
        return Err(DurableTaskRefusal::ReplayConflict.into());
    }
    // An apply is the input's own receipt and carries no reply; a reply is the item's.
    let written = if input.mode == DurableTaskInputMode::ApplyHere {
        sqlx::query_scalar::<_, Uuid7>(include_str!("sql/applied.sql"))
            .bind(guard::wire(&input.item_id)?)
            .bind(&current.fleet_id)
            .bind(&current.workspace_id)
            .bind(guard::wire(&input.operation_id)?)
            .bind(run.run_id)
            .bind(current.fence)
            .bind(digest)
    } else {
        sqlx::query_scalar::<_, Uuid7>(include_str!("sql/insert.sql"))
            .bind(guard::wire(&input.item_id)?)
            .bind(&current.fleet_id)
            .bind(&current.workspace_id)
            .bind(input.mode.as_str())
            .bind(guard::wire(&input.operation_id)?)
            .bind(run.run_id)
            .bind(current.fence)
            .bind(digest)
            .bind(&response)
    };
    written
        .fetch_one(&mut *tx)
        .await
        .map_err(query("retain immutable input outcome"))?;
    let completion = if input.source_turn_id != current.source_turn_id && response.is_some() {
        answer::finish(tasks, &mut tx, &current, input).await?
    } else {
        None
    };
    let item = row::item(&mut tx, &current, input).await?;
    let result = row::result(input, &item, false)?;
    tx.commit()
        .await
        .map_err(query("commit accepted input and reply delivery"))?;
    transaction::recycle(connection);
    if let Some(completion) = completion {
        completion.announce(tasks).await;
    }
    if let Some(task) = item.durable_task_id {
        let scope = super::DurableTaskScope {
            fleet_id: current.fleet_id.clone(),
            workspace_id: current.workspace_id.clone(),
        };
        let id = task.clone();
        if let Ok(snapshot) = super::read::read(tasks, &scope, &id).await {
            tasks
                .publish_changed(&scope, &id, snapshot.resource_version)
                .await;
        }
    }
    Ok(result)
}

async fn response(
    connection: &mut sqlx::PgConnection,
    run: DurableTaskRun<'_>,
    input: &DurableTaskInputReceiptInput,
    item: &row::Item,
) -> Result<Option<String>> {
    let response = match input.mode {
        DurableTaskInputMode::ApplyHere => {
            if input.response_text.is_some() {
                return Err(DurableTaskRefusal::ReplayConflict.into());
            }
            let version = input
                .durable_task
                .as_ref()
                .ok_or(DurableTaskRefusal::InvalidTarget)?;
            execution::load(connection, run, version).await?;
            let sequence = item.sequence.ok_or(DurableTaskRefusal::InvalidTarget)?;
            let applied = sqlx::query(include_str!("sql/apply.sql"))
                .bind(guard::wire(&version.durable_task_id)?)
                .bind(sequence)
                .execute(&mut *connection)
                .await
                .map_err(query("advance exactly one accepted input boundary"))?;
            if applied.rows_affected() != 1 {
                return Err(DurableTaskRefusal::StaleState.into());
            }
            None
        }
        DurableTaskInputMode::Queued => {
            if input.response_text.is_some() || item.durable_task_id.is_none() {
                return Err(DurableTaskRefusal::InvalidTarget.into());
            }
            if item
                .state
                .as_deref()
                .map(super::row::state)
                .transpose()?
                .is_some_and(af_wire::durable_task::DurableTaskState::is_terminal)
            {
                return Err(DurableTaskRefusal::StaleState.into());
            }
            Some(format!(
                "Queued: {}",
                item.title.as_deref().ok_or(DurableTaskRefusal::NotFound)?
            ))
        }
        DurableTaskInputMode::Answered => Some(
            input
                .response_text
                .clone()
                .ok_or(DurableTaskRefusal::InputAmbiguous)?,
        ),
    };
    Ok(response)
}
