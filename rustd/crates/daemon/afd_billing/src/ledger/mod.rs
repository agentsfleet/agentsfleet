//! The built-in biller: wallets and the usage log, both in Postgres.
//!
//! A line is charged as it is written: the statement that inserts it debits
//! the payer's wallet and adds it to the daily rollups (the `billing.usage_lines`
//! triggers), so a funds check is one read of the wallet. It runs under the API
//! login's lock and statement bounds (`afd_db::login`), so a read stuck behind a
//! lock gives up unanswered rather than holding the run path.

use af_core::id::Uuid7;
use afd_db::Db;
use sqlx::postgres::{PgArguments, PgRow};
use sqlx::query::QueryAs;
use sqlx::{FromRow, Postgres, SqlSafeStr};

use crate::biller::{Account, Ask, Balance, Biller, Funds};
use crate::error::{billing_wallet_missing, query};
use crate::{Result, Wallet};

mod lines;

pub use self::lines::log_lines;

const INSERT_LINES: &str = include_str!("sql/insert_lines.sql");

/// What the balance read was doing, for [`query`].
const CONTEXT_BALANCE: &str = "read a balance";

/// The built-in [`Biller`] over `billing.wallets`.
#[derive(Debug, Clone)]
pub struct LedgerBiller {
    database: Db,
}

impl LedgerBiller {
    /// A biller reading through `database`.
    #[must_use]
    pub const fn new(database: Db) -> Self {
        Self { database }
    }
}

/// The funds read, `wrap`ped: as written for production, under `EXPLAIN` for
/// a plan test. It is the wallet row alone, the one the billing page reads.
fn balance_statement<'q, O, S>(
    wrap: impl FnOnce(&'static str) -> S,
    tenant: &'q Uuid7,
) -> QueryAs<'q, Postgres, O, PgArguments>
where
    O: for<'r> FromRow<'r, PgRow>,
    S: SqlSafeStr,
{
    sqlx::query_as(wrap(crate::sql::SELECT_WALLET)).bind(tenant)
}

/// Wraps the production funds read, keeping its bindings, for a plan test.
#[cfg(feature = "test-util")]
pub fn balance_query<'q, O, S>(
    wrap: impl FnOnce(&'static str) -> S,
    tenant: &'q Uuid7,
) -> QueryAs<'q, Postgres, O, PgArguments>
where
    O: for<'r> FromRow<'r, PgRow>,
    S: SqlSafeStr,
{
    balance_statement(wrap, tenant)
}

#[async_trait::async_trait]
impl Biller for LedgerBiller {
    async fn check(&self, ask: &Ask) -> Result<Funds> {
        let funds = self.pooled_balance(&ask.payer.tenant).await?;
        Ok(Funds::of(funds.balance, ask.next))
    }

    async fn balance(&self, payer: &Account) -> Result<Balance> {
        self.pooled_balance(&payer.tenant).await
    }
}

impl LedgerBiller {
    /// `tenant`'s balance, read on a connection of its own taken from the pool,
    /// under the API login's bounds (`afd_db::login`).
    async fn pooled_balance(&self, tenant: &Uuid7) -> Result<Balance> {
        let mut connection = self.database.acquire().await?;
        balance(&mut *connection, tenant).await
    }
}

/// `tenant`'s balance, every logged line already debited, read on `executor`.
///
/// What [`LedgerBiller`] answers for [`Biller::balance`], exposed so a caller
/// already inside a transaction reads it on that transaction's connection
/// rather than taking a second one from the pool: a nested acquire under load
/// starves the pool, every connection held by a transaction waiting for one
/// more. Read that way it sets no bound of its own, because one set here would
/// bound the rest of the caller's transaction too; the login's bounds apply.
///
/// # Errors
/// The statement would not run, or the tenant has no wallet.
pub async fn balance<'e>(executor: impl sqlx::PgExecutor<'e>, tenant: &Uuid7) -> Result<Balance> {
    let wallet: Option<Wallet> = balance_statement(|statement| statement, tenant)
        .fetch_optional(executor)
        .await
        .map_err(query(CONTEXT_BALANCE))?;
    let wallet = wallet.ok_or_else(billing_wallet_missing)?;
    Ok(Balance {
        balance: wallet.balance,
        exhausted_at: wallet.exhausted_at,
    })
}
