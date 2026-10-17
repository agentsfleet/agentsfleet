//! Who pays, and what they have left.
//!
//! Two single-row reads, and the whole design is in what each one's ABSENCE
//! means — because the two absences point in opposite directions.
//!
//! A workspace that resolves to no tenant is a broken foreign key. Nothing
//! about it improves by waiting, and admitting the event would mean running
//! work nobody can be charged for. That is permanent, and the gate ends the
//! event with a named label.
//!
//! A tenant with no wallet row is an operator gap — a tenant that was never
//! provisioned. The gate admits it, and that is right: refusing every event
//! for an unprovisioned tenant turns a billing-setup omission into a total
//! outage for a fleet that is otherwise healthy.
//!
//! Both are `Ok`. Neither is an `Err`, because `Err` here means one thing only
//! — the datastore would not answer — which is the distinction the gate's
//! fail-open posture is applied to and the one RULE ECL is about.

use afd_core::id::Uuid7;
use jiff::Timestamp;
use sqlx::postgres::PgRow;
use sqlx::{FromRow, Row as _};

use crate::Nanos;
use crate::error::{Result, query, row_malformed};
use crate::sql;
use crate::store::Accounts;

/// Statement name, for the context a query failure carries.
const CONTEXT_PAYER: &str = "tenant for workspace";

/// Statement name, for the context a query failure carries.
const CONTEXT_BALANCE: &str = "tenant balance";

/// The table a malformed identifier is reported against.
const TABLE_WORKSPACES: &str = "identity.workspaces";

/// The column a malformed identifier is reported against.
const COLUMN_TENANT_ID: &str = "tenant_id";

/// A tenant's credit pool, as the `billing.wallets` row holds it.
///
/// The one decode of that row: the lease gate reads it through
/// [`Accounts::wallet`] and the billing page through
/// [`crate::tenant::Billing::snapshot`], over the same statement.
///
/// A `FromRow` rather than three `try_get` calls at the call site: it folds
/// every column read into the one failure sqlx already reports for the query,
/// so the caller has a single error path and it is one a test can reach by
/// cutting the connection. Written by hand because `exhausted_at` is nullable,
/// and `#[sqlx(try_from)]` has no form for an optional bridged type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wallet {
    /// What is left to spend.
    pub balance: Nanos,
    /// When the pool last ran dry, if it ever has.
    ///
    /// Read but unused by the issue-time gate, which compares a balance against
    /// an estimate and has no use for when it was previously exhausted. It is
    /// carried because the statement is shared with the billing endpoint, and
    /// narrowing the projection here would fork one statement into two.
    pub exhausted_at: Option<Timestamp>,
    /// When the pool last moved.
    pub updated_at: Timestamp,
}

impl FromRow<'_, PgRow> for Wallet {
    fn from_row(row: &PgRow) -> sqlx::Result<Self> {
        Ok(Self {
            balance: Nanos::from_i64(row.try_get("balance_nanos")?),
            exhausted_at: row
                .try_get::<Option<jiff_sqlx::Timestamp>, _>("exhausted_at")?
                .map(jiff_sqlx::Timestamp::to_jiff),
            updated_at: row
                .try_get::<jiff_sqlx::Timestamp, _>("updated_at")?
                .to_jiff(),
        })
    }
}

impl Accounts {
    /// The tenant answering for `workspace_id`'s work.
    ///
    /// `Ok(None)` is a workspace with no tenant row — permanent, and the caller
    /// ends the event rather than retrying it.
    ///
    /// # Errors
    /// Reports a datastore that would not answer, and a `tenant_id` column
    /// holding something this daemon cannot read as an identifier.
    pub async fn payer(&self, workspace_id: &Uuid7) -> Result<Option<Uuid7>> {
        let mut connection = self.pool().acquire().await?;
        let found: Option<String> = sqlx::query_scalar(sql::SELECT_TENANT_FOR_WORKSPACE)
            .bind(workspace_id.as_str())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_PAYER))?;

        found
            .map(|id| Uuid7::parse(&id).map_err(row_malformed(TABLE_WORKSPACES, COLUMN_TENANT_ID)))
            .transpose()
    }

    /// `tenant_id`'s credit pool.
    ///
    /// `Ok(None)` is a tenant with no wallet row, which the gate ADMITS — see
    /// the module note. It is deliberately not folded into `Ok(Wallet { balance:
    /// ZERO, .. })`: a zero balance refuses every event and an absent wallet
    /// admits them, so collapsing the two would invert the behaviour for
    /// exactly the tenants least able to notice.
    ///
    /// # Errors
    /// Reports a datastore that would not answer.
    pub async fn wallet(&self, tenant_id: &Uuid7) -> Result<Option<Wallet>> {
        let mut connection = self.pool().acquire().await?;
        sqlx::query_as(sql::SELECT_WALLET)
            .bind(tenant_id.as_str())
            .fetch_optional(&mut *connection)
            .await
            .map_err(query(CONTEXT_BALANCE))
    }
}
