//! The billing relay's two passes over runs, a fleet's or a draft's: bill the last slice of a
//! run that ended unreported ([`bill_expired_runs`]), and settle every
//! logged line ([`settle_lines`]).
//!
//! A run logs its usage inside its own transaction and returns; it never
//! settles. These passes are the only things that move
//! that money, through the [`Biller`] they are given, so the biller can live in
//! another process without the run path changing. Settling is idempotent on
//! `(work, kind, seq)`, so a repeated pass or a second daemon's relay charges
//! nothing twice. Each pass answers its own record, because each fills a
//! different half of what a tick can report.

use std::time::Duration;

use af_core::error_code::ErrorCode;
use af_core::id::Uuid7;
use af_crypto::entropy::Entropy;
use afd_billing::biller::Biller;
use afd_billing::ledger::Resume;
use afd_billing::ledger::unreadable::{Unreadable, skipped};
use afd_billing::rates::Posture;
use afd_db::Db;
use jiff::Timestamp;
use sqlx::{Connection as _, FromRow as _};

use crate::error::{Result, query};
use crate::run::sql;
use crate::run::usage::{self, Pricing, SliceColumns};

/// Runs that ended unreported with run time their cursor never billed.
const SELECT_UNBILLED_EXPIRED_RUNS: &str = include_str!("sql/select_unbilled_expired_runs.sql");

/// The compiled metering statement, for real database query-plan measurements.
#[cfg(feature = "test-util")]
pub const UNBILLED_RUNS_QUERY: &str = SELECT_UNBILLED_EXPIRED_RUNS;

/// One such run's last slice: its cursor advanced to its deadline, and the
/// run time between, under the run's row lock.
const BILL_EXPIRED_SLICE: &str = include_str!("sql/bill_expired_slice.sql");

/// What the expiry pass was doing, for [`query`].
const CONTEXT_EXPIRED: &str = "billing sweep expired runs";

/// The run statuses a report settles in. A run in either was billed by its
/// report, so the sweep never measures it again.
const SETTLED_STATUSES: [&str; 2] = [sql::RUN_STATUS_REPORTED, sql::RUN_STATUS_STOPPED];

/// How long an expired, unreported run waits before its last slice is billed.
///
/// Measured from its deadline, so a runner's late report still wins. A
/// reported run is never billed here: its report billed it.
pub const GRACE: Duration = Duration::from_secs(120);

/// Expired runs one pass reads; bounds the pass's work.
const BATCH_LIMIT: i64 = 500;

/// What one expiry pass did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExpirySwept {
    /// Expired runs whose last slice this pass logged and settled.
    pub billed: u64,
    /// Expired runs whose logged last slice did not settle; the line pass
    /// settles it on a later tick.
    pub settle_failures: u64,
    /// The code of the first refusal, for the pass's one aggregate record.
    pub refused: Option<ErrorCode>,
}

/// What one line-settling pass did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LinesSwept {
    /// Work/provider/model/posture groups this pass settled.
    pub resettled: u64,
    /// Settlement groups it could not settle; they wait for the next pass.
    pub settle_failures: u64,
    /// Whether the pass read a whole batch, so more lines may wait.
    pub full: bool,
    /// The code of the first refusal, for the pass's one aggregate record.
    pub refused: Option<ErrorCode>,
    /// Where the next pass's line read resumes.
    pub resume: Option<Resume>,
}

/// A work whose settlement the biller refused this pass.
const EVENT_REFUSED: &str = "billing_relay_refused";

impl ExpirySwept {
    /// Notes that the biller refused `work_id`: one `debug` record per work
    /// (LOG-HOT), and the first code kept for the pass's aggregate.
    fn refused(&mut self, work_id: &str, fault: &afd_billing::Error) {
        let error_code = fault.code();
        self.refused.get_or_insert(error_code);
        let error_code = error_code.as_str();
        {
            let (event, message) = (
                &EVENT_REFUSED,
                format_args!("the biller refused this work; the next pass retries"),
            );
            tracing::debug!(message, work_id, error_code, event);
        };
    }
}

/// Both passes of a tick in one record, as a test drives them.
#[cfg(feature = "test-util")]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BillingSwept {
    /// Work/provider/model/posture groups the line pass settled.
    pub resettled: u64,
    /// Settlement groups the line pass could not settle.
    pub settle_failures: u64,
    /// Expired runs whose last slice the expiry pass logged and settled.
    pub expired_billed: u64,
    /// Expired runs whose logged last slice did not settle.
    pub expired_settle_failures: u64,
    /// Whether the line pass read a whole batch, so more lines may wait.
    pub lines_full: bool,
    /// The code of the first refusal in either pass.
    pub refused: Option<ErrorCode>,
    /// Where the next line pass resumes.
    pub resume: Option<Resume>,
}

/// Both halves of a tick in one pass, as a test drives one: the daemon runs
/// [`bill_expired_runs`] on `sweeper:expiry`'s clock and [`settle_lines`] on
/// the billing relay's.
///
/// Bills the last slice of every run expired past [`GRACE`], then settles one
/// batch of unsettled lines after `after` through
/// [`afd_billing::ledger::resettle`]. `entropy` mints the lines' identifiers
/// and `now` stamps them.
///
/// # Errors
/// Reports a datastore that would not answer a read. A biller that refuses one
/// settlement is counted, not raised: the rest of the batch still runs, and
/// the next pass retries what failed.
#[cfg(feature = "test-util")]
pub async fn sweep(
    database: &Db,
    biller: &dyn Biller,
    entropy: &Entropy,
    now: Timestamp,
    after: Option<&Resume>,
) -> Result<BillingSwept> {
    let expired = bill_expired_runs(database, biller, entropy, now).await?;
    let lines = settle_lines(database, biller, after).await?;
    Ok(BillingSwept {
        resettled: lines.resettled,
        settle_failures: lines.settle_failures,
        expired_billed: expired.billed,
        expired_settle_failures: expired.settle_failures,
        lines_full: lines.full,
        refused: expired.refused.or(lines.refused),
        resume: lines.resume,
    })
}

