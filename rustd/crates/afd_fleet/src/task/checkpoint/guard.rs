//! Native task, capture ownership and post-lock deadlines for every operation.

use super::row::{Pin, Plan};
use crate::{
    Result,
    error::query,
    task::{TaskLease, TaskRefusal, Tasks, execution},
};
use afd_core::{clock, id::Uuid7};
use afd_wire::task::{CheckpointTransferPurpose, DecisionTaskVersion};
use sqlx::PgConnection;

pub(super) fn storage(tasks: &Tasks) -> Result<&crate::task::storage::R2CheckpointStore> {
    tasks
        .dependencies
        .storage
        .as_ref()
        .ok_or_else(|| TaskRefusal::StorageUnavailable.into())
}

pub(super) async fn owned(
    connection: &mut PgConnection,
    lease: TaskLease<'_>,
    version: &DecisionTaskVersion,
    checkpoint: &Uuid7,
    pin: &Uuid7,
    purpose: CheckpointTransferPurpose,
) -> Result<(execution::Owned, Plan, Pin)> {
    let owned = execution::load(connection, lease, version).await?;
    with_execution(connection, lease, owned, checkpoint, pin, purpose).await
}

pub(super) async fn with_execution(
    connection: &mut PgConnection,
    lease: TaskLease<'_>,
    owned: execution::Owned,
    checkpoint: &Uuid7,
    pin: &Uuid7,
    purpose: CheckpointTransferPurpose,
) -> Result<(execution::Owned, Plan, Pin)> {
    let plan = Plan::load(
        connection,
        checkpoint,
        &owned.current.task_id.clone().ok_or(TaskRefusal::NotFound)?,
    )
    .await?;
    let pin: Pin = sqlx::query_as(include_str!("sql/pin.sql"))
        .bind(pin.as_str())
        .bind(checkpoint.as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(query("lock checkpoint pin"))?
        .ok_or(TaskRefusal::CheckpointIncompatible)?;
    if pin.owner_lease_id != lease.lease_id.as_str()
        || pin.owner_fence != owned.current.fence
        || pin.task_generation
            != i64::try_from(owned.snapshot.context.generation)
                .map_err(|_overflow| TaskRefusal::StaleState)?
        || pin.stop_generation
            != i64::try_from(owned.snapshot.context.stop_generation)
                .map_err(|_overflow| TaskRefusal::StaleState)?
        || pin.expires_at <= clock::now()
        || pin.operation_deadline <= clock::now()
        || pin.released
        || pin.kind != purpose_name(purpose)
    {
        return Err(TaskRefusal::StaleState.into());
    }
    plan.compatible(&owned, purpose)?;
    if purpose == CheckpointTransferPurpose::Upload
        && (plan.owner_lease_id != lease.lease_id.as_str()
            || plan.owner_fence != owned.current.fence
            || plan.transfer_deadline <= clock::now())
    {
        return Err(TaskRefusal::StaleState.into());
    }
    Ok((owned, plan, pin))
}

pub(super) const fn purpose_name(purpose: CheckpointTransferPurpose) -> &'static str {
    match purpose {
        CheckpointTransferPurpose::Upload => "writer",
        CheckpointTransferPurpose::Restore => "reader",
    }
}
