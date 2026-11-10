//! What a delivery cycle did to an obligation: counted, delivered, or given up.
//!
//! Split from `obligation.rs` at the file cap, along the line its own note
//! draws between OWING an answer and DELIVERING one. Everything here runs on
//! the holder's side, once per cycle, and each write matches the holder's
//! token, so a holder whose claim lapsed changes nothing.

use afd_db::Db;

use super::{Claimed, sql};
use crate::error::Result;

/// Statement name, for the context a failure carries.
const CONTEXT_STAMP: &str = "stamp delivered";

/// Statement name, for the context a cycle-start failure carries.
const CONTEXT_COUNT: &str = "count delivery attempt";

/// Statement name, for the context an abandon failure carries.
const CONTEXT_ABANDON: &str = "abandon obligation";

/// Why an answer was given up on.
///
/// A closed set whose spelling is what `abandon_reason` stores, so an operator
/// filtering on it and the code writing it cannot drift apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbandonReason {
    /// The destination refused it and no retry changes that: a deleted
    /// channel, a removed bot, an address naming nowhere. The poster's own
    /// failure event names which.
    Refused,
    /// Every delivery cycle it was allowed ended retryable.
    CyclesExhausted,
    /// Its stored connector id names no connector, so no poster could deliver
    /// it: a connector removed from the catalogue, or an edit made out of band.
    Unaddressable,
}

impl AbandonReason {
    /// The stored spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Refused => "refused",
            Self::CyclesExhausted => "cycles_exhausted",
            Self::Unaddressable => "unaddressable",
        }
    }
}

/// Record that this holder has taken the obligation for a delivery cycle,
/// and hold the claim for a fresh [`CLAIM_TTL`](super::CLAIM_TTL) from now.
///
/// Answers the count this call produced, or `None` when the row was already
/// delivered or another holder has it and nothing was counted.
///
/// Called at the START of the cycle, so the number survives the cycle failing,
/// and so the post that follows runs under a claim this call just proved.
///
/// # Errors
/// Reports a database that would not answer. A caller must log that and DELIVER
/// ANYWAY: the answer is owed to a person and bookkeeping is not.
pub async fn count_attempt(database: &Db, job: &Claimed) -> Result<Option<i64>> {
    let mut connection = database.acquire().await?;
    sqlx::query_scalar(sql::COUNT_ATTEMPT)
        .bind(&job.id)
        .bind(&job.token)
        .bind(super::CLAIM_TTL)
        .fetch_optional(&mut *connection)
        .await
        .map_err(crate::error::query(CONTEXT_COUNT))
}

/// Record that a destination accepted this answer; `false` when the claim
/// lapsed and the stamp changed nothing.
///
/// # Errors
/// Reports a database that would not answer.
pub async fn stamp_delivered(database: &Db, job: &Claimed) -> Result<bool> {
    let mut connection = database.acquire().await?;
    let stamped = sqlx::query(sql::STAMP_DELIVERED)
        .bind(&job.id)
        .bind(&job.token)
        .execute(&mut *connection)
        .await
        .map_err(crate::error::query(CONTEXT_STAMP))?;
    Ok(stamped.rows_affected() == 1)
}

/// Record that nobody can take this answer, so no claim offers it again.
///
/// Answers the attempt count when THIS call stamped the row, and `None` when
/// the row was already delivered or abandoned or the claim lapsed — so a
/// caller announces an abandonment once.
///
/// # Errors
/// Reports a database that would not answer.
pub async fn abandon(database: &Db, job: &Claimed, reason: AbandonReason) -> Result<Option<i64>> {
    let mut connection = database.acquire().await?;
    sqlx::query_scalar(sql::ABANDON)
        .bind(&job.id)
        .bind(&job.token)
        .bind(reason.as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(crate::error::query(CONTEXT_ABANDON))
}