/// Bills the last slice of every run expired past [`GRACE`]: what
/// `sweeper:expiry` runs every `maintenance.expired_work.interval_seconds`.
///
/// # Errors
/// Reports a datastore that would not answer the candidate read.
pub async fn bill_expired_runs(
    database: &Db,
    biller: &dyn Biller,
    entropy: &Entropy,
    now: Timestamp,
) -> Result<ExpirySwept> {
    let mut swept = ExpirySwept::default();
    bill_expired(database, biller, entropy, now, &mut swept).await?;
    Ok(swept)
}

/// Settles one batch of unsettled lines after `after`: the billing relay's
/// pass.
///
/// # Errors
/// Reports a datastore that would not answer the line read.
pub async fn settle_lines(
    database: &Db,
    biller: &dyn Biller,
    after: Option<&Resume>,
) -> Result<LinesSwept> {
    let lines = afd_billing::ledger::resettle(database, biller, after).await?;
    Ok(LinesSwept {
        resettled: lines.settled,
        settle_failures: lines.failed,
        full: lines.full,
        refused: lines.refused,
        resume: lines.resume,
    })
}

/// Logs and settles the last slice of every run that ended unreported.
///
/// A run that expired, or was reassigned, still owes the run time between
/// its last renewal and the deadline it was granted. Each run is billed in
/// its own transaction, which advances its cursor to that deadline and logs
/// the runtime line together, so a second pass, or a late report racing it,
/// measures nothing more. A reported run is never a candidate: its report
/// billed it. Counts what settled and what did not on `swept`.
async fn bill_expired(
    database: &Db,
    biller: &dyn Biller,
    entropy: &Entropy,
    now: Timestamp,
    swept: &mut ExpirySwept,
) -> Result<()> {
    let candidates: Vec<Uuid7> = {
        let mut connection = database.acquire().await?;
        sqlx::query_scalar(SELECT_UNBILLED_EXPIRED_RUNS)
            .bind(SETTLED_STATUSES.to_vec())
            .bind(GRACE)
            .bind(BATCH_LIMIT)
            .fetch_all(&mut *connection)
            .await
            .map_err(query(CONTEXT_EXPIRED))?
    };
    for run in candidates {
        let Some(settlement) = log_last_slice(database, entropy, &run, now).await? else {
            continue;
        };
        match biller.settle(&settlement).await {
            Ok(_receipt) => swept.billed += 1,
            Err(fault) => {
                swept.settle_failures += 1;
                swept.refused(run.as_str(), &fault);
            }
        }
    }
    Ok(())
}

/// Advances `run`'s cursor to its deadline and logs the run time between,
/// in one transaction; `None` when another pass or a report got there first.
async fn log_last_slice(
    database: &Db,
    entropy: &Entropy,
    run: &Uuid7,
    now: Timestamp,
) -> Result<Option<afd_billing::biller::Settlement>> {
    let mut connection = database.acquire().await?;
    let mut transaction = connection.begin().await.map_err(query(CONTEXT_EXPIRED))?;
    let Some(row) = sqlx::query(BILL_EXPIRED_SLICE)
        .bind(run)
        .bind(SETTLED_STATUSES.to_vec())
        .bind(afd_billing::nanos::MS_PER_SEC)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(query(CONTEXT_EXPIRED))?
    else {
        return Ok(None);
    };
    let billed = ExpiredRun::from_row(&row).map_err(query(CONTEXT_EXPIRED))?;
    let slice = SliceColumns::from_row(&row).map_err(query(CONTEXT_EXPIRED))?;
    // A posture this build cannot name is not guessed at: pricing the slice as
    // the platform's would bill a tenant's own key at the platform's rates. The
    // run is left as it was (the transaction drops unwritten, so its cursor
    // does not move), named once, and the pass moves on to the next run.
    let Some(posture) = Posture::parse(&billed.posture) else {
        skipped(
            Unreadable::Posture,
            afd_billing::sql::work::FLEET_RUN,
            &billed.posture,
        );
        return Ok(None);
    };
    let pricing = Pricing::runtime_only(posture, billed.provider, billed.model);
    // A matched row is the probe row, and the slice is read off it, so it is
    // always present here. A draft's run bills its setup work, not a fleet.
    let settlement = if billed.draft_id.is_some() {
        slice
            .into_draft_slice()
            .map_or(Ok(None), |slice| pricing.draft_settlement(run, slice, &[]))?
    } else {
        slice
            .into_slice()
            .map_or(Ok(None), |slice| pricing.settlement(run, slice, &[]))?
    };
    usage::log_measured(&mut transaction, entropy, now, settlement.as_ref()).await?;
    transaction.commit().await.map_err(query(CONTEXT_EXPIRED))?;
    Ok(settlement)
}

/// What an expired run's last slice is priced under, by column name.
#[derive(Debug, sqlx::FromRow)]
struct ExpiredRun {
    /// Set on a draft-owned run.
    draft_id: Option<Uuid7>,
    posture: String,
    provider: String,
    model: String,
}
