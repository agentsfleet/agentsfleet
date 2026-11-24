//! A run retains physical receipts when its agent future is cancelled.

use std::sync::Arc;

use af_core::id::Uuid7;
use af_wire::durable_task::{DecisionReceiptInput, DecisionReceiptResult};
use dashmap::DashMap;

mod receipt;
use receipt::Receipt;

use crate::{
    Result, client::ControlPlane, error, halt::Halt, report::delivery::until_acknowledged,
};

/// Owned by one run; each in-flight physical call retains at most one receipt.
#[derive(Default)]
pub(crate) struct PendingDecisions {
    receipts: DashMap<Uuid7, Arc<Receipt>>,
}

impl std::fmt::Debug for PendingDecisions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingDecisions")
            .field("pending", &self.receipts.len())
            .finish_non_exhaustive()
    }
}

impl PendingDecisions {
    pub(super) fn retain(&self, call: Uuid7, input: DecisionReceiptInput) -> Result<Arc<Receipt>> {
        let input = Arc::new(Receipt::new(input));
        match self.receipts.entry(call) {
            dashmap::Entry::Occupied(_) => Err(error::config(
                "a physical decision already has a retained receipt",
            )),
            dashmap::Entry::Vacant(entry) => {
                entry.insert(Arc::clone(&input));
                Ok(input)
            }
        }
    }

    pub(super) async fn record(
        &self,
        plane: &ControlPlane,
        halt: &Halt,
        run: &Uuid7,
        call: &Uuid7,
        input: &DecisionReceiptInput,
    ) -> Result<DecisionReceiptResult> {
        let result = until_acknowledged(halt, plane.runtime, async || {
            let result = plane.durable_task_decision_record(run, call, input).await?;
            if result.call_id != call.as_str() {
                return Err(error::config(
                    "decision receipt acknowledgement names another call",
                ));
            }
            Ok(Some(result))
        })
        .await;
        // A dropped future never reaches removal; its run still owns the receipt.
        self.receipts.remove(call);
        result
    }

    pub(crate) async fn settle(
        &self,
        plane: &ControlPlane,
        halt: &Halt,
        run: &Uuid7,
    ) -> Result<()> {
        let receipts: Vec<_> = self
            .receipts
            .iter()
            .map(|entry| (entry.key().clone(), Arc::clone(entry.value())))
            .collect();
        for (call, input) in receipts {
            self.record(plane, halt, run, &call, &input).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
