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
        && Tally::Source(new.source).count(transaction, fleet).await? >= FLEET_SCHEDULES_MAX
    {
        return Ok(Some(Refused::FleetCapReached));
    }
    if Tally::Fleet.count(transaction, fleet).await? >= MAX_SCHEDULES_PER_FLEET {
        return Ok(Some(Refused::TooMany));
    }

    // A key the store mints from the new row's own id cannot be held yet.
    let Some(key) = new.source_key else {
        return Ok(None);
    };
    let duplicate = sqlx::query(sql::SOURCE_KEY_EXISTS)
        .bind(fleet)
        .bind(key)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(error::query(CONTEXT_WRITE))?;
    Ok(duplicate.map(|_held| Refused::DuplicateKey))
}

/// Which of a fleet's schedules a count covers.
///
/// The statement and its binds are picked together, so a per-source count
/// cannot run without the source it filters on.
#[derive(Debug, Clone, Copy)]
enum Tally {
    /// Every schedule the fleet holds.
    Fleet,
    /// The schedules of one source.
    Source(Source),
}

impl Tally {
    /// How many of `fleet`'s schedules this tally covers.
    async fn count(self, transaction: &mut PgConnection, fleet: &str) -> Result<usize> {
        let query = match self {
            Self::Fleet => sqlx::query(sql::COUNT_FOR_FLEET).bind(fleet),
            Self::Source(source) => sqlx::query(sql::COUNT_FOR_SOURCE)
                .bind(fleet)
                .bind(source.as_str()),
        };
        let held: i64 = query
            .fetch_one(&mut *transaction)
            .await
            .map_err(error::query(CONTEXT_WRITE))?
            .try_get(0)
            .map_err(error::query(CONTEXT_WRITE))?;
        Ok(usize::try_from(held).unwrap_or(usize::MAX))
    }
}
