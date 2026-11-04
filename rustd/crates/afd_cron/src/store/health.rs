//! A database snapshot for the scheduler dashboard, outside export callbacks.
use super::Schedules;
use crate::{Result, error, sql};
use jiff::Timestamp;
use jiff_sqlx::ToSqlx as _;

#[derive(sqlx::FromRow)]
struct Health {
    active: i64,
    paused: i64,
    deleting: i64,
    synced: i64,
    syncing: i64,
    failed: i64,
    overdue_once: i64,
}
impl Schedules {
    /// Counts actual intent, sync and overdue-one-shot states in that order.
    ///
    /// # Errors
    /// Reports a datastore that cannot answer the snapshot query.
    pub async fn health(&self, now: Timestamp) -> Result<[u64; 7]> {
        let mut connection = self.database.acquire().await?;
        let row: Health = sqlx::query_as(sql::HEALTH)
            .bind(now.to_sqlx())
            .fetch_one(connection.as_mut())
            .await
            .map_err(error::query(super::CONTEXT_READ))?;
        // PostgreSQL COUNT is nonnegative; do not invent a reading if a driver violates it.
        let values = [
            row.active,
            row.paused,
            row.deleting,
            row.synced,
            row.syncing,
            row.failed,
            row.overdue_once,
        ];
        counts(values)
    }
}

fn counts(values: [i64; 7]) -> Result<[u64; 7]> {
    let mut counts = [0; 7];
    for (value, count) in values.into_iter().zip(&mut counts) {
        *count = u64::try_from(value)
            .map_err(|_invalid_count| error::row_unreadable("schedule count"))?;
    }
    Ok(counts)
}

#[cfg(test)]
mod tests {
    #[test]
    #[expect(clippy::expect_used, reason = "invalid driver data must fail loudly")]
    fn a_negative_driver_count_is_refused_instead_of_becoming_zero() {
        let refused = super::counts([-1, 0, 0, 0, 0, 0, 0]);
        let error = refused.expect_err("negative COUNT is not a reading");
        assert!(error.to_string().contains("schedule count"));
    }
}
