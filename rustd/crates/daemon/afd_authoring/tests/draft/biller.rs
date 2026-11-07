//! A biller whose chosen calls lose their datastore, the rest answered by the
//! real ledger.
//!
//! A failing call goes to the same ledger over a pool nobody answers, so the
//! fault a proof sees is the transport error production would see, not a value
//! the fixture invented.

use afd_billing::biller::{Account, Ask, Balance, Biller, Funds, Receipt, Settlement};
use afd_billing::ledger::LedgerBiller;
use afd_db::{Db, test_util::unreachable_db};
use std::sync::Arc;

/// How the fixture biller answers a funds check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Check {
    /// The real ledger's answer.
    Ledger,
    /// The ledger lost its datastore.
    Down,
}

/// The calls a [`Faulty`] biller fails; every other call reaches the ledger.
#[derive(Debug, Clone, Copy)]
pub(super) struct Faults {
    pub(super) check: Check,
    pub(super) settle: bool,
    pub(super) balance: bool,
}

impl Faults {
    /// Nothing fails.
    pub(super) const NONE: Self = Self {
        check: Check::Ledger,
        settle: false,
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

    const fn pick(&self, fails: bool) -> &LedgerBiller {
        if fails { &self.down } else { &self.ledger }
    }
}

#[async_trait::async_trait]
impl Biller for Faulty {
    async fn check(&self, ask: &Ask) -> afd_billing::Result<Funds> {
        match self.faults.check {
            Check::Ledger => self.ledger.check(ask).await,
            Check::Down => self.down.check(ask).await,
        }
    }

    async fn settle(&self, settlement: &Settlement) -> afd_billing::Result<Receipt> {
        self.pick(self.faults.settle).settle(settlement).await
    }

    async fn balance(&self, payer: &Account) -> afd_billing::Result<Balance> {
        self.pick(self.faults.balance).balance(payer).await
    }
}
