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

pub use self::lines::log_lines;

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

    /// A fresh row identifier.
    fn mint(&self) -> Result<Uuid7> {
        Ok(self.entropy.uuid7(self.clock.now())?)
    }
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
        let hold = reserve::reserve(self, &mut transaction, ask).await?;
        transaction.commit().await.map_err(query(CONTEXT_RESERVE))?;
        Ok(hold)
    }

    async fn settle(&self, settlement: &Settlement) -> Result<Receipt> {
        let mut connection = self.database.acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT_SETTLE))?;
        lock_wallet(&mut transaction, &settlement.payer.tenant, CONTEXT_SETTLE).await?;
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
        settle::release(&mut transaction, work).await?;
        transaction.commit().await.map_err(query(CONTEXT_RELEASE))?;
        Ok(())
    }

    async fn balance(&self, payer: &Account) -> Result<Balance> {
        let mut connection = self.database.acquire().await?;
        let row: Option<BalanceRow> = sqlx::query_as(SELECT_BALANCE)
            .bind(payer.tenant.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(query(CONTEXT_BALANCE))?;
        let row = row.ok_or_else(billing_wallet_missing)?;
        Ok(Balance {
            balance: Nanos::from_i64(row.balance_nanos),
            held: Nanos::from_i64(row.held),
            exhausted_at: row.exhausted_at.map(jiff_sqlx::Timestamp::to_jiff),
        })
    }
}
