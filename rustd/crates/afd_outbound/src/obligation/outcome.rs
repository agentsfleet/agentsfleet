//! What a delivery cycle did to an obligation: counted, delivered, or given up.
//!
//! Split from `obligation.rs` at the file cap, along the line its own note
//! draws between OWING an answer and DELIVERING one. Everything here runs on
//! the worker's side of the queue, once per cycle, and each write is guarded so
//! a duplicate queue entry for the same answer changes nothing.

use afd_core::clock::UnixMillis;
use afd_db::Db;

use super::sql;
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
    /// Its stored connector id names no connector, so no queue entry could
    /// deliver it: a connector removed from the catalogue, or an edit made
    /// out of band.
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

/// Record that a worker has taken this obligation for a delivery cycle.
///
/// Answers the count this call produced, or `None` when the row was already
/// delivered and nothing was counted — which is what a duplicate queue entry
/// for an answer somebody already received looks like from here.
///
/// Called at the START of the cycle, so the number survives the cycle failing.
/// That makes it best-effort in one direction and only one: a process that dies
/// between this write and the delivery has counted a cycle that produced
/// nothing, and a process that dies before it has delivered a cycle it never
/// counted. Neither can move `delivered_at`, which is the fact anything
/// downstream acts on.
///
/// # Errors
/// Reports a database that would not answer. A caller must log that and DELIVER
/// ANYWAY: the answer is owed to a person and bookkeeping is not.
pub async fn count_attempt(
    database: &Db,
    fleet_id: &str,
    event_id: &str,
    now: UnixMillis,
) -> Result<Option<i64>> {
    let mut connection = database.acquire().await?;
    let counted: Option<(i64,)> = sqlx::query_as(sql::COUNT_ATTEMPT)
        .bind(fleet_id)
        .bind(event_id)
        .bind(now.as_millis())
        .fetch_optional(&mut *connection)
        .await
        .map_err(crate::error::query(CONTEXT_COUNT))?;
    Ok(counted.map(|(count,)| count))
}

/// Record that a destination accepted this answer.
///
/// # Errors
/// Reports a database that would not answer.
pub async fn stamp_delivered(
    database: &Db,
    fleet_id: &str,
    event_id: &str,
    now: UnixMillis,
) -> Result<()> {
    let mut connection = database.acquire().await?;
    sqlx::query(sql::STAMP_DELIVERED)
        .bind(fleet_id)
        .bind(event_id)
        .bind(now.as_millis())
        .execute(&mut *connection)
        .await
        .map_err(crate::error::query(CONTEXT_STAMP))?;
    Ok(())
}

/// Record that nobody can take this answer, so no scan offers it again.
///
/// Answers the attempt count when THIS call stamped the row, and `None` when
/// the row was already delivered or abandoned — so a caller announces an
/// abandonment once, whatever duplicate entries reach it.
///
/// # Errors
/// Reports a database that would not answer.
pub async fn abandon(
    database: &Db,
    fleet_id: &str,
    event_id: &str,
    reason: AbandonReason,
    now: UnixMillis,
) -> Result<Option<i64>> {
    let mut connection = database.acquire().await?;
    let stamped: Option<(i64,)> = sqlx::query_as(sql::ABANDON)
        .bind(fleet_id)
        .bind(event_id)
        .bind(now.as_millis())
        .bind(reason.as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(crate::error::query(CONTEXT_ABANDON))?;
    Ok(stamped.map(|(attempts,)| attempts))
}
