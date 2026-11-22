//! Native task, capture ownership and post-lock deadlines for every operation.

use super::row::{Pin, Plan};
use crate::{
    Result,
    durable_task::{DurableTaskRefusal, DurableTaskRun, DurableTasks, execution, row::sql_counter},
    error::query,
};
use af_core::{clock, id::Uuid7};
use af_wire::durable_task::{CheckpointTransferPurpose, DecisionDurableTaskVersion};
use sqlx::PgConnection;

pub(super) fn storage(
    tasks: &DurableTasks,
) -> Result<&crate::durable_task::storage::R2CheckpointStore> {
    tasks
        .dependencies
        .storage
        .as_ref()
        .ok_or_else(|| DurableTaskRefusal::StorageUnavailable.into())
}

pub(super) async fn owned(
    connection: &mut PgConnection,
    run: DurableTaskRun<'_>,
    version: &DecisionDurableTaskVersion,
    checkpoint: &Uuid7,
    pin: &Uuid7,
    purpose: CheckpointTransferPurpose,
) -> Result<(execution::Owned, Plan, Pin)> {
    let owned = execution::load(connection, run, version).await?;
    with_execution(connection, run, owned, checkpoint, pin, purpose).await
}

pub(super) async fn with_execution(
    connection: &mut PgConnection,
    run: DurableTaskRun<'_>,
    owned: execution::Owned,
    checkpoint: &Uuid7,
    pin: &Uuid7,
    purpose: CheckpointTransferPurpose,
) -> Result<(execution::Owned, Plan, Pin)> {
    let plan = Plan::load(
        connection,
        checkpoint,
        owned
            .current
            .durable_task_id
            .as_ref()
            .ok_or(DurableTaskRefusal::NotFound)?,
    )
    .await?;
    let pin: Pin = sqlx::query_as(include_str!("sql/pin.sql"))
        .bind(pin)
        .bind(checkpoint)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query("lock checkpoint pin"))?
        .ok_or(DurableTaskRefusal::CheckpointIncompatible)?;
    if pin.owner_run_id != run.run_id.as_str()
        || pin.owner_fence != owned.current.fence
        || pin.durable_task_generation
            != sql_counter(owned.snapshot.context.generation, "task generation")?
        || pin.stop_generation
            != sql_counter(owned.snapshot.context.stop_generation, "stop generation")?
        || pin.expires_at <= clock::now()
        || pin.operation_deadline <= clock::now()
        || pin.released
        || pin.kind != purpose_name(purpose)
    {
        return Err(DurableTaskRefusal::StaleState.into());
    }
    plan.compatible(&owned, purpose)?;
    if purpose == CheckpointTransferPurpose::Upload
        && (plan.owner_run_id != run.run_id.as_str()
            || plan.owner_fence != owned.current.fence
            || plan.transfer_deadline <= clock::now())
    {
        return Err(DurableTaskRefusal::StaleState.into());
    }
    Ok((owned, plan, pin))
}

pub(super) const fn purpose_name(purpose: CheckpointTransferPurpose) -> &'static str {
    match purpose {
        CheckpointTransferPurpose::Upload => "writer",
        CheckpointTransferPurpose::Restore => "reader",
    }
}
