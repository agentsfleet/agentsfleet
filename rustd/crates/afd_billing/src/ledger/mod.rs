//! The built-in biller: wallets and the usage log, both in Postgres.
//!
//! A funds check is one read: the balance less every line the payer logged
//! and was not yet charged for, capped at a second so a slow read refuses
//! rather than holding the lease path. Charging stamps the lines first and debits the
//! wallet last, so the wallet row is locked only from the debit to the commit.

use std::sync::Arc;

use afd_core::clock::Clock;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use sqlx::postgres::{PgArguments, PgRow};
use sqlx::query::QueryAs;
use sqlx::{Connection as _, FromRow, Postgres, SqlSafeStr};

use crate::biller::{Account, Ask, Balance, Biller, Funds, Receipt, Settlement};
use crate::error::{billing_wallet_missing, query};
use crate::{Nanos, Result};

mod lines;
mod settle;
mod sweep;

pub use self::lines::log_lines;
#[cfg(feature = "test-util")]
pub use self::sweep::unsettled_work;
pub use self::sweep::{Resettled, Resume, resettle, unsettled};

const INSERT_LINES: &str = include_str!("sql/insert_lines.sql");
const SETTLE_LINES: &str = include_str!("sql/settle_lines.sql");
const DEBIT_WALLET: &str = include_str!("sql/debit_wallet.sql");
const SELECT_BALANCE: &str = include_str!("sql/select_balance.sql");
const BOUND_TRANSACTION: &str = include_str!("sql/bound_transaction.sql");

/// Opens a funds read's transaction with its one-second statement cap, in one
/// round trip.
///
/// A funds check sits on the lease path: renewal, admission and every setup
/// chat model call. A read past the cap ends with SQLSTATE `57014`, which
/// [`crate::Error::is_unanswered`] names; each caller then runs the work
/// rather than wait on the read. The read sums one tenant's unsettled lines
/// over a partial index, so it takes milliseconds; the cap is for a relay
/// that stopped and left those lines to pile up, or a database under load.
const BEGIN_READ: &str = include_str!("sql/begin_read.sql");

/// How long a billing statement waits for a row lock.
///
/// The debit waits on the wallet row while another transaction holds it. Past
/// this the statement gives up with SQLSTATE `55P03`, which [`crate::Error::is_contended`] names and the
/// outage policy treats as a biller that could not answer.
const LOCK_TIMEOUT: &str = "2000ms";

/// How long any one billing statement may run.
const STATEMENT_TIMEOUT: &str = "5000ms";

/// What each statement here was doing, for [`query`].
const CONTEXT_SETTLE: &str = "settle usage lines";
const CONTEXT_BALANCE: &str = "read a balance";

/// The built-in [`Biller`] over `billing.wallets` and `billing.usage_lines`.
#[derive(Debug, Clone)]
pub struct LedgerBiller {
    database: Db,
    entropy: Entropy,
    clock: Arc<dyn Clock>,
}

impl LedgerBiller {
    /// A biller reading and writing through `database`, minting row ids from
    /// `entropy` at `clock`'s instant.
    #[must_use]
    pub fn new(database: Db, entropy: Entropy, clock: Arc<dyn Clock>) -> Self {
        Self {
            database,
            entropy,
            clock,
        }
    }

    /// A biller over `database` on the wall clock and fresh entropy: what the
    /// daemon builds, and what a test over a live database wants.
    #[must_use]
    pub fn on_system_clock(database: Db) -> Self {
        Self::new(
            database,
            Entropy::new(),
            Arc::new(afd_core::clock::SystemClock),
        )
    }
}

/// Bounds the lock wait and statement time for the rest of the transaction.
///
/// Set per transaction with `set_config(.., true)` rather than as connection
/// startup options: a transaction-pooling bouncer refuses those, and a local
/// setting ends with the transaction it bounds.
async fn bound(connection: &mut sqlx::PgConnection, context: &'static str) -> Result<()> {
    sqlx::query(BOUND_TRANSACTION)
        .bind(LOCK_TIMEOUT)
        .bind(STATEMENT_TIMEOUT)
        .execute(&mut *connection)
        .await
        .map_err(query(context))?;
    Ok(())
}

/// The funds read, `wrap`ped: as written for production, under `EXPLAIN` for
/// a plan test.
fn balance_statement<'q, O, S>(
    wrap: impl FnOnce(&'static str) -> S,
    tenant: &'q Uuid7,
) -> QueryAs<'q, Postgres, O, PgArguments>
where
    O: for<'r> FromRow<'r, PgRow>,
    S: SqlSafeStr,
{
    sqlx::query_as(wrap(SELECT_BALANCE)).bind(tenant.as_str())
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

/// The wallet's balance and its unsettled lines, by column name.
#[derive(Debug, sqlx::FromRow)]
struct BalanceRow {
    balance_nanos: i64,
    exhausted_at: Option<jiff_sqlx::Timestamp>,
    unsettled: i64,
}

#[async_trait::async_trait]
impl Biller for LedgerBiller {
    async fn check(&self, ask: &Ask) -> Result<Funds> {
        let funds = self.capped_balance(&ask.payer.tenant).await?;
        Ok(Funds::of(funds.left(), ask.next))
    }

    async fn settle(&self, settlement: &Settlement) -> Result<Receipt> {
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_SETTLE))?;
        bound(&mut transaction, CONTEXT_SETTLE).await?;
        log_lines(
            &mut *transaction,
            &self.entropy,
            self.clock.now(),
            settlement,
        )
        .await?;
        let receipt = settle::charge(&mut transaction, settlement).await?;
        transaction.commit().await.map_err(query(CONTEXT_SETTLE))?;
        Ok(receipt)
    }

    async fn balance(&self, payer: &Account) -> Result<Balance> {
        self.capped_balance(&payer.tenant).await
    }
}

impl LedgerBiller {
    /// `tenant`'s balance, read on a connection of its own under
    /// [`BEGIN_READ`]'s cap.
    async fn capped_balance(&self, tenant: &Uuid7) -> Result<Balance> {
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection
            .begin_with(BEGIN_READ)
            .await
            .map_err(query(CONTEXT_BALANCE))?;
        let funds = balance(&mut *transaction, tenant).await?;
        transaction.commit().await.map_err(query(CONTEXT_BALANCE))?;
        Ok(funds)
    }
}

/// `tenant`'s balance and what its logged lines still owe, read on `executor`.
///
/// What [`LedgerBiller`] answers for [`Biller::balance`], exposed so a caller
/// already inside a transaction reads it on that transaction's connection
/// rather than taking a second one from the pool: a nested acquire under load
/// starves the pool, every connection held by a transaction waiting for one
/// more. Read that way it carries no cap of its own, because a cap set here
/// would bound the rest of the caller's transaction too.
///
/// # Errors
/// The statement would not run, or the tenant has no wallet.
pub async fn balance<'e>(executor: impl sqlx::PgExecutor<'e>, tenant: &Uuid7) -> Result<Balance> {
    let row: Option<BalanceRow> = balance_statement(|statement| statement, tenant)
        .fetch_optional(executor)
        .await
        .map_err(query(CONTEXT_BALANCE))?;
    let row = row.ok_or_else(billing_wallet_missing)?;
    Ok(Balance {
        balance: Nanos::from_i64(row.balance_nanos),
        unsettled: Nanos::from_i64(row.unsettled),
        exhausted_at: row.exhausted_at.map(jiff_sqlx::Timestamp::to_jiff),
    })
}
