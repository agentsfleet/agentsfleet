//! The billing sweeper's pass over fleet leases: settle what committed without
//! a settlement, and release the holds of leases that ended.
//!
//! A lease logs its usage inside its own transaction and settles after commit
//! (billing §11), so a crash, an outage or a lost response between the two
//! leaves lines logged and unsettled, or a finished lease's hold open. Both
//! keep the money honest — an unsettled line is still owed, and an open hold
//! still counts against the tenant's balance — so this pass finishes what the
//! request path could not. Every call it makes is idempotent: settling keys on
//! `(work, seq)` and releasing on the work.

use std::time::Duration;

use afd_billing::biller::{Biller, Release, WorkKey};
use afd_core::id::Uuid7;
use afd_db::Db;

use crate::error::{Result, query, row_malformed};
use crate::lease::sql;

/// What the finished-holds statement was doing, for [`query`].
const CONTEXT_HOLDS: &str = "billing sweep finished holds";

/// The table a malformed hold identifier is reported against.
const TABLE_HOLDS: &str = "billing.holds";

/// How long a lease must have been finished before its hold is released.
///
/// The same grace the line sweep gives the request path's own settle,
/// measured from the lease's last change rather than the hold's: settling a
/// line captures onto the hold and would otherwise restart the wait.
pub const GRACE: Duration = afd_billing::ledger::GRACE;

/// Holds one pass reads; bounds the pass's work.
const BATCH_LIMIT: i64 = 500;

/// Open holds of leases that ended without releasing them.
const SELECT_FINISHED_HOLDS: &str = include_str!("sql/select_finished_holds.sql");

/// What one pass did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BillingSwept {
    /// Works whose unsettled lines this pass settled.
    pub resettled: u64,
    /// Lines' works it could not settle; they wait for the next pass.
    pub settle_failures: u64,
    /// Holds of finished leases this pass released.
    pub released: u64,
    /// Holds it could not release; they wait for the next pass.
    pub release_failures: u64,
}

/// One pass: settle every unsettled line older than [`GRACE`] through
/// [`afd_billing::ledger::resettle`], then release every finished lease's open
/// hold.
///
/// # Errors
/// Reports a datastore that would not answer a read. A biller that refuses one
/// settlement or release is counted, not raised: the rest of the batch still
/// runs, and the next pass retries what failed.
pub async fn sweep(database: &Db, biller: &dyn Biller) -> Result<BillingSwept> {
    let lines = afd_billing::ledger::resettle(database, biller).await?;
    let mut swept = BillingSwept {
        resettled: lines.settled,
        settle_failures: lines.failed,
        ..BillingSwept::default()
    };

    let finished: Vec<FinishedHold> = {
        let mut connection = database.acquire().await?;
        sqlx::query_as(SELECT_FINISHED_HOLDS)
            .bind(afd_billing::sql::work::FLEET_LEASE)
            .bind(sql::LEASE_STATUS_ACTIVE)
            .bind(GRACE)
            .bind(BATCH_LIMIT)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_HOLDS))?
    };
    for hold in finished {
        let lease = Uuid7::parse(&hold.lease_id).map_err(row_malformed(TABLE_HOLDS, "work_id"))?;
        match biller
            .release(&WorkKey::FleetLease(lease), hold.release())
            .await
        {
            Ok(()) => swept.released += 1,
            Err(_fault) => swept.release_failures += 1,
        }
    }
    Ok(swept)
}

/// A finished lease's open hold, by column name.
#[derive(Debug, sqlx::FromRow)]
struct FinishedHold {
    lease_id: String,
    /// `None` once retention deleted the lease row.
    status: Option<String>,
}

impl FinishedHold {
    /// Why the hold is released: a reported lease completed and only its
    /// release was lost; anything else ended without a report.
    fn release(&self) -> Release {
        if self.status.as_deref() == Some(sql::LEASE_STATUS_REPORTED) {
            Release::Completed
        } else {
            Release::Expired {
                runtime: Duration::ZERO,
            }
        }
    }
}
