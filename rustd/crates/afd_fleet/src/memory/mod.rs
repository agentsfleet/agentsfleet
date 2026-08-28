//! Durable fleet memory: what a run remembers between runs.
//!
//! Two halves that share one store — [`window`] decides what a run is SEEDED
//! with, and this module writes what it learned.
//!
//! # The role switch is a transaction, not a pair of statements
//!
//! `memory.memory_entries` is written as `memory_runtime`, a role the api pool
//! does not otherwise hold. `helpers.zig` spends `SET ROLE` and `RESET ROLE` on
//! it, with a `defer` between them and a documented hazard: if the reset fails
//! the connection is still running as `memory_runtime` and the pool has to
//! discard it.
//!
//! `SET LOCAL ROLE` inside a transaction has no such failure. Postgres restores
//! the role at COMMIT or ROLLBACK — including the rollback a dropped
//! transaction performs — so there is no reset to fail, no `defer` to forget,
//! and no way for a connection to return to the pool with the wrong role. The
//! hazard is not handled better; it is gone.

pub mod operator;
pub mod page;
pub mod sql;
pub mod window;

use afd_core::clock::UnixMillis;
use afd_core::id::{ENTROPY_LEN, Uuid7};
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_wire::memory::{MAX_ENTRIES_PER_FLEET, MemoryDelta};
use sqlx::{Acquire as _, Row as _};

use crate::error::{Result, query};

/// Statement name, for the context a query failure carries.
const CONTEXT_LIST: &str = "memory list";

/// Statement name, for the context a query failure carries.
const CONTEXT_UPSERT: &str = "memory upsert";

/// Statement name, for the context a query failure carries.
const CONTEXT_EVICT: &str = "memory cap evict";

/// Statement name, for the context a query failure carries.
const CONTEXT_SWEEP: &str = "memory daily sweep";

/// How long a `daily` entry survives before a capture sweeps it.
///
/// Seventy-two hours. Scratch notes only — every other category is exempt
/// because the sweep binds its category as a parameter.
pub const DAILY_RETENTION_MS: i64 = 72 * 60 * 60 * 1_000;

/// The durable memory store, over the api-role pool.
///
/// Its own type rather than another verb on [`Leases`](crate::lease::Leases):
/// the tables are a different schema under a different role, and a lease store
/// that could write memory would be a lease store that needs the role.
#[derive(Debug, Clone)]
pub struct Memories {
    database: Db,
    entropy: Entropy,
}

/// What one capture wrote, and what it declined to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Captured {
    /// Entries upserted.
    pub stored: usize,
    /// Entries refused for shape — an empty or oversized field.
    pub skipped: usize,
    /// Entries beyond the push byte cap, which end the batch.
    pub truncated: usize,
    /// Rows the retention sweep removed.
    pub swept: u64,
    /// Rows evicted to bring the fleet back under its cap.
    pub evicted: u64,
}

impl Memories {
    /// A store reading and writing through `database`.
    #[must_use]
    pub const fn new(database: Db, entropy: Entropy) -> Self {
        Self { database, entropy }
    }

