//! The Postgres store: the window, the upsert with its sweep and cap, recall,
//! the operator page, and forget. Reads live in `read`, writes in `write`.
//!
//! A store holds rows and answers for them; it decides nothing about who may
//! read or publish. [`crate::Memories`] reads the grants from `core.fleets` and
//! passes them in as `reads`, so a query enforces a grant by honouring one flag.
//!
//! # The role switch is a transaction, not a pair of statements
//!
//! `memory.entries` is written as `memory_runtime`, a role the api pool
//! does not otherwise hold. `SET LOCAL ROLE` inside a transaction is restored
//! by Postgres at COMMIT or ROLLBACK — including the rollback a dropped
//! transaction performs — so there is no reset to fail and no connection can
//! return to the pool with the wrong role. [`PgStore::as_memory_role`] is the
//! one place that opens such a transaction.

mod page;
mod read;
mod write;

use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use afd_wire::memory::Visibility;
use sqlx::postgres::PgRow;
use sqlx::{Acquire as _, FromRow as _, PgConnection};

use crate::error::detail::ROLE_SWITCH;
use crate::error::{Error, Result, query, unavailable};
use crate::record::Record;
use crate::sql;

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
/// operation.
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

/// The seven columns every read returns, by name.
#[derive(Debug, sqlx::FromRow)]
struct EntryRow {
    fleet_id: String,
    key: String,
    content: String,
    category: String,
    workspace_visible: bool,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    created_at: jiff::Timestamp,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    updated_at: jiff::Timestamp,
}

/// One row of the seven-column read every statement shares.
fn record(row: &PgRow, failure: Failure) -> Result<Record> {
    let row = EntryRow::from_row(row).map_err(|source| failure.raise(source))?;
    Ok(Record {
        fleet: Uuid7::parse(&row.fleet_id)?,
        key: row.key,
        content: row.content,
        category: row.category,
        visibility: if row.workspace_visible {
            Visibility::Workspace
        } else {
            Visibility::Fleet
        },
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}
