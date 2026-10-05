//! The checks a create passes before its row is written.
//!
//! Split from [`super`] so the create reads as its three steps — admit, insert,
//! commit — and so each refusal sits beside the statement that decides it.
//! Every check runs on the create's own transaction, after the fleet's row is
//! taken, so no concurrent create can slip between a count and the insert.

use afd_core::id::Uuid7;
use sqlx::{PgConnection, Row as _};

use super::{CONTEXT_WRITE, NewSchedule, Refused};
use crate::error::{self, Result};
use crate::model::{FLEET_SCHEDULES_MAX, MAX_SCHEDULES_PER_FLEET, Source};
use crate::sql;

/// Why `new` may not be created, or `None` when it may.
///
/// The fleet's own cap is read before the whole fleet's, because it is the
/// tighter bound and the one a fleet can act on by removing its own.
///
/// # Errors
/// Reports a statement that failed.
pub(super) async fn refusal(
    transaction: &mut PgConnection,
    workspace: &Uuid7,
    new: &NewSchedule<'_>,
) -> Result<Option<Refused>> {
    let fleet = new.fleet.as_str();
    let in_workspace = sqlx::query(sql::FLEET_IN_WORKSPACE)
        .bind(fleet)
        .bind(workspace.as_str())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(error::query(CONTEXT_WRITE))?;
    if in_workspace.is_none() {
        return Ok(Some(Refused::NoSuchFleet));
    }

    let _locked = sqlx::query(sql::LOCK_FLEET)
        .bind(fleet)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(error::query(CONTEXT_WRITE))?;

    if new.source == Source::Fleet
        && count(transaction, sql::COUNT_FOR_SOURCE, fleet, Some(new.source)).await?
            >= FLEET_SCHEDULES_MAX
    {
        return Ok(Some(Refused::FleetCapReached));
    }
    if count(transaction, sql::COUNT_FOR_FLEET, fleet, None).await? >= MAX_SCHEDULES_PER_FLEET {
        return Ok(Some(Refused::TooMany));
    }

    let duplicate = sqlx::query(sql::SOURCE_KEY_EXISTS)
        .bind(fleet)
        .bind(new.source_key)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(error::query(CONTEXT_WRITE))?;
    Ok(duplicate.map(|_held| Refused::DuplicateKey))
}

/// One count statement's answer, bound to the fleet and, for a per-source
/// count, the source's stored word.
async fn count(
    transaction: &mut PgConnection,
    statement: &'static str,
    fleet: &str,
    source: Option<Source>,
) -> Result<usize> {
    let query = sqlx::query(statement).bind(fleet);
    let query = match source {
        Some(source) => query.bind(source.as_str()),
        None => query,
    };
    let held: i64 = query
        .fetch_one(&mut *transaction)
        .await
        .map_err(error::query(CONTEXT_WRITE))?
        .try_get(0)
        .map_err(error::query(CONTEXT_WRITE))?;
    Ok(usize::try_from(held).unwrap_or(usize::MAX))
}
