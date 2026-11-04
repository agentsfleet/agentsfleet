//! Maintenance skips contention in the ordinary financial parent-lock order.

use sqlx::PgConnection;

use super::{row::Call, sql};
use crate::{Result, error::query};

#[derive(Clone, Copy)]
pub(super) enum Mode {
    Wait,
    Nowait,
}

impl Mode {
    pub(super) const fn statement(
        self,
        waiting: &'static str,
        immediate: &'static str,
    ) -> &'static str {
        match self {
            Self::Wait => waiting,
            Self::Nowait => immediate,
        }
    }

    pub(super) async fn parents(self, connection: &mut PgConnection, call: &Call) -> Result<()> {
        sqlx::query_scalar::<_, String>(self.statement(sql::LOCK_PARENT, sql::TRY_PARENT))
            .bind(&call.fleet_id)
            .bind(&call.workspace_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(query("protect any remaining decision parent"))?;
        sqlx::query_scalar::<_, String>(
            self.statement(crate::task::sql::LOCK_SCOPE_SLOT, sql::TRY_SLOT),
        )
        .bind(&call.fleet_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(query(
            "serialize decision accounting with physical issuance",
        ))?;
        if let Some(task) = &call.task_id {
            sqlx::query_scalar::<_, String>(self.statement(sql::LOCK_TASK, sql::TRY_TASK))
                .bind(task)
                .bind(&call.fleet_id)
                .bind(&call.workspace_id)
                .fetch_optional(connection)
                .await
                .map_err(query("lock any remaining decision budget owner"))?;
        }
        Ok(())
    }
}
