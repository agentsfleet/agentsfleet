//! The built-in biller: wallets and the usage log, both in Postgres.
//!
//! A funds check is one read: the balance less every line the payer logged
//! and was not yet charged for. Charging takes the payer's wallet row lock, so
//! concurrent relay passes debit one at a time; lock order is wallet, then
//! lines.

use std::sync::Arc;
use std::time::Duration;

use afd_core::clock::Clock;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use sqlx::Connection as _;

use crate::biller::{Account, Ask, Balance, Biller, Funds, Receipt, Settlement};
use crate::error::{billing_wallet_missing, query};
use crate::{Nanos, Result};

mod lines;
mod settle;
mod sweep;

pub use self::lines::log_lines;
#[cfg(feature = "test-util")]
pub use self::sweep::unsettled_work;
pub use self::sweep::{Resettled, charged_so_far, resettle, unsettled};

/// The ledger's bounds, for integration tests that measure against them.
#[cfg(feature = "test-util")]
pub mod test_util {
    /// How long a billing statement waits for the wallet row lock.
    pub const LOCK_TIMEOUT: std::time::Duration = super::LOCK_TIMEOUT;
}

const LOCK_WALLET: &str = include_str!("sql/lock_wallet.sql");
const INSERT_LINES: &str = include_str!("sql/insert_lines.sql");
const SETTLE_LINES: &str = include_str!("sql/settle_lines.sql");
const DEBIT_WALLET: &str = include_str!("sql/debit_wallet.sql");
const SELECT_BALANCE: &str = include_str!("sql/select_balance.sql");
const BOUND_TRANSACTION: &str = include_str!("sql/bound_transaction.sql");

/// How long a billing statement waits for the wallet row lock.
///
/// Every operation for one tenant serializes on that lock, so a holder that
/// stalls would stall the tenant's whole fleet. Past this the statement gives
/// up with SQLSTATE `55P03`, which [`crate::Error::is_contended`] names and the
/// outage policy treats as a biller that could not answer.
const LOCK_TIMEOUT: Duration = Duration::from_secs(2);

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
        .bind(format!("{}ms", LOCK_TIMEOUT.as_millis()))
        .bind(STATEMENT_TIMEOUT)
        .execute(&mut *connection)
        .await
        .map_err(query(context))?;
    Ok(())
}

/// `tenant`'s wallet, locked for the rest of the transaction, or `None` when
/// the tenant has no wallet row.
///
/// Settling and releasing take this rather than [`lock_wallet`]: usage that
/// already happened must settle even for a tenant whose wallet was never
/// provisioned, or its lines would stay unsettled forever and hold every
/// newer line behind them in the sweeper's oldest-first scan. With no wallet
/// nothing is debited, and the settlement logs that it was not.
async fn lock_wallet_if_any(
    connection: &mut sqlx::PgConnection,
    tenant: &Uuid7,
    context: &'static str,
) -> Result<Option<Nanos>> {
    let balance: Option<i64> = sqlx::query_scalar(LOCK_WALLET)
        .bind(tenant.as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(context))?;
    Ok(balance.map(Nanos::from_i64))
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
        let mut connection = self.database.acquire().await?;
        let funds = balance(connection.as_mut(), &ask.payer.tenant).await?;
        let available = Nanos::from_i64(
            funds
                .balance
                .as_i64()
                .saturating_sub(funds.unsettled.as_i64())
                .max(0),
        );
        Ok(if available.covers(ask.next) {
            Funds::Covered
        } else {
            Funds::Short {
                available,
                asked: ask.next,
            }
        })
    }

    async fn settle(&self, settlement: &Settlement) -> Result<Receipt> {
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_SETTLE))?;
        bound(&mut transaction, CONTEXT_SETTLE).await?;
        lock_wallet_if_any(&mut transaction, &settlement.payer.tenant, CONTEXT_SETTLE).await?;
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
        let mut connection = self.database.acquire().await?;
        balance(connection.as_mut(), &payer.tenant).await
    }
}

/// `tenant`'s balance and what its logged lines still owe, read on `executor`.
///
/// What [`LedgerBiller`] answers for [`Biller::balance`], exposed so a caller
/// already inside a transaction reads it on that transaction's connection
/// rather than taking a second one from the pool: a nested acquire under load
/// starves the pool, every connection held by a transaction waiting for one
/// more.
///
/// # Errors
/// The statement would not run, or the tenant has no wallet.
pub async fn balance<'e>(executor: impl sqlx::PgExecutor<'e>, tenant: &Uuid7) -> Result<Balance> {
    let row: Option<BalanceRow> = sqlx::query_as(SELECT_BALANCE)
        .bind(tenant.as_str())
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
