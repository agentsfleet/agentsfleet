//! Context writes prove the live assignment on the same transaction as publication.
use super::{Leases, sql, standing::LiveLease};
use crate::{
    Result,
    error::{lease_not_found, query, stale_fence},
};
use af_context::{
    Checkpoint, CheckpointStore as _,
    repository::{Repository, Scope},
};
use af_core::id::Uuid7;
use sqlx::Connection as _;

const CONTEXT: &str = "publish private run context checkpoint";

impl Leases {
    /// Publish against a generation under the live lease's publication lock.
    /// Repeating the exact document acknowledges the existing row.
    ///
    /// # Errors
    /// Changed assignment, mismatched private scope, stale generation or storage.
    pub async fn context_commit(
        &self,
        runner: &Uuid7,
        lease: &Uuid7,
        branch: &Uuid7,
        fence: u64,
        expected: u64,
        checkpoint: Checkpoint,
    ) -> Result<()> {
        let mut connection = self.pool().acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let scope = scope(&mut tx, runner, lease, branch, fence).await?;
        let mut store = Repository::new(&mut tx, scope, branch.clone());
        if let Err(error) = store.commit(expected, checkpoint).await {
            if error.is_stale() {
                return Err(stale_fence());
            }
            return Err(error.into());
        }
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(())
    }
}

async fn scope(
    connection: &mut sqlx::PgConnection,
    runner: &Uuid7,
    lease: &Uuid7,
    branch: &Uuid7,
    fence: u64,
) -> Result<Scope> {
    let row: LiveLease = sqlx::query_as(sql::standing::SELECT_STANDING_SHARED)
        .bind(lease)
        .bind(runner)
        .bind(sql::LEASE_STATUS_ACTIVE)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?
        .ok_or_else(lease_not_found)?;
    if !row.holds(fence) {
        return Err(stale_fence());
    }
    let fleet = row.fleet_id;
    Ok(Scope::Run {
        lease: lease.clone(),
        fleet,
        branch: branch.clone(),
    })
}
