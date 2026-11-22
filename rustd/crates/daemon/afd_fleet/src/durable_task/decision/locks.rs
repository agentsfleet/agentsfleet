//! Decision accounting takes its financial parents in the lifecycle lock order.

use af_core::id::Uuid7;
use sqlx::PgConnection;

use super::{row::Call, sql};
use crate::{Result, error::query};

/// Takes the financial parent locks of `call`, in the lifecycle order.
pub(super) async fn parents(connection: &mut PgConnection, call: &Call) -> Result<()> {
    sqlx::query_scalar::<_, Uuid7>(sql::LOCK_PARENT)
        .bind(&call.fleet_id)
        .bind(&call.workspace_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query("protect any remaining decision parent"))?;
    sqlx::query_scalar::<_, Uuid7>(crate::durable_task::sql::LOCK_SCOPE_THREAD)
        .bind(&call.fleet_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(
            "serialize decision accounting with physical issuance",
        ))?;
    if let Some(task) = &call.durable_task_id {
        sqlx::query_scalar::<_, i64>(sql::LOCK_DURABLE_TASK)
            .bind(task)
            .bind(&call.fleet_id)
            .bind(&call.workspace_id)
            .fetch_optional(connection)
            .await
            .map_err(query("lock any remaining decision budget owner"))?;
    }
    Ok(())
}
