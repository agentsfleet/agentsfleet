//! The billing sweeper's pass over setup chat: release the holds of attempts
//! that ended without releasing them.
//!
//! A report settles and releases after its transaction commits (billing §11),
//! so a crash or a biller that could not answer leaves a settled attempt's hold
//! open, and an open hold still counts against the tenant's balance. Unsettled
//! lines are the line sweep's (`afd_billing::ledger::resettle`), which covers
//! every work type; this pass only frees what finished attempts still hold.
//!
//! An attempt awaiting accounting keeps its hold: its usage is not known yet,
//! and the replay that learns it settles and releases.

use afd_billing::biller::{Biller, Release, WorkKey};
use afd_core::id::Uuid7;
use afd_db::Db;
use afd_wire::authoring::Outcome;

use crate::{Result, draft, error::raise::query, lease::Status};

/// What the finished-holds statement was doing, for [`query`].
const CONTEXT: &str = "billing sweep finished setup chat holds";

/// Holds one pass reads; bounds the pass's work.
const BATCH_LIMIT: i64 = 500;

/// `$1` the attempt work type, `$2` the settled status, `$3` the grace,
/// `$4` the batch limit.
const SELECT_FINISHED_HOLDS: &str = include_str!("finished_holds.sql");

/// What one pass released.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Released {
    /// Holds of finished attempts this pass released.
    pub released: u64,
    /// Holds it could not release; they wait for the next pass.
    pub release_failures: u64,
}

/// A finished attempt's open hold, by column name.
#[derive(sqlx::FromRow)]
struct FinishedHold {
    attempt_id: String,
    /// `None` once the attempt row is gone.
    outcome: Option<String>,
}

/// One pass: release every open hold whose attempt settled more than the
/// line sweep's grace ago, or whose attempt no longer exists.
///
/// # Errors
/// Reports a datastore that would not answer the read. A biller that refuses
/// one release is counted, not raised: the rest of the batch still runs.
pub async fn release_finished(database: &Db, biller: &dyn Biller) -> Result<Released> {
    let finished: Vec<FinishedHold> = sqlx::query_as(SELECT_FINISHED_HOLDS)
        .bind(afd_billing::sql::work::AUTHORING_ATTEMPT)
        .bind(Status::Settled.as_str())
        .bind(afd_billing::ledger::GRACE)
        .bind(BATCH_LIMIT)
        .fetch_all(&mut *database.acquire().await?)
        .await
        .map_err(query(CONTEXT))?;
    let mut pass = Released::default();
    for hold in finished {
        let work = WorkKey::AuthoringAttempt(Uuid7::parse(&hold.attempt_id)?);
        match biller
            .release(&work, release(hold.outcome.as_deref()))
            .await
        {
            Ok(()) => pass.released += 1,
            Err(_fault) => pass.release_failures += 1,
        }
    }
    Ok(pass)
}

/// Why the hold is released: a completed attempt completed; anything else,
/// including a vanished row, ended without completing.
fn release(outcome: Option<&str>) -> Release {
    match outcome.map(draft::word::<Outcome>) {
        Some(Ok(Outcome::Completed)) => Release::Completed,
        _ => Release::Failed,
    }
}

#[cfg(test)]
mod tests;
