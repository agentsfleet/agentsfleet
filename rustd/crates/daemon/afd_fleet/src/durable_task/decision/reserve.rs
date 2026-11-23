//! One bounded reservation operation; retry policy never lives in transport.

use af_wire::durable_task::DecisionReservationInput;

use super::{
    ReservedDecision,
    allocate::{self, Allocation},
    fund, replay,
};
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
    match allocate::allocate(tasks, durable_task_run, input).await? {
        Allocation::Reserved(call) => fund::issue(tasks, durable_task_run, input, call).await,
        Allocation::Existing(call) => replay::response(tasks, call).await,
    }
}
