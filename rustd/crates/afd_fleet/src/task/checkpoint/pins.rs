//! One operation pin protects its complete immutable object-reference set.

use super::row::{Pin, Plan};
use crate::{
    Result,
    error::query,
    task::{TaskLease, TaskRefusal, Tasks, execution, transaction},
};
use afd_core::{clock, id::Uuid7};
use afd_wire::task::{CheckpointPin, CheckpointPinInput, CheckpointTransferPurpose};
use sqlx::PgConnection;

pub(super) async fn insert(
    tasks: &Tasks,
    connection: &mut PgConnection,
    owned: &execution::Owned,
    plan: &Plan,
    lease: TaskLease<'_>,
    kind: &str,
) -> Result<Pin> {
    // The live task/slot lock proves a replacement fence before releasing the prior reader.
    sqlx::query(include_str!("sql/pin_replace.sql"))
        .bind(&plan.id)
        .bind(kind)
        .bind(lease.lease_id.as_str())
        .bind(owned.current.fence)
        .execute(&mut *connection)
        .await
        .map_err(query("release superseded recovery pin"))?;
    let existing: Option<Pin> = sqlx::query_as(include_str!("sql/live_pin.sql"))
        .bind(&plan.id)
        .bind(kind)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query("read existing operation pin"))?;
    if let Some(pin) = existing {
        if pin.expires_at <= clock::now() || pin.operation_deadline <= clock::now() {
            return Err(TaskRefusal::StaleState.into());
        }
        return Ok(pin);
    }
    let id = tasks.leases.entropy().uuid7(clock::now())?;
    sqlx::query(include_str!("sql/pin_insert.sql"))
        .bind(id.as_str())
        .bind(&plan.id)
        .bind(&plan.task_id)
        .bind(&plan.fleet_id)
        .bind(&plan.workspace_id)
        .bind(kind)
        .bind(lease.lease_id.as_str())
        .bind(owned.current.fence)
        .bind(
            i64::try_from(owned.snapshot.context.generation)
                .map_err(|_overflow| TaskRefusal::StaleState)?,
        )
        .bind(
            i64::try_from(owned.snapshot.context.stop_generation)
                .map_err(|_overflow| TaskRefusal::StaleState)?,
        )
        .bind(kind == "writer")
        .bind(super::TRANSFER_SECONDS)
        .bind(super::PIN_SECONDS)
        .execute(&mut *connection)
        .await
        .map_err(query("acquire bounded checkpoint operation pin"))?;
    sqlx::query_as(include_str!("sql/pin.sql"))
        .bind(id.as_str())
        .bind(&plan.id)
        .fetch_one(connection)
        .await
        .map_err(query("read acquired checkpoint pin"))
}

pub(in crate::task) async fn renew(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    checkpoint: &Uuid7,
    pin: &Uuid7,
    input: &garde::Valid<CheckpointPinInput>,
) -> Result<CheckpointPin> {
    let mut connection = transaction::acquire(tasks.leases.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let owned = execution::load(&mut tx, lease, &input.task).await?;
    let plan = Plan::load(&mut tx, checkpoint, &input.task.task_id).await?;
    let current: Pin = sqlx::query_as(include_str!("sql/pin.sql"))
        .bind(pin.as_str())
        .bind(checkpoint.as_str())
        .fetch_optional(&mut *tx)
        .await
        .map_err(query("lock pin renewal"))?
        .ok_or(TaskRefusal::NotFound)?;
    let purpose = match current.kind.as_str() {
        "writer" => CheckpointTransferPurpose::Upload,
        "reader" => CheckpointTransferPurpose::Restore,
        _ => return Err(TaskRefusal::CheckpointIncompatible.into()),
    };
    plan.compatible(&owned, purpose)?;
    if current.owner_lease_id != lease.lease_id.as_str()
        || current.owner_fence != owned.current.fence
        || current.resource_version
            != i64::try_from(input.resource_version).map_err(|_overflow| TaskRefusal::StaleState)?
        || current.task_generation
            != i64::try_from(input.task.generation).map_err(|_overflow| TaskRefusal::StaleState)?
        || current.stop_generation
            != i64::try_from(owned.snapshot.context.stop_generation)
                .map_err(|_overflow| TaskRefusal::StaleState)?
        || current.released
        || current.expires_at <= clock::now()
        || current.operation_deadline <= clock::now()
    {
        return Err(TaskRefusal::StaleState.into());
    }
    sqlx::query(include_str!("sql/pin_renew.sql"))
        .bind(pin.as_str())
        .bind(super::PIN_SECONDS)
        .execute(&mut *tx)
        .await
        .map_err(query("renew pin within original operation deadline"))?;
    let current: Pin = sqlx::query_as(include_str!("sql/pin.sql"))
        .bind(pin.as_str())
        .bind(checkpoint.as_str())
        .fetch_one(&mut *tx)
        .await
        .map_err(query("read renewed recovery pin"))?;
    tx.commit()
        .await
        .map_err(query("commit recovery pin renewal"))?;
    transaction::recycle(connection);
    current.wire()
}
