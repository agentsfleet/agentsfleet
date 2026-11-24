//! One bounded reservation operation; retry policy never lives in transport.

use af_wire::durable_task::DecisionReservationInput;

use super::{ReservedDecision, allocate};
use crate::{
    Result,
    durable_task::{DurableTaskRun, DurableTasks},
    error::{Error, ErrorKind},
};

pub(super) async fn reserve(
    tasks: &DurableTasks,
    durable_task_run: DurableTaskRun<'_>,
    input: &garde::Valid<DecisionReservationInput>,
) -> Result<ReservedDecision> {
    tokio::time::timeout(
        tasks.times().operation_timeout,
        run(tasks, durable_task_run, input),
    )
    .await
    .map_err(|source| Error::from(ErrorKind::DecisionDeadline { source }))?
}

async fn run(
    tasks: &DurableTasks,
    durable_task_run: DurableTaskRun<'_>,
    input: &garde::Valid<DecisionReservationInput>,
) -> Result<ReservedDecision> {
    allocate::allocate(tasks, durable_task_run, input).await
}
