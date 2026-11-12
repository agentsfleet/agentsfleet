//! A tenant's credit pool, as the `billing.wallets` row holds it.
//!
//! A tenant with no wallet row is a bootstrap-invariant violation, not an empty
//! pool: [`crate::tenant::Billing::snapshot`] refuses it, and the ledger biller
//! refuses it the same way, so no path reads a missing wallet as a zero balance.

use jiff::Timestamp;
use sqlx::FromRow;
use sqlx::postgres::PgRow;

use crate::Nanos;

/// A tenant's credit pool, as the `billing.wallets` row holds it.
///
/// The one decode of that row, for the billing page
/// ([`crate::tenant::Billing::snapshot`]).
///
/// Decoded through [`WalletRow`], so every column read folds into the one
/// failure sqlx already reports for the query: the caller has a single error
/// path, and it is one a test can reach by cutting the connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wallet {
    /// What is left to spend; negative while the tenant owes a debt.
    pub balance: Nanos,
    /// When the pool last ran dry, if it ever has.
    pub exhausted_at: Option<Timestamp>,
    /// When the pool last moved.
    pub updated_at: Timestamp,
}

/// The `billing.wallets` row as its columns spell it.
///
/// `exhausted_at` is nullable, and `#[sqlx(try_from)]` has no form for an
/// optional bridged type, so it decodes as the bridge type itself and becomes a
/// jiff instant in [`Wallet::from_row`].
#[derive(FromRow)]
struct WalletRow {
    balance_nanos: i64,
    exhausted_at: Option<jiff_sqlx::Timestamp>,
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    updated_at: Timestamp,
}

impl FromRow<'_, PgRow> for Wallet {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        let stored = WalletRow::from_row(row)?;
        Ok(Self {
            balance: Nanos::from_i64(stored.balance_nanos),
            exhausted_at: stored.exhausted_at.map(jiff_sqlx::Timestamp::to_jiff),
            updated_at: stored.updated_at,
        })
    }
}
