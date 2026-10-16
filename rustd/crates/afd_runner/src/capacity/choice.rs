//! Durable alternation: the successful lease write owns the next preference.

use afd_core::id::Uuid7;
use afd_wire::lease::{AcceptedKinds, LeaseKind};
use sqlx::{Postgres, Transaction};

use super::{Capacity, sql};
use crate::{Result, Runners, error::query};

/// An optimistic choice, rechecked while holding the physical host row.
#[derive(Debug, Clone, Copy)]
pub struct Ticket {
    revision: i64,
    preferred: LeaseKind,
    accepted: AcceptedKinds,
}

impl Ticket {
    /// The preferred eligible kind, followed by the other accepted kind.
    pub fn order(self) -> impl Iterator<Item = LeaseKind> {
        [self.preferred, self.preferred.other()]
            .into_iter()
            .filter(move |&kind| self.accepted.contains(kind))
    }

    /// A concurrent placement invalidates this choice before another insert.
    #[must_use]
    pub const fn permits(self, capacity: Capacity, kind: LeaseKind) -> bool {
        self.revision == capacity.claim_revision && self.accepted.contains(kind)
    }
}

impl Runners {
    /// Read the host's next placement choice without reserving a worker.
    ///
    /// # Errors
    /// Preserves datastore and column failures; a removed host has no offer.
    pub async fn claim_offer(
        &self,
        runner: &Uuid7,
        accepted: AcceptedKinds,
    ) -> Result<Option<Ticket>> {
        let row: Option<(i64, Option<String>)> = sqlx::query_as(sql::CHOICE)
            .bind(runner.as_str())
            .fetch_optional(&mut *self.pool().acquire().await?)
            .await
            .map_err(query(sql::CONTEXT))?;
        Ok(row.map(|(revision, last)| Ticket {
            revision,
            preferred: if last.as_deref() == Some(LeaseKind::Authoring.as_str()) {
                LeaseKind::Fleet
            } else {
                LeaseKind::Authoring
            },
            accepted,
        }))
    }
}

/// Advance only alongside a successful lease insertion in the same transaction.
///
/// # Errors
/// Preserves datastore failures, including a revision beyond its integer range.
pub async fn record(
    transaction: &mut Transaction<'_, Postgres>,
    runner: &Uuid7,
    capacity: Capacity,
    kind: LeaseKind,
) -> Result<bool> {
    let result = sqlx::query(sql::RECORD_CHOICE)
        .bind(runner.as_str())
        .bind(capacity.claim_revision)
        .bind(kind.as_str())
        .execute(&mut **transaction)
        .await
        .map_err(query(sql::CONTEXT))?;
    Ok(result.rows_affected() == 1)
}
