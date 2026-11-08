//! The biller this daemon moves money through, built in one place.
//!
//! Every money path holds an `Arc<dyn Biller>` (billing §11); which biller
//! that is, and what it does while it cannot answer, is decided here and
//! nowhere else. The lease plane and the billing sweeper both build theirs
//! through [`biller`], so the request path and the sweeper that retries it can
//! never disagree about the policy.

use std::sync::Arc;

use afd_billing::biller::{Biller, Observed, OutagePolicy, WithOutagePolicy};
use afd_billing::ledger::LedgerBiller;
use afd_db::Db;

/// What a reservation does while the biller cannot answer.
///
/// Open: the work runs and its usage settles when the biller is back, because
/// early adoption favours the optimistic path (Indy, 2026-10-19). Flipping to
/// [`OutagePolicy::Closed`] is this one line; revisit when unpaid outage
/// exposure is measurable, or before any third-party biller goes live.
pub const OUTAGE_POLICY: OutagePolicy = OutagePolicy::Open;

/// The built-in ledger biller over `database`, under [`OUTAGE_POLICY`], with
/// every reservation and settlement counted.
///
/// The counting wraps outside the policy, so a reservation the policy admitted
/// during an outage is counted as `unconfirmed` rather than lost as an error.
#[must_use]
pub fn biller(database: Db) -> Arc<dyn Biller> {
    Arc::new(Observed::new(WithOutagePolicy::new(
        LedgerBiller::on_system_clock(database),
        OUTAGE_POLICY,
    )))
}
