//! One bounded reservation operation; retry policy never lives in transport.

use afd_wire::task::DecisionReservationInput;

use super::{
    ReservedDecision,
    allocate::{self, Allocation},
    fund, replay,
};
use crate::{
    Result,
    error::{Error, ErrorKind},
    task::{TaskLease, Tasks},
};

pub(super) async fn reserve(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    input: &garde::Valid<DecisionReservationInput>,
) -> Result<ReservedDecision> {
    tokio::time::timeout(super::OPERATION_TIMEOUT, run(tasks, lease, input))
        .await
        .map_err(|source| Error::from(ErrorKind::DecisionDeadline { source }))?
}

async fn run(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    input: &garde::Valid<DecisionReservationInput>,
) -> Result<ReservedDecision> {
    match allocate::allocate(tasks, lease, input).await? {
        Allocation::Reserved(call) => fund::issue(tasks, lease, input, call).await,
        Allocation::Existing(call) => replay::response(tasks, call).await,
    }
}
