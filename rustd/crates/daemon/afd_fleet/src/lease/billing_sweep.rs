//! The billing relay's pass over fleet leases: bill the last slice of a lease
//! that ended unreported, and settle every logged line.
//!
//! A lease logs its usage inside its own transaction and returns; it never
//! settles (billing §11). This pass is the only thing that moves
//! that money, through the [`Biller`] it is given, so the biller can live in
//! another process without the lease path changing. Settling is idempotent on
//! `(work, seq)`, so a repeated pass or a second daemon's relay charges nothing
//! twice.

use std::time::Duration;

use af_core::error_code::ErrorCode;
use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use afd_billing::biller::Biller;
use afd_billing::ledger::Resume;
use afd_billing::rates::Posture;
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

/// How long an expired, unreported lease waits before its last slice is billed.
///
/// Measured from its deadline, so a runner's late report still wins. A
/// reported lease is never billed here: its report billed it.
pub const GRACE: Duration = Duration::from_secs(120);

/// Expired leases one pass reads; bounds the pass's work.
const BATCH_LIMIT: i64 = 500;

/// What one pass did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BillingSwept {
    /// Work/provider/model/posture groups this pass settled.
    pub resettled: u64,
    /// Settlement groups it could not settle; they wait for the next pass.
    pub settle_failures: u64,
    /// Expired leases whose last slice this pass logged and settled.
    pub expired_billed: u64,
    /// Expired leases whose logged last slice did not settle; the line sweep
    /// settles it on a later pass.
    pub expired_settle_failures: u64,
    /// Whether the line sweep read a whole batch, so more lines may wait.
    pub lines_full: bool,
    /// The code of the first refusal, for the pass's one aggregate record.
    pub refused: Option<ErrorCode>,
    /// Where the next pass's line read resumes.
    pub resume: Option<Resume>,
}

/// A work whose settlement the biller refused this pass.
const EVENT_REFUSED: &str = "billing_relay_refused";

impl BillingSwept {
    /// Notes that the biller refused `work_id`: one `debug` record per work
    /// (LOG-HOT), and the first code kept for the pass's aggregate.
    fn refused(&mut self, work_id: &str, fault: &afd_billing::Error) {
        let error_code = fault.code();
        self.refused.get_or_insert(error_code);
        let error_code = error_code.as_str();
        tracing::debug!(
            work_id,
            error_code,
            event = EVENT_REFUSED,
            "the biller refused this work; the next pass retries"
        );
    }
}

/// One relay pass over fleet leases.
///
/// Bills the last slice of every lease expired past [`GRACE`], then settles one
/// batch of unsettled lines after `after` through
/// [`afd_billing::ledger::resettle`]. `entropy` mints the lines' identifiers
/// and `now` stamps them.
///
/// # Errors
/// Reports a datastore that would not answer a read. A biller that refuses one
/// settlement is counted, not raised: the rest of the batch still runs, and
/// the next pass retries what failed.
pub async fn sweep(
    database: &Db,
    biller: &dyn Biller,
    entropy: &Entropy,
    now: Timestamp,
    after: Option<&Resume>,
) -> Result<BillingSwept> {
    let mut swept = BillingSwept::default();
    bill_expired(database, biller, entropy, now, &mut swept).await?;
    let lines = afd_billing::ledger::resettle(database, biller, after).await?;
    swept.resettled = lines.settled;
    swept.settle_failures = lines.failed;
    swept.lines_full = lines.full;
    swept.refused = swept.refused.or(lines.refused);
    swept.resume = lines.resume;

    Ok(swept)
}

/// Logs and settles the last slice of every lease that ended unreported.
///
/// A lease that expired, or was reclaimed, still owes the run time between
/// its last renewal and the deadline it was granted. Each lease is billed in
/// its own transaction, which advances its cursor to that deadline and logs
/// the runtime line together, so a second pass, or a late report racing it,
/// measures nothing more. A reported lease is never a candidate: its report
/// billed it. Counts what settled and what did not on `swept`.
async fn bill_expired(
    database: &Db,
    biller: &dyn Biller,
    entropy: &Entropy,
    now: Timestamp,
    swept: &mut BillingSwept,
) -> Result<()> {
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
    for lease_id in candidates {
        let lease = Uuid7::parse(&lease_id).map_err(row_malformed(TABLE_LEASES, "id"))?;
        let Some(settlement) = log_last_slice(database, entropy, &lease, now).await? else {
            continue;
        };
        match biller.settle(&settlement).await {
            Ok(_receipt) => swept.expired_billed += 1,
            Err(fault) => {
                swept.expired_settle_failures += 1;
                swept.refused(&lease_id, &fault);
            }
        }
    }
    Ok(())
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
        .bind(i64::from(af_wire::usage::DAEMON_SEQ_BASE))
        .bind(i64::from(af_wire::usage::DECISION_SEQ_BASE))
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
