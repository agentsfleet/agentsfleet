//! Durable alternation: the successful run write owns the next preference.

use af_core::id::Uuid7;
use af_wire::run::{AcceptedKinds, RunKind};
use sqlx::{Postgres, Transaction};

use super::{Capacity, sql};
use crate::{Result, Runners, error::query};

/// An optimistic choice, rechecked while holding the physical host row.
#[derive(Debug, Clone, Copy)]
pub struct Ticket {
    revision: i64,
    preferred: RunKind,
    accepted: AcceptedKinds,
}

impl Ticket {
    /// The preferred eligible kind, followed by the other accepted kind.
    pub fn order(self) -> impl Iterator<Item = RunKind> {
        [self.preferred, self.preferred.other()]
            .into_iter()
            .filter(move |&kind| self.accepted.contains(kind))
    }

    /// A concurrent placement invalidates this choice before another insert.
    #[must_use]
    pub const fn permits(self, capacity: Capacity, kind: RunKind) -> bool {
        self.permits_revision(capacity.run_version, kind)
    }

    /// [`Self::permits`] against a host row's revision, for a lock that took
    /// no census.
    #[must_use]
    pub const fn permits_revision(self, run_version: i64, kind: RunKind) -> bool {
        self.revision == run_version && self.accepted.contains(kind)
    }
}

impl Runners {
    /// Read the host's next placement choice without reserving a worker.
    ///
    /// # Errors
    /// Preserves datastore and column failures; a removed host has no assignment.
    pub async fn assignment_ticket(
        &self,
        runner: &Uuid7,
        accepted: AcceptedKinds,
    ) -> Result<Option<Ticket>> {
        let row: Option<(i64, Option<String>)> = sqlx::query_as(sql::CHOICE)
            .bind(runner)
            .fetch_optional(&mut *self.pool().acquire().await?)
            .await
            .map_err(query(sql::CONTEXT))?;
        Ok(row.map(|(revision, last)| Ticket {
            revision,
            preferred: if last.as_deref() == Some(RunKind::Authoring.as_str()) {
                RunKind::Fleet
            } else {
                RunKind::Authoring
            },
            accepted,
        }))
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
    kind: RunKind,
) -> Result<bool> {
    let result = sqlx::query(sql::RECORD_CHOICE)
        .bind(runner)
        .bind(capacity.run_version)
        .bind(kind.as_str())
        .execute(&mut **transaction)
        .await
        .map_err(query(sql::CONTEXT))?;
    Ok(result.rows_affected() == 1)
}
