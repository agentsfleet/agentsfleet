//! The tenant's money, read-only: the wallet snapshot and the charges ledger.
//!
//! # Why this module cannot move money
//!
//! Runtime renewals and reports, plus setup settlement, debit the wallet.
//! Signup bootstrap adds the starter grant. This module holds none of those
//! writes; it is the read side those movements are
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
                .bind(i64::from(limit))
                .bind(sql::CHARGE_TYPE_AUTHORING)
                .bind(sql::work::FLEET_LEASE)
                .bind(sql::charge::STAGE),
            Some(after) => sqlx::query_as(sql::SELECT_TENANT_CHARGES_AFTER)
                .bind(tenant.as_str())
                .bind(after.created_at.to_sqlx())
                .bind(after.id.as_str())
                .bind(i64::from(limit))
                .bind(sql::CHARGE_TYPE_AUTHORING)
                .bind(sql::work::FLEET_LEASE)
                .bind(sql::charge::STAGE),
        };
        query
            .fetch_all(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_CHARGES))
    }
}

/// One fleet charge or setup settlement, as the charges response shows it.
///
/// Decoded by name through `FromRow`, so each field is its column's name.
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ChargeRow {
    /// The charge's or settlement's identifier.
    pub id: String,
    /// Whose charge it is.
    pub tenant_id: String,
    /// The workspace it was incurred in. No foreign key on either source, so
    /// it outlives the workspace.
    pub workspace_id: String,
    /// The fleet it was incurred by. A fleet charge always names it, and the
    /// name OUTLIVES the fleet: no foreign key, so a purged fleet's charges
    /// still name themselves. A setup settlement names the fleet its draft was
    /// saved as, and nothing while the draft has none.
    pub fleet_id: Option<String>,
    /// That fleet's name. A fleet charge's is a snapshot taken when it was
    /// written, so a later rename never reaches it, and a fleet purged before
    /// the charge landed leaves its identifier here, equal to `fleet_id`. A
    /// setup settlement's is the saved fleet's current name.
    pub fleet_name: Option<String>,
    /// The event that triggered the work; a setup chat has none.
    pub event_id: Option<String>,
    /// The setup attempt a settlement paid for; a fleet charge has none.
    pub authoring_attempt_id: Option<String>,
    /// `stage` for one usage line of a fleet run, or
    /// [`sql::CHARGE_TYPE_AUTHORING`] for a setup chat.
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
