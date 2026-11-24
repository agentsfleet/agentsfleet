//! Optimistic placement: a host's run version, read before the host lock and
//! rechecked under it, so a stale poll cannot place on a host another
//! assignment already changed.

use af_core::id::Uuid7;
use sqlx::{Postgres, Transaction};

use super::{Capacity, sql};
use crate::{Result, Runners, error::query};

/// An optimistic choice, rechecked while holding the physical host row.
#[derive(Debug, Clone, Copy)]
pub struct Ticket {
    revision: i64,
}

impl Ticket {
    /// A concurrent placement invalidates this choice before another insert.
    #[must_use]
    pub const fn permits(self, capacity: Capacity) -> bool {
        self.permits_revision(capacity.run_version)
    }

    /// [`Self::permits`] against a host row's revision, for a lock that took
    /// no census.
    #[must_use]
    pub const fn permits_revision(self, run_version: i64) -> bool {
        self.revision == run_version
    }
}

impl Runners {
    /// Read the host's run version without reserving a worker.
    ///
    /// # Errors
    /// Preserves datastore and column failures; a removed host has no assignment.
    pub async fn assignment_ticket(&self, runner: &Uuid7) -> Result<Option<Ticket>> {
        let revision: Option<i64> = sqlx::query_scalar(sql::CHOICE)
            .bind(runner)
            .fetch_optional(&mut *self.pool().acquire().await?)
            .await
            .map_err(query(sql::CONTEXT))?;
        Ok(revision.map(|revision| Ticket { revision }))
    }
}

/// Advance only alongside a successful run insertion in the same transaction.
///
/// # Errors
/// Preserves datastore failures, including a revision beyond its integer range.
pub async fn record(
    transaction: &mut Transaction<'_, Postgres>,
    runner: &Uuid7,
    capacity: Capacity,
) -> Result<bool> {
    let result = sqlx::query(sql::RECORD_CHOICE)
        .bind(runner)
        .bind(capacity.run_version)
        .execute(&mut **transaction)
        .await
        .map_err(query(sql::CONTEXT))?;
    Ok(result.rows_affected() == 1)
}
