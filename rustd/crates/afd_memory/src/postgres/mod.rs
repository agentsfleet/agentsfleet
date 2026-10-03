//! The Postgres store: the memory module `afd_fleet` carried, moved behind
//! [`MemoryStore`] with its window, upsert, sweep and eviction unchanged.
//!
//! # The role switch is a transaction, not a pair of statements
//!
//! `memory.memory_entries` is written as `memory_runtime`, a role the api pool
//! does not otherwise hold. `SET LOCAL ROLE` inside a transaction is restored
//! by Postgres at COMMIT or ROLLBACK — including the rollback a dropped
//! transaction performs — so there is no reset to fail and no connection can
//! return to the pool with the wrong role. [`PgStore::as_memory_role`] is the
//! one place that opens such a transaction.

mod page;
mod read;
mod sql;
mod write;

use afd_core::clock::UnixMillis;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_wire::memory::{MemoryDelta, Visibility};
use sqlx::postgres::PgRow;
use sqlx::{Acquire as _, PgConnection, Row as _};

use crate::error::detail::ROLE_SWITCH;
use crate::error::{Error, Result, query, unavailable};
use crate::page::{After, View};
use crate::record::{Housekept, Owner, Record};
use crate::store::MemoryStore;

/// The name the flip's log lines give this store.
const NAME: &str = "postgres";

/// The durable memory store, over the api-role pool.
#[derive(Debug, Clone)]
pub struct PgStore {
    database: Db,
    entropy: Entropy,
}

/// How a refused statement is reported, chosen by who asked.
///
/// The runner plane's verbs answer the internal database codes they always
/// have; the operator surface answers `UZ-MEM-003` with a sentence naming the
/// operation. Both are kept, so moving behind the trait changes no reply.
#[derive(Debug, Clone, Copy)]
enum Failure {
    /// A runner-plane statement, named for the log.
    Runner(&'static str),
    /// An operator statement, with the sentence its 503 carries.
    Operator(&'static str),
}

impl Failure {
    /// `source` as this caller's failure.
    fn raise(self, source: sqlx::Error) -> Error {
        match self {
            Self::Runner(context) => query(context)(source),
            Self::Operator(detail) => unavailable(detail)(source),
        }
    }

    /// The role switch's failure, which the operator surface names apart.
    fn role(self, source: sqlx::Error) -> Error {
        match self {
            Self::Runner(context) => query(context)(source),
            Self::Operator(_detail) => unavailable(ROLE_SWITCH)(source),
        }
    }
}

impl PgStore {
    /// A store reading and writing through `database`.
    #[must_use]
    pub const fn new(database: Db, entropy: Entropy) -> Self {
        Self { database, entropy }
    }

    /// Runs `work` inside one transaction under the memory role, committing
    /// only when it succeeds.
    async fn as_memory_role<T>(
        &self,
        failure: Failure,
        work: impl AsyncFnOnce(&mut PgConnection) -> Result<T>,
    ) -> Result<T> {
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection
            .begin()
            .await
            .map_err(|source| failure.raise(source))?;
        sqlx::query(sql::ASSUME_MEMORY_ROLE)
            .execute(&mut *transaction)
            .await
            .map_err(|source| failure.role(source))?;
        let value = work(&mut transaction).await?;
        transaction
            .commit()
            .await
            .map_err(|source| failure.raise(source))?;
        Ok(value)
    }
}

/// One row of the seven-column read every statement shares.
fn record(row: &PgRow, failure: Failure) -> Result<Record> {
    let column = |source| failure.raise(source);
    let writer: String = row.try_get(0).map_err(column)?;
    Ok(Record {
        fleet: Uuid7::parse(&writer)?,
        key: row.try_get(1).map_err(column)?,
        content: row.try_get(2).map_err(column)?,
        category: row.try_get(3).map_err(column)?,
        visibility: if row.try_get(4).map_err(column)? {
            Visibility::Workspace
        } else {
            Visibility::Fleet
        },
        created_at_ms: row.try_get(5).map_err(column)?,
        updated_at_ms: row.try_get(6).map_err(column)?,
    })
}

#[async_trait::async_trait]
impl MemoryStore for PgStore {
    fn name(&self) -> &'static str {
        NAME
    }

    async fn window(&self, owner: Owner<'_>, reads: bool) -> Result<Vec<Record>> {
        read::window(self, owner, reads).await
    }

    async fn upsert(
        &self,
        owner: Owner<'_>,
        entries: &[&MemoryDelta<'_>],
        now: UnixMillis,
    ) -> Result<Housekept> {
        write::upsert(self, owner, entries, now).await
    }

    async fn search(
        &self,
        owner: Owner<'_>,
        reads: bool,
        query: &str,
        limit: usize,
    ) -> Result<Vec<Record>> {
        read::search(self, owner, reads, query, limit).await
    }

    async fn page(
        &self,
        owner: Owner<'_>,
        reads: bool,
        view: View<'_>,
        after: Option<After<'_>>,
        limit: i64,
    ) -> Result<Vec<Record>> {
        read::page(self, owner, reads, view, after, limit).await
    }

    async fn forget(&self, owner: Owner<'_>, key: &str) -> Result<bool> {
        write::forget(self, owner, key).await
    }

    async fn export(&self, workspace: &Uuid7) -> Result<Vec<Record>> {
        read::export(self, workspace).await
    }

    async fn import(&self, workspace: &Uuid7, record: &Record) -> Result<()> {
        write::import(self, workspace, record).await
    }
}
