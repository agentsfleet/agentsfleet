//! What work does when its biller cannot answer.

use super::{Account, Balance, Biller, Hold, Receipt, Release, Reservation, Settlement, WorkKey};
use crate::Result;

/// Whether work runs while its biller cannot answer a reservation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutagePolicy {
    /// Admit the work; its lines are logged and settle when the biller is back.
    Open,
    /// Refuse the work until the biller answers.
    Closed,
}

/// Any [`Biller`], with an [`OutagePolicy`] applied to its reservations.
///
/// Only `reserve` gates work, so only `reserve` reads the policy: a failed
/// `settle` or `release` leaves the lines logged and unsettled, and the sweeper
/// retries them under either policy.
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
    async fn reserve(&self, ask: &Reservation) -> Result<Hold> {
        match (self.inner.reserve(ask).await, self.policy) {
            (Err(fault), OutagePolicy::Open) if fault.is_datastore_unavailable() => {
                tracing::warn!(
                    error = %fault,
                    error_code = fault.code().as_str(),
                    work_type = ask.work.work_type(),
                    work_id = ask.work.id().as_str(),
                    event = "billing_reserve_unconfirmed",
                    "the biller could not answer; the work runs and settles later"
                );
                Ok(Hold::Unconfirmed)
            }
            (answer, _) => answer,
        }
    }

    async fn settle(&self, settlement: &Settlement) -> Result<Receipt> {
        self.inner.settle(settlement).await
    }

    async fn release(&self, work: &WorkKey, why: Release) -> Result<()> {
        self.inner.release(work, why).await
    }

    async fn balance(&self, payer: &Account) -> Result<Balance> {
        self.inner.balance(payer).await
    }
}
