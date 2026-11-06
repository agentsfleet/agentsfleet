//! A biller whose settlements lose their datastore, everything else answered
//! by the real ledger.
//!
//! A failing call goes to the same ledger over a pool nobody answers, so the
//! fault a proof sees is the transport error production would see, not a value
//! the fixture invented.

use std::sync::Arc;

use afd_billing::biller::{Account, Ask, Balance, Biller, Funds, Receipt, Settlement};
use afd_billing::ledger::LedgerBiller;
use afd_db::{Db, test_util::unreachable_db};

/// The calls a [`Faulty`] biller fails; every other call reaches the ledger.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Faults {
    pub(crate) settle: bool,
}

#[derive(Debug)]
pub(crate) struct Faulty {
    ledger: LedgerBiller,
    down: LedgerBiller,
    faults: Faults,
}

impl Faulty {
    /// The real ledger over `database`, failing the calls `faults` names.
    pub(crate) fn over(database: &Db, faults: Faults) -> Arc<dyn Biller> {
        Arc::new(Self {
            ledger: LedgerBiller::on_system_clock(database.clone()),
            down: LedgerBiller::on_system_clock(unreachable_db()),
            faults,
        })
    }
}

#[async_trait::async_trait]
impl Biller for Faulty {
    async fn check(&self, ask: &Ask) -> afd_billing::Result<Funds> {
        self.ledger.check(ask).await
    }

    async fn settle(&self, settlement: &Settlement) -> afd_billing::Result<Receipt> {
        if self.faults.settle {
            self.down.settle(settlement).await
        } else {
            self.ledger.settle(settlement).await
        }
    }

    async fn balance(&self, payer: &Account) -> afd_billing::Result<Balance> {
        self.ledger.balance(payer).await
    }
}
