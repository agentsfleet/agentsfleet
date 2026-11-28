//! Lock the fleet's policy, the current thread holder and the live run in
//! parent-first order. Commit (and release every lock) before any HTTP send.

use af_core::id::Uuid7;
use af_fleet_runtime::config::HttpEgress;
use sqlx::{Acquire as _, PgConnection};

use super::{Failure, VendorName, failure::reservation};
use crate::error::{Result, run_cancelled, run_not_found, stale_fence};
use crate::run::{
    Runs,
    sql::{FLEET_STATUS_ACTIVE, RUN_STATUS_ACTIVE, standing::SELECT_STANDING_SHARED},
    standing::LiveRun,
};

pub(super) struct Claim<'a> {
    pub(super) runner: &'a Uuid7,
    pub(super) run: &'a Uuid7,
    pub(super) fence: u64,
}

impl Claim<'_> {
    pub(super) async fn authorize(
        &self,
        runs: &Runs,
        dispatch: Option<(VendorName, &'static str)>,
        deadline: tokio::time::Instant,
    ) -> Result<Uuid7> {
        tokio::time::timeout_at(deadline, self.commit(runs, dispatch))
            .await
            .map_err(reservation)?
    }

    async fn commit(
        &self,
        runs: &Runs,
        dispatch: Option<(VendorName, &'static str)>,
    ) -> Result<Uuid7> {
        let mut connection = runs.pool().acquire().await.map_err(reservation)?;
        let mut tx = connection.begin().await.map_err(reservation)?;
        let (status, policy): (String, String) = sqlx::query_as(include_str!("sql/policy.sql"))
            .bind(self.run)
            .bind(self.runner)
            .fetch_optional(&mut *tx)
            .await
            .map_err(reservation)?
            .ok_or_else(run_not_found)?;
        let _: Uuid7 = sqlx::query_scalar(include_str!("sql/thread.sql"))
            .bind(self.run)
            .bind(self.runner)
            .fetch_optional(&mut *tx)
            .await
            .map_err(reservation)?
            .ok_or_else(run_not_found)?;
        let owner = self.live(&mut tx).await?;
        if status != FLEET_STATUS_ACTIVE {
            return Err(run_cancelled());
        }
        if policy != HttpEgress::Public.as_str() {
            return Err(Failure::Restricted.into());
        }
        if let Some((vendor, operation)) = dispatch {
            sqlx::query(include_str!("sql/reserve.sql"))
                .bind(self.run)
                .bind(vendor.name())
                .bind(operation)
                .execute(&mut *tx)
                .await
                .map_err(reservation)?;
            // A competing upsert may have held the counter lock past expiry.
            self.live(&mut tx).await?;
        }
        tx.commit().await.map_err(reservation)?;
        #[cfg(any(test, feature = "test-util"))]
        if dispatch.is_some()
            && let Some(committed) = &runs.web.post_commit
        {
            drop(connection);
            committed.notify_one();
            std::future::pending::<()>().await;
        }
        Ok(owner.fleet_id)
    }

    async fn live(&self, connection: &mut PgConnection) -> Result<LiveRun> {
        let row: LiveRun = sqlx::query_as(SELECT_STANDING_SHARED)
            .bind(self.run)
            .bind(self.runner)
            .bind(RUN_STATUS_ACTIVE)
            .fetch_optional(&mut *connection)
            .await
            .map_err(reservation)?
            .ok_or_else(run_not_found)?;
        let owns: bool = sqlx::query_scalar(include_str!("sql/holds_thread.sql"))
            .bind(&row.fleet_id)
            .bind(self.runner)
            .bind(row.fence)
            .fetch_one(connection)
            .await
            .map_err(reservation)?;
        if !owns || !row.holds(self.fence) {
            return Err(stale_fence());
        }
        Ok(row)
    }
}
