//! Physical decision calls share the native task owner and original run biller.

mod account;
mod allocate;
mod attempt;
pub(super) mod authorize;
mod budget;
pub(super) mod compile;
mod context;
mod credit;
mod dispatch;
mod fund;
mod journal;
mod locks;
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
pub(in crate::durable_task) use authorize::goal;

use std::fmt;

use af_core::id::Uuid7;
use af_wire::durable_task::{
    DecisionReceiptInput, DecisionReceiptResult, DecisionReservationInput,
};

use super::{DurableTaskRun, DurableTasks};
use crate::Result;

/// Full assembled budget query for the database statement inventory.
#[cfg(feature = "test-util")]
pub const DECISION_BUDGET_STATEMENT: &str = sql::BUDGET;

/// Locking call lookup for the database statement inventory.
#[cfg(feature = "test-util")]
pub const LOCKED_DECISION_CALL_STATEMENT: &str = sql::FIND_CALL;

pub(crate) const OPERATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

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
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    input: &garde::Valid<DecisionReservationInput>,
) -> Result<ReservedDecision> {
    reserve::reserve(tasks, run, input).await
}

pub(super) async fn record(
    tasks: &DurableTasks,
    run: DurableTaskRun<'_>,
    call: &Uuid7,
    input: &garde::Valid<DecisionReceiptInput>,
) -> Result<DecisionReceiptResult> {
    record::record(tasks, run, call, input).await
}
