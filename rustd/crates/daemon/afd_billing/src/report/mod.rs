//! Settled usage reports over the existing ledger; no HTTP and no money writes.

mod queries;
mod types;

pub use types::{
    Activity, Aggregate, Boundary, Charge, Filter, Report, Request, Window, dimension,
};

#[cfg(feature = "test-util")]
mod test_util;
#[cfg(feature = "test-util")]
pub use test_util::{activities_query, aggregates_query, charges_query};

/// The selected Coordinated Universal Time (UTC) calendar month through the reporting cutoff.
pub const BILLING_PERIOD_MONTH: &str = "month";
/// The selected UTC calendar day through the reporting cutoff.
pub const BILLING_PERIOD_TODAY: &str = "today";
/// A billed fleet lease, as the reporting API names it.
pub const BILLING_ACTIVITY_RUNTIME: &str = "runtime";
/// A private setup attempt, as the reporting API names it.
pub const BILLING_ACTIVITY_SETUP: &str = "setup";

use afd_core::id::Uuid7;
use afd_db::Db;

use crate::biller::WorkKey;
use crate::{Result, error};

const CONTEXT_READ: &str = "read settled usage report";
const CONTEXT_DETAILS: &str = "read settled activity charges";

/// Read aggregates and the activity page from the same database image.
pub(crate) async fn read(database: &Db, tenant: &Uuid7, request: &Request) -> Result<Report> {
    let mut connection = database.acquire().await?;
    let mut transaction = sqlx::Connection::begin(connection.as_mut())
        .await
        .map_err(error::query(CONTEXT_READ))?;
    sqlx::query(include_str!("sql/snapshot.sql"))
        .execute(&mut *transaction)
        .await
        .map_err(error::query(CONTEXT_READ))?;
    let aggregates = queries::aggregates(|statement| statement, tenant, request)
        .fetch_all(&mut *transaction)
        .await
        .map_err(error::query(CONTEXT_READ))?;
    let activities = queries::activities(|statement| statement, tenant, request)
        .fetch_all(&mut *transaction)
        .await
        .map_err(error::query(CONTEXT_READ))?;
    transaction
        .commit()
        .await
        .map_err(error::query(CONTEXT_READ))?;
    Ok(Report {
        aggregates,
        activities,
    })
}

/// Read one work item's settled lines without exposing whether foreign work exists.
pub(crate) async fn details(
    database: &Db,
    tenant: &Uuid7,
    work: &WorkKey,
    request: &Request,
    after: Option<i64>,
) -> Result<Vec<Charge>> {
    let mut connection = database.acquire().await?;
    queries::charges(|statement| statement, tenant, work, request, after)
        .fetch_all(connection.as_mut())
        .await
        .map_err(error::query(CONTEXT_DETAILS))
}
