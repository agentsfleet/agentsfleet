//! The biller this daemon moves money through, built in one place.
//!
//! Every money path holds an `Arc<dyn Biller>`; which biller that is, and what
//! it does while it cannot answer, is decided here and nowhere else.

use std::sync::Arc;

use afd_billing::biller::{Biller, FailOpen, Observed};
use afd_billing::ledger::LedgerBiller;
use afd_db::Db;

/// The built-in ledger biller over `database`, failing open while it cannot
/// answer, with every funds check counted.
///
/// The counting wraps outside the failing-open layer, so a check admitted
/// during an outage is counted as `unconfirmed` rather than lost as an error.
#[must_use]
pub fn biller(database: Db) -> Arc<dyn Biller> {
    Arc::new(Observed::new(FailOpen::new(LedgerBiller::new(database))))
}
