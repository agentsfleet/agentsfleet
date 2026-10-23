//! Lock before checking a gate, so its answer cannot be followed by a stale park.

use crate::{
    error::{Result, query},
    lease::{
        affinity::Fence,
        sql::{self, queue},
        store::Leases,
    },
};
use afd_core::id::Uuid7;
use sqlx::Connection as _;
const CONTEXT: &str = "admission queue transition";

impl Leases {
    /// Park and free the slot atomically. An answer that already committed is
    /// visible to the second statement; an answer waiting on this lock un-parks
    /// after this transaction commits. Neither ordering can strand the row.
    ///
    /// # Errors
    /// Preserves database failures. A superseded claim changes nothing.
    pub async fn park(&self, fleet: &Uuid7, event: &str, fence: Fence) -> Result<()> {
        let mut connection = self.pool().acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT))?;
        if self
            .lock_admission_on(&mut transaction, fleet, event, fence)
            .await?
        {
            sqlx::query(queue::PARK_WAITING)
                .bind(fleet.as_str())
                .bind(event)
                .bind(fence.as_i64())
                .execute(&mut *transaction)
                .await
                .map_err(query(CONTEXT))?;
            sqlx::query(sql::lease::RELEASE_UNLEASED_SLOT)
                .bind(fleet.as_str())
                .bind(fence.as_i64())
                .execute(&mut *transaction)
                .await
                .map_err(query(CONTEXT))?;
        }
        transaction.commit().await.map_err(query(CONTEXT))?;
        Ok(())
    }

    pub(crate) async fn lock_admission_on(
        &self,
        connection: &mut sqlx::PgConnection,
        fleet: &Uuid7,
        event: &str,
        fence: Fence,
    ) -> Result<bool> {
        sqlx::query_scalar(queue::LOCK_ADMISSION)
            .bind(fleet.as_str())
            .bind(event)
            .bind(fence.as_i64())
            .fetch_one(connection)
            .await
            .map_err(query(CONTEXT))
    }

    /// Suppress a terminal event without allowing a stale claimant to clear it.
    pub(crate) async fn settle_terminal(&self, acquired: &super::Acquired) -> Result<()> {
        let mut connection = self.pool().acquire().await?;
        let mut transaction = connection.begin().await.map_err(query(CONTEXT))?;
        if self
            .lock_admission_on(
                &mut transaction,
                &acquired.fleet_id,
                &acquired.event_id,
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
            .bind(acquired.fleet_id.as_str())
            .bind(&acquired.event_id)
            .bind(acquired.fence.as_i64())
            .bind(afd_core::event::status::RECEIVED)
            .execute(connection)
            .await
            .map_err(query(CONTEXT))?;
        Ok(())
    }
}
