//! Lock the turn before settling a terminal redelivery, so a stale holder
//! cannot clear what a newer assignment holds.

use crate::{
    error::{Result, query},
    run::{affinity::Fence, sql::queue, store::Runs},
};
use af_core::id::Uuid7;
use sqlx::Connection as _;

const CONTEXT: &str = "turn queue transition";

impl Runs {
    pub(crate) async fn lock_turn_on(
        &self,
        connection: &mut sqlx::PgConnection,
        fleet: &Uuid7,
        event: &Uuid7,
        fence: Fence,
    ) -> Result<bool> {
        sqlx::query_scalar(queue::LOCK_TURN)
            .bind(fleet)
            .bind(event)
            .bind(fence.as_i64())
            .fetch_one(connection)
            .await
            .map_err(query(CONTEXT))
    }

    /// Suppress a terminal event without allowing a stale holder to clear it.
    pub(crate) async fn settle_terminal(&self, acquired: &super::Acquired) -> Result<()> {
        let mut connection = self.pool().acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT))?;
        if self
            .lock_turn_on(
                &mut transaction,
                &acquired.fleet_id,
                &acquired.turn_id,
                acquired.fence,
            )
            .await?
        {
            self.settle_terminal_on(&mut transaction, acquired).await?;
        }
        transaction.commit().await.map_err(query(CONTEXT))?;
        Ok(())
    }

    pub(crate) async fn settle_terminal_on(
        &self,
        connection: &mut sqlx::PgConnection,
        acquired: &super::Acquired,
    ) -> Result<()> {
        sqlx::query(queue::SETTLE_TERMINAL)
            .bind(&acquired.fleet_id)
            .bind(&acquired.turn_id)
            .bind(acquired.fence.as_i64())
            .bind(af_core::turn::status::RECEIVED)
            .execute(connection)
            .await
            .map_err(query(CONTEXT))?;
        Ok(())
    }
}
