//! What work does when its biller cannot answer.

use super::{Account, Ask, Balance, Biller, Funds, Receipt, Settlement};
use crate::Result;

/// Whether work runs while its biller cannot answer a funds check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutagePolicy {
    /// Admit the work; its lines are logged and settle when the biller is back.
    Open,
    /// Refuse the work until the biller answers.
    Closed,
}

/// Any [`Biller`], with an [`OutagePolicy`] applied to its funds checks.
///
/// Only `check` gates work, so only `check` reads the policy: a failed
/// `settle` leaves the lines logged and unsettled, and the relay retries them
/// under either policy.
#[derive(Debug)]
pub struct WithOutagePolicy<B> {
    inner: B,
    policy: OutagePolicy,
}

impl<B> WithOutagePolicy<B> {
    /// `inner`, answering `policy` when it cannot reach its datastore.
    #[must_use]
    pub const fn new(inner: B, policy: OutagePolicy) -> Self {
        Self { inner, policy }
    }
}

#[async_trait::async_trait]
impl<B: Biller> Biller for WithOutagePolicy<B> {
    async fn check(&self, ask: &Ask) -> Result<Funds> {
        match (self.inner.check(ask).await, self.policy) {
            (Err(fault), OutagePolicy::Open) if fault.is_unanswered() => {
                tracing::warn!(
                    error = %fault,
                    error_code = fault.code().as_str(),
                    work_type = ask.work.work_type(),
                    work_id = ask.work.id().as_str(),
                    event = "billing_check_unconfirmed",
                    "the biller could not answer; the work runs and settles later"
                );
                Ok(Funds::Unconfirmed)
            }
            (answer, _) => answer,
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
