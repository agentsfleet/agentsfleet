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

/// One tenant's wallet.
#[derive(Debug)]
struct Wallet {
    tenant: Uuid7,
    balance: i64,
    exhausted_at: Option<Timestamp>,
}

/// Every wallet the biller holds.
#[derive(Debug, Default)]
struct State {
    wallets: Vec<Wallet>,
}

impl State {
    fn wallet(&mut self, tenant: &Uuid7) -> Option<&mut Wallet> {
        self.wallets
            .iter_mut()
            .find(|wallet| wallet.tenant == *tenant)
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
        if let Some(wallet) = state.wallet(tenant) {
            wallet.balance = balance.as_i64();
            wallet.exhausted_at = ran_out.map(|now| wallet.exhausted_at.unwrap_or(now));
        } else {
            state.wallets.push(Wallet {
                tenant: tenant.clone(),
                balance: balance.as_i64(),
                exhausted_at: ran_out,
            });
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
        let mut state = self.lock();
        let wallet = state
            .wallet(&payer.tenant)
            .ok_or_else(billing_wallet_missing)?;
        Ok(Balance {
            balance: Nanos::from_i64(wallet.balance),
            exhausted_at: wallet.exhausted_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use af_core::id::{ENTROPY_LEN, Uuid7};

    use super::RecordingBiller;
    use crate::Nanos;
    use crate::biller::{Account, Biller as _};

    type Checked = Result<(), Box<dyn std::error::Error>>;

    /// A second fund replaces the balance, and a wallet that ran out keeps
    /// the instant it first did until it holds credit again.
    #[tokio::test]
    async fn refunding_a_wallet_replaces_its_balance_and_keeps_when_it_ran_out() -> Checked {
        let tenant = Uuid7::encode(jiff::Timestamp::UNIX_EPOCH, [2; ENTROPY_LEN])?;
        let payer = Account {
            tenant: tenant.clone(),
            workspace: tenant.clone(),
        };
        let biller = RecordingBiller::new();
        biller.fund(&tenant, Nanos::from_i64(500));
        assert_eq!(biller.balance(&payer).await?.exhausted_at, None);

        biller.fund(&tenant, Nanos::ZERO);
        let ran_out = biller.balance(&payer).await?;
        assert_eq!(ran_out.balance, Nanos::ZERO);
        let since = ran_out.exhausted_at.ok_or("a spent wallet records when")?;

        biller.fund(&tenant, Nanos::from_i64(-40));
        let owing = biller.balance(&payer).await?;
        assert_eq!(owing.balance, Nanos::from_i64(-40));
        assert_eq!(owing.exhausted_at, Some(since), "still the first instant");

        biller.fund(&tenant, Nanos::from_i64(75));
        let funded = biller.balance(&payer).await?;
        assert_eq!(
            (funded.balance, funded.exhausted_at),
            (Nanos::from_i64(75), None)
        );
        Ok(())
    }
}
