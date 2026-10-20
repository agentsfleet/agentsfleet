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
use afd_billing::rates::Posture;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use jiff::Timestamp;
use sqlx::{Connection as _, FromRow as _};

use crate::error::{Result, query, row_malformed};
use crate::lease::sql;
use crate::lease::usage::{self, Pricing, SliceColumns};

/// Leases that ended unreported with run time their cursor never billed.
const SELECT_UNBILLED_EXPIRED_LEASES: &str = include_str!("sql/select_unbilled_expired_leases.sql");

/// The compiled metering statement, for real database query-plan measurements.
#[cfg(feature = "test-util")]
pub const UNBILLED_LEASES_QUERY: &str = SELECT_UNBILLED_EXPIRED_LEASES;

/// One such lease's last slice: its cursor advanced to its deadline, and the
/// run time between, under the lease's row lock.
const BILL_EXPIRED_SLICE: &str = include_str!("sql/bill_expired_slice.sql");

/// What the expiry pass was doing, for [`query`].
const CONTEXT_EXPIRED: &str = "billing sweep expired leases";

/// The table a malformed expired-lease identifier is reported against.
const TABLE_LEASES: &str = "runner.leases";

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
    /// Expired leases whose last slice this pass logged and settled.
    pub expired_billed: u64,
    /// Expired leases whose logged last slice did not settle; the line sweep
    /// settles it on a later pass.
    pub expired_settle_failures: u64,
}

/// One pass: bill every expired lease's last slice, settle every unsettled
/// line older than [`GRACE`] through [`afd_billing::ledger::resettle`], then
/// release every finished lease's open hold.
///
/// The last slice comes first, so the release that follows closes a hold
/// whose run time is already logged (spec Dimension 9.5). `entropy` mints the
/// lines' identifiers and `now` stamps them.
///
/// # Errors
/// Reports a datastore that would not answer a read. A biller that refuses one
/// settlement or release is counted, not raised: the rest of the batch still
/// runs, and the next pass retries what failed.
pub async fn sweep(
    database: &Db,
    biller: &dyn Biller,
    entropy: &Entropy,
    now: Timestamp,
) -> Result<BillingSwept> {
    let (expired_billed, expired_settle_failures) =
        bill_expired(database, biller, entropy, now).await?;
    let lines = afd_billing::ledger::resettle(database, biller).await?;
    let mut swept = BillingSwept {
        resettled: lines.settled,
        settle_failures: lines.failed,
        expired_billed,
        expired_settle_failures,
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

/// Logs and settles the last slice of every lease that ended unreported.
///
/// A lease that expired, or was reclaimed, still owes the run time between
/// its last renewal and the deadline it was granted. Each lease is billed in
/// its own transaction, which advances its cursor to that deadline and logs
/// the runtime line together, so a second pass, or a late report racing it,
/// measures nothing more. A reported lease is never a candidate: its report
/// billed it. Answers how many settled and how many did not.
async fn bill_expired(
    database: &Db,
    biller: &dyn Biller,
    entropy: &Entropy,
    now: Timestamp,
) -> Result<(u64, u64)> {
    let candidates: Vec<String> = {
        let mut connection = database.acquire().await?;
        sqlx::query_scalar(SELECT_UNBILLED_EXPIRED_LEASES)
            .bind(sql::LEASE_STATUS_REPORTED)
            .bind(GRACE)
            .bind(BATCH_LIMIT)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_EXPIRED))?
    };
    let (mut settled, mut unsettled) = (0, 0);
    for lease_id in candidates {
        let lease = Uuid7::parse(&lease_id).map_err(row_malformed(TABLE_LEASES, "id"))?;
        let Some(settlement) = log_last_slice(database, entropy, &lease, now).await? else {
            continue;
        };
        match biller.settle(&settlement).await {
            Ok(_receipt) => settled += 1,
            Err(_fault) => unsettled += 1,
        }
    }
    Ok((settled, unsettled))
}

/// Advances `lease`'s cursor to its deadline and logs the run time between,
/// in one transaction; `None` when another pass or a report got there first.
async fn log_last_slice(
    database: &Db,
    entropy: &Entropy,
    lease: &Uuid7,
    now: Timestamp,
) -> Result<Option<afd_billing::biller::Settlement>> {
    let mut connection = database.acquire().await?;
    let mut transaction = connection.begin().await.map_err(query(CONTEXT_EXPIRED))?;
    let Some(row) = sqlx::query(BILL_EXPIRED_SLICE)
        .bind(lease.as_str())
        .bind(sql::LEASE_STATUS_REPORTED)
        .bind(afd_billing::nanos::MS_PER_SEC)
        .bind(afd_billing::sql::work::FLEET_LEASE)
        .bind(i64::from(afd_wire::usage::DAEMON_SEQ_BASE))
        .fetch_optional(&mut *transaction)
        .await
        .map_err(query(CONTEXT_EXPIRED))?
    else {
        return Ok(None);
    };
    let billed = ExpiredLease::from_row(&row).map_err(query(CONTEXT_EXPIRED))?;
    let slice = SliceColumns::from_row(&row).map_err(query(CONTEXT_EXPIRED))?;
    let pricing = Pricing::runtime_only(
        Posture::parse(&billed.posture).unwrap_or(Posture::Platform),
        billed.provider,
        billed.model,
    );
    // A matched row is the probe row, and the slice is read off it, so it is
    // always present here.
    let settlement = slice
        .into_slice()
        .map_or(Ok(None), |slice| pricing.settlement(lease, slice, &[]))?;
    usage::log_measured(&mut transaction, entropy, now, settlement.as_ref()).await?;
    transaction.commit().await.map_err(query(CONTEXT_EXPIRED))?;
    Ok(settlement)
}

/// What an expired lease's last slice is priced under, by column name.
#[derive(Debug, sqlx::FromRow)]
struct ExpiredLease {
    posture: String,
    provider: String,
    model: String,
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
