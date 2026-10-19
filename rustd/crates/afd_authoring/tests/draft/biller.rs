//! A biller whose chosen calls lose their datastore, the rest answered by the
//! real ledger.
//!
//! A failing call goes to the same ledger over a pool nobody answers, so the
//! fault a proof sees is the transport error production would see, not a value
//! the fixture invented. `Short` is the one invented answer: a balance that moved
//! between a gate and its hold, which no single-threaded fixture can race into.

use afd_billing::biller::{
    Account, Balance, Biller, Hold, Receipt, Release, Reservation, Settlement, WorkKey,
};
use afd_billing::ledger::LedgerBiller;
use afd_db::{Db, test_util::unreachable_db};
use std::sync::Arc;

/// How the fixture biller answers a reservation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Reserve {
    /// The real ledger's answer.
    Ledger,
    /// The ledger lost its datastore.
    Down,
    /// The balance cannot cover the ask any more.
    Short,
}

/// The calls a [`Faulty`] biller fails; every other call reaches the ledger.
#[derive(Debug, Clone, Copy)]
pub(super) struct Faults {
    pub(super) reserve: Reserve,
    pub(super) settle: bool,
    pub(super) release: bool,
    pub(super) balance: bool,
}

impl Faults {
    /// Nothing fails.
    pub(super) const NONE: Self = Self {
        reserve: Reserve::Ledger,
        settle: false,
        release: false,
        balance: false,
    };
}

#[derive(Debug)]
pub(super) struct Faulty {
    ledger: LedgerBiller,
    down: LedgerBiller,
    faults: Faults,
}

impl Faulty {
    /// The real ledger over `db`, failing the calls `faults` names.
    pub(super) fn over(db: &Db, faults: Faults) -> Arc<dyn Biller> {
        Arc::new(Self {
            ledger: LedgerBiller::on_system_clock(db.clone()),
            down: LedgerBiller::on_system_clock(unreachable_db()),
            faults,
        })
    }

    fn pick(&self, fails: bool) -> &LedgerBiller {
        if fails { &self.down } else { &self.ledger }
    }
}

#[async_trait::async_trait]
impl Biller for Faulty {
    async fn reserve(&self, ask: &Reservation) -> afd_billing::Result<Hold> {
        match self.faults.reserve {
            Reserve::Ledger => self.ledger.reserve(ask).await,
            Reserve::Down => self.down.reserve(ask).await,
            Reserve::Short => Ok(Hold::Short {
                available: afd_billing::Nanos::ZERO,
                asked: ask.ceiling,
            }),
        }
    }

    async fn settle(&self, settlement: &Settlement) -> afd_billing::Result<Receipt> {
        self.pick(self.faults.settle).settle(settlement).await
    }

    async fn release(&self, work: &WorkKey, why: Release) -> afd_billing::Result<()> {
        self.pick(self.faults.release).release(work, why).await
    }

    async fn balance(&self, payer: &Account) -> afd_billing::Result<Balance> {
        self.pick(self.faults.balance).balance(payer).await
    }
}
