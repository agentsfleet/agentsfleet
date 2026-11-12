//! The biller this daemon moves money through, built in one place.
//!
//! Every money path holds an `Arc<dyn Biller>`; which biller that is, and what
//! it does while it cannot answer, is decided here and nowhere else. The lease
//! plane and the billing sweeper both build theirs through [`biller`], so the
//! request path and the sweeper that retries it can never disagree.

use std::sync::Arc;

use afd_billing::biller::{Biller, FailOpen, Observed};
use afd_billing::ledger::LedgerBiller;
use afd_db::Db;

/// The built-in ledger biller over `database`, failing open while it cannot
/// answer, with every reservation and settlement counted.
///
/// The counting wraps outside the failing-open layer, so a reservation admitted
/// during an outage is counted as `unconfirmed` rather than lost as an error.
#[must_use]
pub fn biller(database: Db) -> Arc<dyn Biller> {
    Arc::new(Observed::new(FailOpen::new(LedgerBiller::on_system_clock(
        database,
    ))))
}
