//! One bounded reservation operation; retry policy never lives in transport.

use af_wire::task::DecisionReservationInput;

use super::{
    ReservedDecision,
    allocate::{self, Allocation},
    fund, replay,
};
use crate::{
    Result,
    error::{Error, ErrorKind},
    task::{TaskRun, Tasks},
};

pub(super) async fn reserve(
    tasks: &Tasks,
    task_run: TaskRun<'_>,
    input: &garde::Valid<DecisionReservationInput>,
) -> Result<ReservedDecision> {
    tokio::time::timeout(super::OPERATION_TIMEOUT, run(tasks, task_run, input))
        .await
        .map_err(|source| Error::from(ErrorKind::DecisionDeadline { source }))?
}

async fn run(
    tasks: &Tasks,
    task_run: TaskRun<'_>,
    input: &garde::Valid<DecisionReservationInput>,
) -> Result<ReservedDecision> {
    match allocate::allocate(tasks, task_run, input).await? {
        Allocation::Reserved(call) => fund::issue(tasks, task_run, input, call).await,
        Allocation::Existing(call) => replay::response(tasks, call).await,
    }
}
