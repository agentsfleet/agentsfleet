//! The `billing.wallets` row a suite seeds, spelled once.
//!
//! Every suite that drives work past the credits gate needs a wallet under
//! the tenant it seeded, and a wallet with no row reads as an empty one. They
//! build a [`WalletSeed`] rather than each spelling the insert, the way
//! [`super::TenantSeed`] seeds the tenant it hangs from, and the column list
//! changes here alone.

use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;
use sqlx::PgExecutor;

use crate::{Result, error};

/// Every column a wallet seed sets.
const INSERT_WALLET: &str = include_str!("sql/insert_wallet.sql");

const CONTEXT: &str = "seed a wallet";

/// One tenant's wallet, holding `balance_nanos`. Stamped by the database's
/// `now()` unless the suite stamps it.
#[derive(Debug, Clone, Copy)]
#[must_use = "a seed writes nothing until `insert` runs"]
pub struct WalletSeed<'a> {
    tenant_id: &'a str,
    balance_nanos: i64,
    created_at: Option<Timestamp>,
}

impl<'a> WalletSeed<'a> {
    /// A wallet under `tenant_id` holding `balance_nanos`.
    pub const fn new(tenant_id: &'a str, balance_nanos: i64) -> Self {
        Self {
            tenant_id,
            balance_nanos,
            created_at: None,
        }
    }

    /// Stamps both `created_at` and `updated_at` with `instant`.
    pub const fn at(mut self, instant: Timestamp) -> Self {
        self.created_at = Some(instant);
        self
    }

    /// Writes the row.
    ///
    /// # Errors
    /// [`crate::Error`] when the insert fails: a missing tenant, a tenant that
    /// already holds a wallet, or a negative balance the table refuses.
    pub async fn insert<'e>(self, executor: impl PgExecutor<'e>) -> Result<()> {
        sqlx::query(INSERT_WALLET)
            .bind(self.tenant_id)
            .bind(self.balance_nanos)
            .bind(self.created_at.map(Timestamp::to_sqlx))
            .execute(executor)
            .await
            .map_err(|source| error::query(CONTEXT, source))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{INSERT_WALLET, WalletSeed};

    #[test]
    fn a_seed_stamps_nothing_until_the_suite_backdates_it() {
        let seed = WalletSeed::new("tenant", 1);

        assert_eq!((seed.balance_nanos, seed.created_at), (1, None));
    }

    #[test]
    fn the_seed_writes_the_wallet_table_by_its_schema() {
        assert!(
            INSERT_WALLET.starts_with("INSERT INTO billing.wallets "),
            "{INSERT_WALLET}"
        );
    }
}
