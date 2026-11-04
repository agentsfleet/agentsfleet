//! Physical decision calls share the native task owner and original lease biller.

mod account;
mod allocate;
mod attempt;
mod budget;
mod compile;
mod context;
mod dispatch;
mod fund;
mod intent;
mod journal;
mod ownership;
mod prepare;
mod quote;
mod record;
mod replay;
mod request;
mod reserve;
mod row;
mod semantic;
mod sql;
mod verify;

use std::fmt;

use afd_core::id::Uuid7;
use afd_wire::task::{DecisionReceiptInput, DecisionReceiptResult, DecisionReservationInput};

use super::{TaskLease, Tasks};
use crate::Result;

/// Full assembled budget query for the database statement inventory.
#[cfg(feature = "test-util")]
pub const DECISION_BUDGET_STATEMENT: &str = sql::BUDGET;

/// Locking call lookup for the database statement inventory.
#[cfg(feature = "test-util")]
pub const LOCKED_DECISION_CALL_STATEMENT: &str = sql::FIND_CALL;

const OPERATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Encoded trusted response; Debug cannot expose its one-use credential.
pub struct ReservedDecision {
    body: zeroize::Zeroizing<String>,
    pub(super) issued: bool,
}

impl ReservedDecision {
    /// Consume the HTTP body together with its newly-issued/replayed status.
    #[must_use]
    pub fn into_parts(mut self) -> (bool, String) {
        (self.issued, std::mem::take(&mut *self.body))
    }
}

impl fmt::Debug for ReservedDecision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReservedDecision")
            .field("issued", &self.issued)
            .field("body", &"[REDACTED]")
            .finish()
    }
}

pub(super) async fn reserve(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    input: &garde::Valid<DecisionReservationInput>,
) -> Result<ReservedDecision> {
    reserve::reserve(tasks, lease, input).await
}

pub(super) async fn record(
    tasks: &Tasks,
    lease: TaskLease<'_>,
    call: &Uuid7,
    input: &garde::Valid<DecisionReceiptInput>,
) -> Result<DecisionReceiptResult> {
    record::record(tasks, lease, call, input).await
}
