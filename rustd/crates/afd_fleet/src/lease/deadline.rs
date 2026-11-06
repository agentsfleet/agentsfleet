//! The immutable ceiling shared by lease delivery and task binding receipts.

use crate::error::{Result, query};

/// Called only inside the transaction already holding this lease's ownership locks.
pub(crate) async fn read(
    connection: &mut sqlx::PgConnection,
    lease_id: &str,
) -> Result<jiff::Timestamp> {
    let deadline: jiff_sqlx::Timestamp =
        sqlx::query_scalar(include_str!("sql/execution_deadline.sql"))
            .bind(lease_id)
            .bind(afd_core::timing::MAX_RUNTIME)
            .fetch_one(connection)
            .await
            .map_err(query("read immutable lease execution ceiling"))?;
    Ok(deadline.to_jiff())
}
