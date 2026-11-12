//! What work does when its biller cannot answer.

use super::{Account, Ask, Balance, Biller, Funds, Receipt, Settlement};
use crate::Result;

/// Any [`Biller`], admitting work whose funds check it cannot answer.
///
/// The work runs and its lines are logged and settle when the biller is back:
/// early adoption favours the optimistic path, so an outage does not stop a
/// fleet that is otherwise healthy. Only `check` gates work, so only `check` is
/// changed: a failed `settle` leaves the lines logged and unsettled, and the
/// relay retries them.
#[derive(Debug)]
pub struct FailOpen<B> {
    inner: B,
}

impl<B> FailOpen<B> {
    /// `inner`, admitting work when it cannot reach its datastore.
    #[must_use]
    pub const fn new(inner: B) -> Self {
        Self { inner }
    }
}

#[async_trait::async_trait]
impl<B: Biller> Biller for FailOpen<B> {
    async fn check(&self, ask: &Ask) -> Result<Funds> {
        match self.inner.check(ask).await {
            Err(fault) if fault.is_unanswered() => {
                // Bound first: the macro repeats its field expressions.
                let (error_code, work_type, work_id) = (
                    fault.code().as_str(),
                    ask.work.work_type(),
                    ask.work.id().as_str(),
                );
                tracing::warn!(
                    error = %fault,
                    error_code,
                    work_type,
                    work_id,
                    event = "billing_check_unconfirmed",
                    "the biller could not answer; the work runs and settles later"
                );
                Ok(Funds::Unconfirmed)
            }
            answer => answer,
        }
    }

    async fn settle(&self, settlement: &Settlement) -> Result<Receipt> {
        self.inner.settle(settlement).await
    }

    async fn balance(&self, payer: &Account) -> Result<Balance> {
        self.inner.balance(payer).await
    }
}

#[cfg(all(test, feature = "test-util"))]
mod tests;