    /// Every entry for `fleet_id`, newest first.
    ///
    /// Unbounded by design — [`window::select`] is what bounds the reply, and
    /// bounding here instead would make the budget a property of the statement
    /// rather than of the caller that has to spend it.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn list(&self, fleet_id: &Uuid7) -> Result<Vec<MemoryDelta<'static>>> {
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_LIST))?;
        sqlx::query(sql::ASSUME_MEMORY_ROLE)
            .execute(&mut *transaction)
            .await
            .map_err(query(CONTEXT_LIST))?;

        let rows = sqlx::query(sql::SELECT_ALL_FOR_FLEET)
            .bind(fleet_id.as_str())
            .fetch_all(&mut *transaction)
            .await
            .map_err(query(CONTEXT_LIST))?;

        // Collected before the commit, because the rows borrow the transaction.
        let entries = rows
            .into_iter()
            .map(|row| {
                Ok(MemoryDelta {
                    key: row
                        .try_get::<String, _>(0)
                        .map_err(query(CONTEXT_LIST))?
                        .into(),
                    content: row
                        .try_get::<String, _>(1)
                        .map_err(query(CONTEXT_LIST))?
                        .into(),
                    category: row
                        .try_get::<String, _>(2)
                        .map_err(query(CONTEXT_LIST))?
                        .into(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        transaction.commit().await.map_err(query(CONTEXT_LIST))?;
        Ok(entries)
    }

    /// Upsert `deltas` under `fleet_id`, then sweep and cap.
    ///
    /// One transaction, so the role is scoped and the writes land together. The
    /// sweep runs BEFORE the cap deliberately: an already-expired `daily` row
    /// must not occupy a cap slot during victim selection, or eviction deletes
    /// a durable row in the doomed row's place.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and an instant that cannot be
    /// encoded as an identifier. A delta refused for its shape is counted, not
    /// an error — see [`Captured::skipped`].
    pub async fn capture(
        &self,
        fleet_id: &Uuid7,
        deltas: &[MemoryDelta<'_>],
        now: UnixMillis,
    ) -> Result<Captured> {
        let admitted = admit(deltas);
        let mut counted = Captured {
            stored: 0,
            skipped: admitted.skipped,
            truncated: admitted.truncated,
            swept: 0,
            evicted: 0,
        };

        let mut connection = self.database.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_UPSERT))?;
        sqlx::query(sql::ASSUME_MEMORY_ROLE)
            .execute(&mut *transaction)
            .await
            .map_err(query(CONTEXT_UPSERT))?;

        for delta in admitted.entries {
            let mut bytes = [0_u8; ENTROPY_LEN];
            self.entropy.fill(&mut bytes)?;
            let row_id = Uuid7::encode(now, bytes)?;
            sqlx::query(sql::UPSERT_ENTRY)
                .bind(row_id.as_str())
                .bind(delta.key.as_ref())
                .bind(delta.content.as_ref())
                .bind(delta.category.as_ref())
                .bind(fleet_id.as_str())
                .bind(now.as_millis())
                .execute(&mut *transaction)
                .await
                .map_err(query(CONTEXT_UPSERT))?;
            counted.stored += 1;
        }

        counted.swept = sqlx::query(sql::DELETE_AGED_IN_CATEGORY)
            .bind(fleet_id.as_str())
            .bind(window::DAILY_CATEGORY)
            .bind(now.as_millis().saturating_sub(DAILY_RETENTION_MS))
            .execute(&mut *transaction)
            .await
            .map_err(query(CONTEXT_SWEEP))?
            .rows_affected();

        counted.evicted = sqlx::query(sql::EVICT_PAST_CAP)
            .bind(fleet_id.as_str())
            .bind(i64::try_from(MAX_ENTRIES_PER_FLEET).unwrap_or(i64::MAX))
            .bind(window::PINNED_CATEGORY)
            .execute(&mut *transaction)
            .await
            .map_err(query(CONTEXT_EVICT))?
            .rows_affected();

        transaction.commit().await.map_err(query(CONTEXT_UPSERT))?;
        Ok(counted)
    }
}

/// The deltas a push may store, and the tallies for those it may not.
struct Admitted<'a, 'b> {
    entries: Vec<&'a MemoryDelta<'b>>,
    skipped: usize,
    truncated: usize,
}

/// Filter a push to the deltas worth storing.
///
/// Two different refusals, counted apart because they mean different things. A
/// SKIPPED delta is malformed — an empty or oversized field — and the rest of
/// the batch is unaffected. A TRUNCATED delta is well-formed and simply past
/// the push byte cap, which ENDS the batch: the cap bounds one request, so
/// everything after the entry that crosses it is refused too.
///
/// Truncating rather than refusing the whole push is deliberate. A runner that
/// learned more than the cap allows should keep what fits, not lose all of it.
fn admit<'a, 'b>(deltas: &'a [MemoryDelta<'b>]) -> Admitted<'a, 'b> {
    let well_formed = |delta: &&MemoryDelta<'_>| {
        (1..=MAX_KEY_LEN).contains(&delta.key.len())
            && (1..=MAX_CONTENT_LEN).contains(&delta.content.len())
            && (1..=MAX_CATEGORY_LEN).contains(&delta.category.len())
    };
    let skipped = deltas.iter().filter(|d| !well_formed(d)).count();

    // The cap is a running total over the well-formed deltas, so a malformed
    // one neither consumes budget nor ends the batch.
    let mut used = 0_usize;
    let entries: Vec<_> = deltas
        .iter()
        .filter(well_formed)
        .take_while(|delta| {
            used += window::entry_bytes(delta);
            used <= afd_wire::memory::MAX_PUSH_BYTES
        })
        .collect();
    Admitted {
        truncated: deltas.len() - skipped - entries.len(),
        entries,
        skipped,
    }
}

/// Longest stored key. `helpers.zig`'s `MAX_KEY_LEN`.
///
/// Public because the operator surface bounds a path segment by it before it
/// decodes one: a key too long to have been STORED cannot name a row, so the
/// HTTP edge refuses it rather than spending a statement discovering that. One
/// declaration, so the write cap and the read cap cannot drift apart.
pub const MAX_KEY_LEN: usize = 255;

/// Longest stored content. `helpers.zig`'s `MAX_CONTENT_LEN`.
const MAX_CONTENT_LEN: usize = 16 * 1024;

/// Longest stored category.
///
/// The column carries no CHECK — value constraints live in app constants — so
/// this is the only bound stopping an oversized label landing a junk category.
const MAX_CATEGORY_LEN: usize = 64;
