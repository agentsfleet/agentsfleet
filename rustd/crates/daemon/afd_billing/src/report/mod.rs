//! Settled usage reports over the existing ledger; no HTTP and no money writes.

mod queries;
mod types;

pub use types::{Aggregate, Filter, Report, Request, Window, dimension};

#[cfg(feature = "test-util")]
mod test_util;
#[cfg(feature = "test-util")]
pub use test_util::aggregates_query;

/// The selected Coordinated Universal Time (UTC) calendar month through the reporting cutoff.
pub const BILLING_PERIOD_MONTH: &str = "month";
/// The selected UTC calendar day through the reporting cutoff.
pub const BILLING_PERIOD_TODAY: &str = "today";
/// Seven UTC calendar days ending at the reporting cutoff.
pub const BILLING_PERIOD_WEEK: &str = "week";
/// A billed fleet run, as the reporting API names it.
pub const BILLING_ACTIVITY_RUNTIME: &str = "runtime";
/// A private setup attempt, as the reporting API names it.
pub const BILLING_ACTIVITY_SETUP: &str = "setup";

use af_core::id::Uuid7;
use afd_db::Db;

use crate::{Result, error};

const CONTEXT_READ: &str = "read settled usage report";

/// Read the period's totals, groups and choices from the daily usage rollup.
pub(crate) async fn read(database: &Db, tenant: &Uuid7, request: &Request) -> Result<Report> {
    let mut connection = database.acquire().await?;
    let rows: Vec<types::AggregateRow> =
        queries::aggregates(|statement| statement, tenant, request)
            .fetch_all(connection.as_mut())
            .await
            .map_err(error::query(CONTEXT_READ))?;
    Ok(Report {
        aggregates: rows.into_iter().map(Aggregate::from).collect(),
    })
}
