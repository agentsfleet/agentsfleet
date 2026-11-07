//! An in-memory [`Biller`] for domain tests: the same answers as the built-in
//! biller, with no datastore, and every call kept for assertions.
//!
//! It has no usage log, so a check here weighs only the balance: lines a test
//! wants counted against a check are settled through it first.

use std::collections::HashSet;
use std::sync::{Mutex, MutexGuard};

use afd_core::id::Uuid7;
use jiff::Timestamp;

use super::{Account, Ask, Balance, Biller, Funds, Receipt, Settlement, WorkKey};
use crate::error::{billing_wallet_missing, charge_overflow};
use crate::{Nanos, Result};

/// Everything the biller remembers.
#[derive(Debug, Default)]
struct State {
    tenants: Vec<Uuid7>,
    balances: Vec<i64>,
    exhausted_at: Vec<Option<Timestamp>>,
    settled: HashSet<(WorkKey, u32)>,
    settlements: Vec<Settlement>,
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

    /// Gives `tenant` a balance of `balance`, creating its wallet if needed.
    pub fn fund(&self, tenant: &Uuid7, balance: Nanos) {
        let mut state = self.lock();
        if let Some(slot) = state
            .tenant(tenant)
            .and_then(|index| state.balances.get_mut(index))
        {
            *slot = balance.as_i64();
        } else {
            state.tenants.push(tenant.clone());
            state.balances.push(balance.as_i64());
            state.exhausted_at.push(None);
        }
    }

    /// Every settlement this biller was asked to charge, in order.
    #[must_use]
    pub fn settlements(&self) -> Vec<Settlement> {
        self.lock().settlements.clone()
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
        let state = self.lock();
        let index = state
            .tenant(&ask.payer.tenant)
            .ok_or_else(billing_wallet_missing)?;
        let left = Nanos::from_i64(state.balances.get(index).copied().unwrap_or_default());
        Ok(Funds::of(left, ask.next))
    }

    async fn settle(&self, settlement: &Settlement) -> Result<Receipt> {
        let mut state = self.lock();
        let index = state
            .tenant(&settlement.payer.tenant)
            .ok_or_else(billing_wallet_missing)?;
        state.settlements.push(settlement.clone());
        let mut receipt = Receipt::default();
        let mut total: i64 = 0;
        for line in &settlement.lines {
            if state
                .settled
                .insert((settlement.work.clone(), line.usage.seq))
            {
                total = total
                    .checked_add(line.charged.as_i64())
                    .ok_or_else(charge_overflow)?;
                receipt.settled_lines += 1;
            }
        }
        if total == 0 {
            return Ok(receipt);
        }
        // The whole charge, past zero if it must: as the built-in biller's
        // debit, which also stamps when the balance ran out.
        if let Some(balance) = state.balances.get_mut(index) {
            *balance = balance.checked_sub(total).ok_or_else(charge_overflow)?;
            let left = *balance;
            if let Some(since) = state.exhausted_at.get_mut(index) {
                *since = (left <= 0).then(|| since.unwrap_or_else(afd_core::clock::now));
            }
        }
        receipt.charged = Nanos::from_i64(total);
        receipt.taken = receipt.charged;
        Ok(receipt)
    }

    async fn balance(&self, payer: &Account) -> Result<Balance> {
        let state = self.lock();
        let index = state
            .tenant(&payer.tenant)
            .ok_or_else(billing_wallet_missing)?;
        Ok(Balance {
            balance: Nanos::from_i64(state.balances.get(index).copied().unwrap_or_default()),
            unsettled: Nanos::ZERO,
            exhausted_at: state.exhausted_at.get(index).copied().flatten(),
        })
    }
}
