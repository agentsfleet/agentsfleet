//! Funds checks retain unmeasured physical calls without charging logged usage twice.

use af_core::id::Uuid7;
use afd_billing::Nanos;
use sqlx::PgConnection;

use crate::{Result, error::query};

pub(super) async fn required_on(
    connection: &mut PgConnection,
    lease: &Uuid7,
    coding_headroom: Nanos,
) -> Result<Nanos> {
    let unresolved: i64 = sqlx::query_scalar(include_str!("sql/required_credit.sql"))
        .bind(lease.as_str())
        .fetch_one(connection)
        .await
        .map_err(query("read unresolved physical decision credit"))?;
    // The biller already subtracts logged unsettled lines from spendable credit.
    Ok(Nanos::from_i64(unresolved).saturating_add(coding_headroom))
}
