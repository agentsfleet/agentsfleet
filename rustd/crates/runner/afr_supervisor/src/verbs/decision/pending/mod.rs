//! A lease retains physical receipts when its agent future is cancelled.

use std::{collections::HashMap, sync::Arc};

use af_core::id::Uuid7;
use af_wire::task::{DecisionReceiptInput, DecisionReceiptResult};
use parking_lot::Mutex;

mod receipt;
use receipt::Receipt;

use crate::{
    Result, client::ControlPlane, error, halt::Halt, report::delivery::until_acknowledged,
};

/// Owned by one lease; each in-flight physical call retains at most one receipt.
#[derive(Default)]
pub(crate) struct PendingDecisions {
    receipts: Mutex<HashMap<Uuid7, Arc<Receipt>>>,
}

impl std::fmt::Debug for PendingDecisions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PendingDecisions")
            .field("pending", &self.receipts.lock().len())
            .finish_non_exhaustive()
    }
}

impl PendingDecisions {
    pub(super) fn retain(&self, call: Uuid7, input: DecisionReceiptInput) -> Result<Arc<Receipt>> {
        let input = Arc::new(Receipt::new(input));
        let mut receipts = self.receipts.lock();
        match receipts.entry(call) {
            std::collections::hash_map::Entry::Occupied(_) => Err(error::config(
                "a physical decision already has a retained receipt",
            )),
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(Arc::clone(&input));
                Ok(input)
            }
        }
    }

    pub(super) async fn record(
        &self,
        plane: &ControlPlane,
        halt: &Halt,
        lease: &Uuid7,
        call: &Uuid7,
        input: &DecisionReceiptInput,
    ) -> Result<DecisionReceiptResult> {
        let result = until_acknowledged(halt, || async {
            let result = plane.task_decision_record(lease, call, input).await?;
            if result.call_id != call.as_str() {
                return Err(error::config(
                    "decision receipt acknowledgement names another call",
                ));
            }
            Ok(Some(result))
        })
        .await;
        // A dropped future never reaches removal; its lease still owns the receipt.
        self.receipts.lock().remove(call);
        result
    }

    pub(crate) async fn settle(
        &self,
        plane: &ControlPlane,
        halt: &Halt,
        lease: &Uuid7,
    ) -> Result<()> {
        let receipts: Vec<_> = self
            .receipts
            .lock()
            .iter()
            .map(|(call, input)| (call.clone(), Arc::clone(input)))
            .collect();
        for (call, input) in receipts {
            self.record(plane, halt, lease, &call, &input).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
