//! An in-memory [`Biller`] for domain tests: the same answers as the built-in
//! biller, with no datastore, and every call kept for assertions.

use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, MutexGuard};

use afd_core::id::Uuid7;

use super::{Account, Balance, Biller, Hold, Receipt, Release, Reservation, Settlement, WorkKey};
use crate::error::{billing_wallet_missing, charge_overflow};
use crate::{Nanos, Result};

/// One work's hold.
#[derive(Debug, Clone, Copy)]
struct Held {
    tenant_index: usize,
    ceiling: i64,
    captured: i64,
    released: bool,
}

/// Everything the biller remembers.
#[derive(Debug, Default)]
struct State {
    tenants: Vec<Uuid7>,
    balances: Vec<i64>,
    holds: HashMap<WorkKey, Held>,
    settled: HashSet<(WorkKey, u32)>,
    settlements: Vec<Settlement>,
    releases: Vec<(WorkKey, Release)>,
}

impl State {
    fn tenant(&self, tenant: &Uuid7) -> Option<usize> {
        self.tenants.iter().position(|known| known == tenant)
    }

    fn held_by_others(&self, tenant_index: usize, work: &WorkKey) -> i64 {
        self.holds
            .iter()
            .filter(|(key, held)| {
                held.tenant_index == tenant_index && !held.released && *key != work
            })
            .map(|(_, held)| (held.ceiling - held.captured).max(0))
            .sum()
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
        }
    }

    /// Every settlement this biller was asked to charge, in order.
    #[must_use]
    pub fn settlements(&self) -> Vec<Settlement> {
        self.lock().settlements.clone()
    }

    /// Every release this biller was asked to make, in order.
    #[must_use]
    pub fn releases(&self) -> Vec<(WorkKey, Release)> {
        self.lock().releases.clone()
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
    async fn reserve(&self, ask: &Reservation) -> Result<Hold> {
        let mut state = self.lock();
        let index = state
            .tenant(&ask.payer.tenant)
            .ok_or_else(billing_wallet_missing)?;
        let balance = state.balances.get(index).copied().unwrap_or_default();
        let existing = state.holds.get(&ask.work).copied();
        let (ceiling, captured) = match existing {
            Some(held) if held.released => return Ok(Hold::Closed),
            Some(held) => (held.ceiling, held.captured),
            None => (0, 0),
        };
        if ask.ceiling.as_i64() <= ceiling && ceiling > 0 {
            return Ok(held_answer(ceiling, captured));
        }
        let available = (balance - state.held_by_others(index, &ask.work)).max(0);
        let needed = (ask.ceiling.as_i64() - captured).max(0);
        if available < needed {
            return Ok(Hold::Short {
                available: Nanos::from_i64(available),
                asked: ask.ceiling,
            });
        }
        let ceiling = ceiling.max(ask.ceiling.as_i64());
        state.holds.insert(
            ask.work.clone(),
            Held {
                tenant_index: index,
                ceiling,
                captured,
                released: false,
            },
        );
        Ok(held_answer(ceiling, captured))
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
        let taken = state.balances.get_mut(index).map_or(0, |slot| {
            let taken = total.min(*slot);
            *slot -= taken;
            taken
        });
        if let Some(held) = state.holds.get_mut(&settlement.work) {
            held.captured += taken;
        }
        receipt.charged = Nanos::from_i64(total);
        receipt.taken = Nanos::from_i64(taken);
        receipt.written_off = Nanos::from_i64(total - taken);
        Ok(receipt)
    }

    async fn release(&self, work: &WorkKey, why: Release) -> Result<()> {
        let mut state = self.lock();
        state.releases.push((work.clone(), why));
        if let Some(held) = state.holds.get_mut(work) {
            held.released = true;
        }
        Ok(())
    }

    async fn balance(&self, payer: &Account) -> Result<Balance> {
        let state = self.lock();
        let index = state
            .tenant(&payer.tenant)
            .ok_or_else(billing_wallet_missing)?;
        let held = state
            .holds
            .values()
            .filter(|held| held.tenant_index == index && !held.released)
            .map(|held| (held.ceiling - held.captured).max(0))
            .sum();
        Ok(Balance {
            balance: Nanos::from_i64(state.balances.get(index).copied().unwrap_or_default()),
            held: Nanos::from_i64(held),
            exhausted_at: None,
        })
    }
}

const fn held_answer(ceiling: i64, captured: i64) -> Hold {
    Hold::Held {
        ceiling: Nanos::from_i64(ceiling),
        captured: Nanos::from_i64(captured),
    }
}
