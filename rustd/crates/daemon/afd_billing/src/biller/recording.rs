//! An in-memory [`Biller`] for domain tests: the same answers as the built-in
//! biller, with no datastore.
//!
//! It has no usage log: a test sets the balance a check weighs with
//! [`RecordingBiller::fund`].

use std::sync::{Mutex, MutexGuard};

use af_core::id::Uuid7;
use jiff::Timestamp;

use super::{Account, Ask, Balance, Biller, Funds};
use crate::error::billing_wallet_missing;
use crate::{Nanos, Result};

/// Every balance the biller holds, by tenant.
#[derive(Debug, Default)]
struct State {
    tenants: Vec<Uuid7>,
    balances: Vec<i64>,
    exhausted_at: Vec<Option<Timestamp>>,
}

impl State {
    fn tenant(&self, tenant: &Uuid7) -> Option<usize> {
        self.tenants.iter().position(|known| known == tenant)
    }
}

/// A [`Biller`] whose balances live in memory.
#[derive(Debug, Default)]
pub struct RecordingBiller {
    state: Mutex<State>,
}

impl RecordingBiller {
    /// A biller with no payers yet.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Gives `tenant` a balance of `balance`, creating its wallet if needed. A
    /// balance at or below zero records that it ran out, as a debit there does.
    pub fn fund(&self, tenant: &Uuid7, balance: Nanos) {
        let mut state = self.lock();
        let ran_out = (balance.as_i64() <= 0).then(af_core::clock::now);
        if let Some(index) = state.tenant(tenant) {
            if let Some(slot) = state.balances.get_mut(index) {
                *slot = balance.as_i64();
            }
            if let Some(since) = state.exhausted_at.get_mut(index) {
                *since = ran_out.map(|now| since.unwrap_or(now));
            }
        } else {
            state.tenants.push(tenant.clone());
            state.balances.push(balance.as_i64());
            state.exhausted_at.push(ran_out);
        }
    }

    #[expect(
        clippy::expect_used,
        reason = "test double: a poisoned lock means another test thread already failed"
    )]
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .expect("no test thread panics while holding the biller's state")
    }
}

#[async_trait::async_trait]
impl Biller for RecordingBiller {
    async fn check(&self, ask: &Ask) -> Result<Funds> {
        let balance = self.balance(&ask.payer).await?;
        Ok(Funds::of(balance.balance, ask.next))
    }

    async fn balance(&self, payer: &Account) -> Result<Balance> {
        let state = self.lock();
        let index = state
            .tenant(&payer.tenant)
            .ok_or_else(billing_wallet_missing)?;
        Ok(Balance {
            balance: Nanos::from_i64(state.balances.get(index).copied().unwrap_or_default()),
            exhausted_at: state.exhausted_at.get(index).copied().flatten(),
        })
    }
}
