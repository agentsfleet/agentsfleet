//! The Postgres store's writes: a push's upsert with its sweep and cap, a
//! flip's newer-row-wins import, and the operator's forget.

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_wire::memory::{MAX_ENTRIES_PER_FLEET, MemoryDelta, PINNED_CATEGORY};

use super::{Failure, PgStore, sql};
use crate::error::Result;
use crate::error::detail::FORGET_FAILED;
use crate::record::{Housekept, Owner, Record};
use crate::window::{DAILY_CATEGORY, DAILY_RETENTION_MS};

/// Statement names, for the context a query failure carries.
const CONTEXT_UPSERT: &str = "memory upsert";
const CONTEXT_EVICT: &str = "memory cap evict";
const CONTEXT_SWEEP: &str = "memory daily sweep";
const CONTEXT_IMPORT: &str = "memory import";

/// Upserts `entries`, then sweeps and caps, in one transaction.
///
/// The sweep runs BEFORE the cap deliberately: an already-expired `daily` row
/// must not occupy a cap slot during victim selection, or eviction deletes a
/// durable row in the doomed row's place.
pub(super) async fn upsert(
    store: &PgStore,
    owner: Owner<'_>,
    entries: &[&MemoryDelta<'_>],
    now: UnixMillis,
) -> Result<Housekept> {
    store
        .as_memory_role(Failure::Runner(CONTEXT_UPSERT), async |connection| {
            for delta in entries {
                let row_id = store.entropy.uuid7(now)?;
                sqlx::query(sql::UPSERT_ENTRY)
                    .bind(row_id.as_str())
                    .bind(delta.key.as_ref())
                    .bind(delta.content.as_ref())
                    .bind(delta.category.as_ref())
                    .bind(owner.fleet.as_str())
                    .bind(owner.workspace.as_str())
                    .bind(delta.visibility.is_workspace())
                    .bind(now.as_millis())
                    .execute(&mut *connection)
                    .await
                    .map_err(|source| Failure::Runner(CONTEXT_UPSERT).raise(source))?;
            }
            let swept = sqlx::query(sql::DELETE_AGED_IN_CATEGORY)
                .bind(owner.fleet.as_str())
                .bind(DAILY_CATEGORY)
                .bind(now.as_millis().saturating_sub(DAILY_RETENTION_MS))
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

/// Writes `record` as it stands, unless a newer row is already here.
pub(super) async fn import(store: &PgStore, workspace: &Uuid7, record: &Record) -> Result<()> {
    let failure = Failure::Runner(CONTEXT_IMPORT);
    let row_id = store
        .entropy
        .uuid7(UnixMillis::from_millis(record.created_at_ms))?;
    store
        .as_memory_role(failure, async |connection| {
            sqlx::query(sql::IMPORT_ENTRY)
                .bind(row_id.as_str())
                .bind(record.key.as_str())
                .bind(record.content.as_str())
                .bind(record.category.as_str())
                .bind(record.fleet.as_str())
                .bind(workspace.as_str())
                .bind(record.visibility.is_workspace())
                .bind(record.created_at_ms)
                .bind(record.updated_at_ms)
                .execute(&mut *connection)
                .await
                .map_err(|source| failure.raise(source))?;
            Ok(())
        })
        .await
}

/// Removes the fleet's own entry under `key`; whether there was one.
///
/// The transaction commits BEFORE the verdict is read: answering from inside
/// it would drop the transaction on the not-found path and roll back nothing,
/// but on the deleted path it would roll the delete back.
pub(super) async fn forget(store: &PgStore, owner: Owner<'_>, key: &str) -> Result<bool> {
    let failure = Failure::Operator(FORGET_FAILED);
    store
        .as_memory_role(failure, async |connection| {
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
