//! The built-in biller: wallets, holds and the usage log, all in Postgres.
//!
//! One wallet row lock per tenant serializes every reservation and charge for
//! that tenant, so concurrent holds can never promise more than the balance
//! the lock reads. Lock order is always wallet, then holds, then lines.

use std::sync::Arc;

use afd_core::clock::Clock;
use afd_core::id::Uuid7;
use afd_crypto::entropy::Entropy;
use afd_db::Db;
use sqlx::Connection as _;

use crate::biller::{
    Account, Balance, Biller, Hold, Receipt, Release, Reservation, Settlement, WorkKey,
};
use crate::error::{billing_wallet_missing, query};
use crate::{Nanos, Result};

mod lines;
mod reserve;
mod settle;
mod sweep;

pub use self::lines::log_lines;
pub use self::sweep::{GRACE, Resettled, charged_so_far, resettle, unsettled};

const LOCK_WALLET: &str = include_str!("sql/lock_wallet.sql");
const SELECT_HOLD: &str = include_str!("sql/select_hold.sql");
const SELECT_HELD_BY_OTHERS: &str = include_str!("sql/select_held_by_others.sql");
const UPSERT_HOLD: &str = include_str!("sql/upsert_hold.sql");
const INSERT_LINES: &str = include_str!("sql/insert_lines.sql");
const SETTLE_LINES: &str = include_str!("sql/settle_lines.sql");
const SELECT_WORK_TENANT: &str = include_str!("sql/select_work_tenant.sql");
const DEBIT_WALLET: &str = include_str!("sql/debit_wallet.sql");
const CAPTURE_HOLD: &str = include_str!("sql/capture_hold.sql");
const RELEASE_HOLD: &str = include_str!("sql/release_hold.sql");
const SELECT_BALANCE: &str = include_str!("sql/select_balance.sql");
const BOUND_TRANSACTION: &str = include_str!("sql/bound_transaction.sql");

/// How long a billing statement waits for the wallet row lock.
///
/// Every operation for one tenant serializes on that lock, so a holder that
/// stalls would stall the tenant's whole fleet. Past this the statement gives
/// up with SQLSTATE `55P03`, which [`crate::Error::is_contended`] names and the
/// outage policy treats as a biller that could not answer.
const LOCK_TIMEOUT: &str = "2000ms";

/// How long any one billing statement may run.
const STATEMENT_TIMEOUT: &str = "5000ms";

/// What each statement here was doing, for [`query`].
const CONTEXT_RESERVE: &str = "hold a reservation";
const CONTEXT_SETTLE: &str = "settle usage lines";
const CONTEXT_RELEASE: &str = "release a hold";
const CONTEXT_BALANCE: &str = "read a balance";

/// The built-in [`Biller`] over `billing.wallets`, `billing.holds` and
/// `billing.usage_lines`.
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

    /// A fresh row identifier.
    fn mint(&self) -> Result<Uuid7> {
        Ok(self.entropy.uuid7(self.clock.now())?)
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

/// `tenant`'s wallet balance, locked for the rest of the transaction.
///
/// # Errors
/// The statement would not run, or the tenant has no wallet.
async fn lock_wallet(
    connection: &mut sqlx::PgConnection,
    tenant: &Uuid7,
    context: &'static str,
) -> Result<Nanos> {
    let balance: Option<i64> = sqlx::query_scalar(LOCK_WALLET)
        .bind(tenant.as_str())
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(context))?;
    balance
        .map(Nanos::from_i64)
        .ok_or_else(billing_wallet_missing)
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

/// The wallet's balance and open holds, by column name.
#[derive(Debug, sqlx::FromRow)]
struct BalanceRow {
    balance_nanos: i64,
    exhausted_at: Option<jiff_sqlx::Timestamp>,
    held: i64,
}

#[async_trait::async_trait]
impl Biller for LedgerBiller {
    async fn reserve(&self, ask: &Reservation) -> Result<Hold> {
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_RESERVE))?;
        bound(&mut transaction, CONTEXT_RESERVE).await?;
        let hold = reserve::reserve(self, &mut transaction, ask).await?;
        transaction.commit().await.map_err(query(CONTEXT_RESERVE))?;
        Ok(hold)
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

    async fn release(&self, work: &WorkKey, _why: Release) -> Result<()> {
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_RELEASE))?;
        bound(&mut transaction, CONTEXT_RELEASE).await?;
        settle::release(&mut transaction, work).await?;
        transaction.commit().await.map_err(query(CONTEXT_RELEASE))?;
        Ok(())
    }

    async fn balance(&self, payer: &Account) -> Result<Balance> {
        let mut connection = self.database.acquire().await?;
        balance(connection.as_mut(), &payer.tenant).await
    }
}

/// `tenant`'s balance and what its open holds reserve, read on `executor`.
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
        held: Nanos::from_i64(row.held),
        exhausted_at: row.exhausted_at.map(jiff_sqlx::Timestamp::to_jiff),
    })
}
