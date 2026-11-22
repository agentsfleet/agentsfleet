//! One operation pin protects its complete immutable object-reference set.

use super::row::{Pin, Plan};
use crate::{
    Result,
    durable_task::{
        DurableTaskRefusal, DurableTaskRun, DurableTasks, execution, row::sql_counter, transaction,
    },
    error::query,
};
use af_core::{clock, id::Uuid7};
use af_wire::durable_task::{CheckpointPin, CheckpointPinInput, CheckpointTransferPurpose};
use sqlx::PgConnection;

pub(super) async fn insert(
    tasks: &DurableTasks,
    connection: &mut PgConnection,
    owned: &execution::Owned,
    plan: &Plan,
    run: DurableTaskRun<'_>,
    kind: &str,
) -> Result<Pin> {
    // The live task/thread lock proves a replacement fence before releasing the prior reader.
    sqlx::query(include_str!("sql/pin_replace.sql"))
        .bind(&plan.id)
        .bind(kind)
        .bind(run.run_id)
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
            return Err(DurableTaskRefusal::StaleState.into());
        }
        return Ok(pin);
    }
    let id = tasks.runs.entropy().uuid7(clock::now())?;
    sqlx::query(include_str!("sql/pin_insert.sql"))
        .bind(&id)
        .bind(&plan.id)
        .bind(&plan.durable_task_id)
        .bind(kind)
        .bind(run.run_id)
        .bind(owned.current.fence)
        .bind(sql_counter(
            owned.snapshot.context.generation,
            "pin task generation",
        )?)
        .bind(sql_counter(
            owned.snapshot.context.stop_generation,
            "pin stop generation",
        )?)
        .bind(kind == "writer")
        .bind(tasks.times().transfer_seconds())
        .bind(tasks.times().pin_seconds())
        .execute(&mut *connection)
        .await
        .map_err(query("acquire bounded checkpoint operation pin"))?;
    sqlx::query_as(include_str!("sql/pin.sql"))
        .bind(&id)
        .bind(&plan.id)
        .fetch_one(connection)
        .await
        .map_err(query("read acquired checkpoint pin"))
}

pub(in crate::durable_task) async fn renew(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    checkpoint: &Uuid7,
    pin: &Uuid7,
    input: &garde::Valid<CheckpointPinInput>,
) -> Result<CheckpointPin> {
    let mut connection = transaction::acquire(tasks.runs.pool()).await?;
    let mut tx = transaction::begin(&mut connection).await?;
    let owned = execution::load(&mut tx, run, &input.durable_task).await?;
    let plan = Plan::load(
        &mut tx,
        checkpoint,
        owned
            .current
            .durable_task_id
            .as_ref()
            .ok_or(DurableTaskRefusal::NotFound)?,
    )
    .await?;
    let current: Pin = sqlx::query_as(include_str!("sql/pin.sql"))
        .bind(pin)
        .bind(checkpoint)
        .fetch_optional(&mut *tx)
        .await
        .map_err(query("lock pin renewal"))?
        .ok_or(DurableTaskRefusal::NotFound)?;
    let purpose = match current.kind.as_str() {
        "writer" => CheckpointTransferPurpose::Upload,
        "reader" => CheckpointTransferPurpose::Restore,
        _ => return Err(DurableTaskRefusal::CheckpointIncompatible.into()),
    };
    plan.compatible(&owned, purpose)?;
    if current.owner_run_id != run.run_id.as_str()
        || current.owner_fence != owned.current.fence
        || current.resource_version
            != i64::try_from(input.resource_version)
                .map_err(|_overflow| DurableTaskRefusal::StaleState)?
        || current.durable_task_generation
            != i64::try_from(input.durable_task.generation)
                .map_err(|_overflow| DurableTaskRefusal::StaleState)?
        || current.stop_generation
            != sql_counter(
                owned.snapshot.context.stop_generation,
                "pin stop generation",
            )?
        || current.released
        || current.expires_at <= clock::now()
        || current.operation_deadline <= clock::now()
    {
        return Err(DurableTaskRefusal::StaleState.into());
    }
    sqlx::query(include_str!("sql/pin_renew.sql"))
        .bind(pin)
        .bind(tasks.times().pin_seconds())
        .execute(&mut *tx)
        .await
        .map_err(query("renew pin within original operation deadline"))?;
    let current: Pin = sqlx::query_as(include_str!("sql/pin.sql"))
        .bind(pin)
        .bind(checkpoint)
        .fetch_one(&mut *tx)
        .await
        .map_err(query("read renewed recovery pin"))?;
    tx.commit()
        .await
        .map_err(query("commit recovery pin renewal"))?;
    transaction::recycle(connection);
    current.wire()
}
