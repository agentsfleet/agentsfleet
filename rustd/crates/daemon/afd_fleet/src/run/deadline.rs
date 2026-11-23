//! The immutable ceiling a task binding receipt carries.

use crate::error::{Result, query};
use af_core::id::Uuid7;

/// The run's durable-task execution ceiling: `None` for a run bound to no
/// task, which no clock ends.
///
/// Called only inside the transaction already holding this run's ownership locks.
pub(crate) async fn read(
    connection: &mut sqlx::PgConnection,
    run_id: &Uuid7,
) -> Result<Option<jiff::Timestamp>> {
    let deadline: Option<jiff_sqlx::Timestamp> =
        sqlx::query_scalar(include_str!("sql/execution_deadline.sql"))
            .bind(run_id)
            .fetch_one(connection)
            .await
            .map_err(query("read immutable run execution ceiling"))?;
    Ok(deadline.map(jiff_sqlx::Timestamp::to_jiff))
}
