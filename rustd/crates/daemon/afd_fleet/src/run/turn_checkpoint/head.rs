//! Absence permits a clean start; present but incompatible state is an error.

use af_wire::run::TurnResume;

use super::{TurnCheckpoints, TurnRun, allocation, guard::TurnRunGuard, row::Plan, transaction};
use crate::{
    Result,
    durable_task::{
        checkpoint::StoredObject,
        storage::{CheckpointKeys, Held},
    },
    error::query,
    run::Runs,
};

impl TurnCheckpoints {
    /// Resolve the event's last committed generation under the newly issued run.
    ///
    /// A head whose workspace key is missing, will not open or did not seal it
    /// is no checkpoint: the workspace's generations are retired and the run
    /// starts from its original message.
    ///
    /// # Errors
    /// Refuses stale authority or incompatible present state; datastore failures
    /// never become a missing checkpoint or permission to start a fresh model run.
    pub async fn resume(&self, runs: &Runs, run: TurnRun<'_>) -> Result<Option<TurnResume>> {
        let mut connection = transaction::acquire(runs.pool()).await?;
        let mut owner = TurnRunGuard::lock(runs, &mut connection, run).await?;
        let Some(id) = allocation::head(&mut owner).await? else {
            owner.commit().await?;
            transaction::recycle(connection);
            return Ok(None);
        };
        let plan = Plan::load(&mut owner, &id).await?;
        let workspace = owner.current.workspace_id.clone();
        let descriptor = if self.readable(&mut owner, &plan).await? {
            Some(plan.descriptor(&owner)?)
        } else {
            None
        };
        owner.commit().await?;
        transaction::recycle(connection);
        if descriptor.is_none() {
            self.keys.retire(&workspace).await?;
        }
        Ok(descriptor)
    }

    /// Whether the workspace's present key sealed this generation, judged by
    /// one of its objects' identities; a generation with no files is judged
    /// when its root is opened.
    async fn readable(&self, owner: &mut TurnRunGuard<'_>, plan: &Plan) -> Result<bool> {
        let workspace = owner.current.workspace_id.clone();
        let Held::Usable(key) = self.keys.for_restore(&mut owner.tx, &workspace).await? else {
            return Ok(false);
        };
        let object: Option<StoredObject> = sqlx::query_as(include_str!("sql/head_object.sql"))
            .bind(&plan.id)
            .bind(&workspace)
            .fetch_optional(&mut *owner.tx)
            .await
            .map_err(query("read one object of the turn recovery head"))?;
        let sealed = object.is_none_or(|object| object.sealed_under(&key));
        if !sealed {
            CheckpointKeys::mismatched(&workspace);
        }
        Ok(sealed)
    }
}
