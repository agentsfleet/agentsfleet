//! The tenant's money, read-only: the wallet snapshot and the charges ledger.
//!
//! # Why this module cannot move money
//!
//! Every drain of `balance_nanos` is a writable CTE on the lease plane — the
//! renew and settle statements — and the one inflow is the signup starter
//! grant. This module holds neither; it is the read side those movements are
//! audited through, and a compromise of the tenant plane's HTTP surface
//! therefore cannot mint credit. That is a property of what is ABSENT here, so
//! it is stated where somebody adding a write will read it.
//!
//! # The missing wallet row is an invariant violation, not a 404
//!
//! Signup bootstrap inserts the wallet in the tenant-create transaction, so a
//! tenant without one cannot be produced by any path this daemon serves — only
//! by operator surgery or a defect. [`Billing::snapshot`] therefore refuses
//! with an INTERNAL code and the bootstrap-invariant sentence rather than
//! answering 404: telling the caller "not found" would send them looking for a
//! resource they cannot create.

pub mod cursor;

use afd_core::id::Uuid7;
use afd_db::Db;
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;

use crate::{Result, Wallet, error, sql};

/// The context a datastore failure on the snapshot path reports under.
const CONTEXT_SNAPSHOT: &str = "read tenant wallet";

/// The context the charges walk reports under.
const CONTEXT_CHARGES: &str = "list tenant charges";

/// The most rows one charges page may carry.
///
/// Public because the HANDLER owns the refusal sentence for a limit past this,
/// and a sentence naming a number the store does not enforce would be two
/// truths (RULE UFS).
pub const CHARGES_LIMIT_MAX: u32 = 200;

/// The page size a caller naming nothing gets.
pub const CHARGES_LIMIT_DEFAULT: u32 = 50;

/// The tenant's billing reads.
#[derive(Debug, Clone)]
pub struct Billing {
    database: Db,
}

impl Billing {
    /// A read surface over `database`.
    #[must_use]
    pub const fn new(database: Db) -> Self {
        Self { database }
    }

    /// The wallet snapshot behind `GET /v1/tenants/me/billing`.
    ///
    /// # Errors
    /// Refuses a tenant with no wallet row as the bootstrap-invariant violation
    /// it is. Reports a datastore that would not answer.
    pub async fn snapshot(&self, tenant: &Uuid7) -> Result<Wallet> {
        let mut connection = self.database.acquire().await?;
        let wallet: Option<Wallet> = sqlx::query_as(sql::SELECT_WALLET)
            .bind(tenant.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_SNAPSHOT))?;
        wallet.ok_or_else(error::billing_wallet_missing)
    }

    /// One page of the tenant's charges, newest first.
    ///
    /// `boundary` is the decoded cursor, when the caller is resuming — the
    /// handler parses the token so a malformed one is refused before a
    /// connection is drawn.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a row this daemon cannot
    /// read.
    pub async fn charges(
        &self,
        tenant: &Uuid7,
        limit: u32,
        boundary: Option<&cursor::Boundary>,
    ) -> Result<Vec<ChargeRow>> {
        let mut connection = self.database.acquire().await?;
        let query = match boundary {
            None => sqlx::query_as(sql::SELECT_TENANT_CHARGES_FIRST)
                .bind(tenant.as_str())
                .bind(i64::from(limit)),
            Some(after) => sqlx::query_as(sql::SELECT_TENANT_CHARGES_AFTER)
                .bind(tenant.as_str())
                .bind(after.created_at.to_sqlx())
                .bind(after.id.as_str())
                .bind(i64::from(limit)),
        };
        query
            .fetch_all(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_CHARGES))
    }
}

/// One `billing.fleet_charges` row, as the charges response shows it.
///
/// Decoded by name through `FromRow`, so each field is its column's name.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ChargeRow {
    /// The charge row's identifier.
    pub id: String,
    /// Whose charge it is.
    pub tenant_id: String,
    /// The workspace it was incurred in. No foreign key, so it outlives the
    /// workspace.
    pub workspace_id: String,
    /// The fleet it was incurred by, and it OUTLIVES that fleet.
    ///
    /// No foreign key and part of the accumulate arbiter. The dashboard
    /// derives a fleet's callsign from this value, so a purged fleet's charges
    /// still name themselves.
    pub fleet_id: String,
    /// That fleet's name as it stood when the charge was written. A snapshot,
    /// so a later rename never reaches it. A fleet purged before the charge
    /// landed leaves its identifier here, equal to `fleet_id`.
    pub fleet_name: String,
    /// The event that triggered the work.
    pub event_id: String,
    /// `receive` or `stage`: the two halves of one event's cost.
    pub charge_type: String,
    /// Whose model bill the tokens landed on.
    pub posture: String,
    /// The model the stage ran against.
    pub model: String,
    /// What this row drained, in nanos.
    pub charged_nanos: i64,
    /// Tokens in, on stage rows that have settled.
    pub input_token_count: Option<i64>,
    /// Tokens out, likewise.
    pub output_token_count: Option<i64>,
    /// How long the stage ran.
    pub wall_ms: Option<i64>,
    /// When the row was written: the walk's sort key.
    #[sqlx(try_from = "jiff_sqlx::Timestamp")]
    pub created_at: Timestamp,
}
