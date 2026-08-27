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
use sqlx::Row as _;

use crate::sql::billing as sql;
use crate::{Result, error};

/// The context a datastore failure on the snapshot path reports under.
const CONTEXT_SNAPSHOT: &str = "read tenant wallet";

/// The context the charges walk reports under.
const CONTEXT_CHARGES: &str = "list tenant charges";

/// The context a column this daemon cannot read reports under.
const CONTEXT_ROW: &str = "read charge row";

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
        let row: Option<(i64, i64, Option<i64>)> = sqlx::query_as(sql::SELECT_TENANT_BALANCE)
            .bind(tenant.as_str())
            .fetch_optional(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_SNAPSHOT))?;

        let (balance_nanos, updated_at_ms, exhausted_at_ms) =
            row.ok_or_else(error::billing_wallet_missing)?;
        Ok(Wallet {
            balance_nanos,
            updated_at_ms,
            exhausted_at_ms,
        })
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
            None => sqlx::query(sql::SELECT_TENANT_CHARGES_PAGE_FIRST)
                .bind(tenant.as_str())
                .bind(i64::from(limit)),
            Some(after) => sqlx::query(sql::SELECT_TENANT_CHARGES_PAGE_AFTER)
                .bind(tenant.as_str())
                .bind(after.recorded_at)
                .bind(after.id.as_str())
                .bind(i64::from(limit)),
        };
        let rows = query
            .fetch_all(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_CHARGES))?;

        rows.iter().map(ChargeRow::read).collect()
    }
}

/// The wallet, as the dashboard's Billing tab reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wallet {
    /// What remains, in nanos — one thousand-millionth of a dollar.
    pub balance_nanos: i64,
    /// When the balance last moved.
    pub updated_at_ms: i64,
    /// When the balance reached zero, if it has.
    pub exhausted_at_ms: Option<i64>,
}

/// One ledger row, as the charges response shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChargeRow {
    /// The ledger row's identifier.
    pub id: String,
    /// Whose charge it is.
    pub tenant_id: String,
    /// The workspace it was incurred in, until that workspace is deleted.
    pub workspace_id: Option<String>,
    /// The fleet it was incurred by, until that fleet is deleted.
    pub fleet_id: Option<String>,
    /// The event that triggered the work.
    pub event_id: String,
    /// `receive` or `stage` — the two halves of one event's cost.
    pub charge_type: String,
    /// Whose model bill the tokens landed on.
    pub posture: String,
    /// The model the stage ran against.
    pub model: String,
    /// What this row drained, in nanos.
    pub credit_deducted_nanos: i64,
    /// Tokens in, on stage rows that have settled.
    pub token_count_input: Option<i64>,
    /// Tokens out, likewise.
    pub token_count_output: Option<i64>,
    /// How long the stage ran.
    pub wall_ms: Option<i64>,
    /// When the row was written — the walk's sort key.
    pub recorded_at: i64,
}

impl ChargeRow {
    /// Reads one row by column name.
    ///
    /// Through [`error::query`] with one context, the way the api-key reader
    /// is: a `try_get` failure already names the column and the type it
    /// refused, so a second spelling of the column here would be the same fact
    /// twice.
    fn read(row: &sqlx::postgres::PgRow) -> Result<Self> {
        let unreadable = error::query(CONTEXT_ROW);
        Ok(Self {
            id: row.try_get("id").map_err(&unreadable)?,
            tenant_id: row.try_get("tenant_id").map_err(&unreadable)?,
            workspace_id: row.try_get("workspace_id").map_err(&unreadable)?,
            fleet_id: row.try_get("fleet_id").map_err(&unreadable)?,
            event_id: row.try_get("event_id").map_err(&unreadable)?,
            charge_type: row.try_get("charge_type").map_err(&unreadable)?,
            posture: row.try_get("posture").map_err(&unreadable)?,
            model: row.try_get("model").map_err(&unreadable)?,
            credit_deducted_nanos: row.try_get("credit_deducted_nanos").map_err(&unreadable)?,
            token_count_input: row.try_get("token_count_input").map_err(&unreadable)?,
            token_count_output: row.try_get("token_count_output").map_err(&unreadable)?,
            wall_ms: row.try_get("wall_ms").map_err(&unreadable)?,
            recorded_at: row.try_get("created_at").map_err(&unreadable)?,
        })
    }
}
