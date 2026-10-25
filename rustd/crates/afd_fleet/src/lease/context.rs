//! Context writes prove the live assignment on the same transaction as publication.
use super::{Leases, sql, standing::LiveLease};
use crate::{
    Result,
    error::{lease_not_found, query, row_malformed, stale_fence},
};
use afd_context::{
    Checkpoint, CheckpointStore as _,
    repository::{Repository, Scope},
};
use afd_core::id::Uuid7;
use sqlx::Connection as _;

const CONTEXT: &str = "access private run context checkpoint";

impl Leases {
    /// Read private notes only while this runner still holds the assignment.
    ///
    /// # Errors
    /// An absent/stale assignment or unavailable storage.
    pub async fn context_load(
        &self,
        runner: &Uuid7,
        lease: &Uuid7,
        branch: &Uuid7,
        fence: u64,
    ) -> Result<Option<Checkpoint>> {
        let mut connection = self.pool().acquire().await?;
        let mut tx = connection.begin().await.map_err(query(CONTEXT))?;
        let scope = scope(&mut tx, runner, lease, branch, fence).await?;
        let found = Repository::new(&mut tx, scope, branch.clone())
            .load()
            .await?;
        tx.commit().await.map_err(query(CONTEXT))?;
        Ok(found)
    }

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
        .bind(lease.as_str())
        .bind(runner.as_str())
        .bind(sql::LEASE_STATUS_ACTIVE)
        .fetch_optional(connection)
        .await
        .map_err(query(CONTEXT))?
        .ok_or_else(lease_not_found)?;
    if !row.holds(fence) {
        return Err(stale_fence());
    }
    let fleet = Uuid7::parse(&row.fleet_id).map_err(row_malformed("runner.leases", "fleet_id"))?;
    Ok(Scope::Run {
        lease: lease.clone(),
        fleet,
        branch: branch.clone(),
    })
}
