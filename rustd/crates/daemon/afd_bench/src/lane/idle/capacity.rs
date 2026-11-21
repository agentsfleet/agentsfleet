//! Measure the turn ledger's backlog at population: rows admitted and not yet delivered.

use af_core::clock;
use af_crypto::entropy::Entropy;
use afd_turns::Turns;

use crate::datastores::Datastores;
use crate::error::Result;
use crate::report::{Report, count};

/// Measurement keys. The `ledger_` prefix says the figure came from the
/// turn ledger in Postgres.
const LEDGER_BACKLOG_ROWS: &str = "ledger_backlog_rows";
const LEDGER_BACKLOG_OLDEST_AGE_MS: &str = "ledger_backlog_oldest_age_ms";

/// Reads the ledger's backlog and records both figures on `report`.
///
/// # Errors
///
/// A ledger that would not answer, through the lane's own lifts.
pub(super) async fn record(stores: &Datastores, report: &mut Report) -> Result<()> {
    // The same ledger shape the daemon holds — see `lane::intake`.
    let ledger = Turns::new(stores.database.clone(), stores.bus.clone(), Entropy::new());
    let backlog = ledger.backlog().await?;
    report.measurement(LEDGER_BACKLOG_ROWS, count(backlog.rows));
    report.measurement(
        LEDGER_BACKLOG_OLDEST_AGE_MS,
        count(backlog.oldest_age.map_or(0, clock::saturating_millis)),
    );
    Ok(())
}
