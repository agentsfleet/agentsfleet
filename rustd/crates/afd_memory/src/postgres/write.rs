//! The Postgres store's writes: a push's upsert with its sweep and cap, and
//! the operator's forget.

use afd_wire::memory::{MAX_ENTRIES_PER_FLEET, MemoryDelta, PINNED_CATEGORY};
use jiff::Timestamp;

use super::{Failure, PgStore};
use crate::error::Result;
use crate::error::detail::FORGET_FAILED;
use crate::record::{Housekept, Owner};
use crate::sql;
use crate::window::{DAILY_CATEGORY, DAILY_RETENTION};

/// Statement names, for the context a query failure carries.
const CONTEXT_UPSERT: &str = "memory upsert";
const CONTEXT_EVICT: &str = "memory cap evict";
const CONTEXT_SWEEP: &str = "memory daily sweep";

impl PgStore {
    /// Writes each entry under `owner.fleet` at `now`, replacing what its key
    /// held, then sweeps expired scratch notes and evicts past the cap, in one
    /// transaction.
    ///
    /// The sweep runs BEFORE the cap deliberately: an already-expired `daily` row
    /// must not occupy a cap slot during victim selection, or eviction deletes a
    /// durable row in the doomed row's place.
    ///
    /// # Errors
    /// The database cannot take the write; none of it lands.
    pub async fn upsert(
        &self,
        owner: Owner<'_>,
        entries: &[&MemoryDelta<'_>],
        now: Timestamp,
    ) -> Result<Housekept> {
        self.as_memory_role(Failure::Runner(CONTEXT_UPSERT), async |connection| {
            for delta in entries {
                let row_id = self.entropy.uuid7(now)?;
                sqlx::query(sql::UPSERT_ENTRY)
                    .bind(row_id.as_str())
                    .bind(delta.key.as_ref())
                    .bind(delta.content.as_ref())
                    .bind(delta.category.as_ref())
                    .bind(owner.fleet.as_str())
                    .bind(owner.workspace.as_str())
                    .bind(delta.visibility.is_workspace())
                    .bind(jiff_sqlx::ToSqlx::to_sqlx(now))
                    .execute(&mut *connection)
                    .await
                    .map_err(|source| Failure::Runner(CONTEXT_UPSERT).raise(source))?;
            }
            let swept = sqlx::query(sql::DELETE_AGED_IN_CATEGORY)
                .bind(owner.fleet.as_str())
                .bind(DAILY_CATEGORY)
                .bind(jiff_sqlx::ToSqlx::to_sqlx(afd_core::clock::before(
                    now,
                    DAILY_RETENTION,
                )))
                .execute(&mut *connection)
                .await
                .map_err(|source| Failure::Runner(CONTEXT_SWEEP).raise(source))?
                .rows_affected();
            let evicted = sqlx::query(sql::EVICT_PAST_CAP)
                .bind(owner.fleet.as_str())
                .bind(i64::try_from(MAX_ENTRIES_PER_FLEET).unwrap_or(i64::MAX))
                .bind(PINNED_CATEGORY)
                .execute(&mut *connection)
                .await
                .map_err(|source| Failure::Runner(CONTEXT_EVICT).raise(source))?
                .rows_affected();
            Ok(Housekept { swept, evicted })
        })
        .await
    }

    /// Removes the fleet's own entry under `key`; whether there was one.
    ///
    /// The transaction commits BEFORE the verdict is read: answering from inside
    /// it would drop the transaction on the not-found path and roll back nothing,
    /// but on the deleted path it would roll the delete back. Another fleet's
    /// entry under the same key is never touched.
    ///
    /// # Errors
    /// The database cannot answer.
    pub async fn forget(&self, owner: Owner<'_>, key: &str) -> Result<bool> {
        let failure = Failure::Operator(FORGET_FAILED);
        self.as_memory_role(failure, async |connection| {
            let forgotten = sqlx::query(sql::DELETE_ENTRY_BY_KEY)
                .bind(owner.fleet.as_str())
                .bind(key)
                .fetch_optional(&mut *connection)
                .await
                .map_err(|source| failure.raise(source))?;
            Ok(forgotten.is_some())
        })
        .await
    }
}
