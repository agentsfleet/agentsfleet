//! Removing a fleet: kill first, then purge.
//!
//! # Why deleting takes two steps
//!
//! A running fleet cannot be deleted. An operator marks it `killed`, which is
//! terminal and reversible by nothing, and only then may purge it. That is not
//! ceremony: the purge takes the fleet's memories and its approval history with
//! it, and a one-step delete puts an irreversible loss one mis-typed identifier
//! away.
//!
//! # What survives, and why
//!
//! `core.fleet_events` and `core.integration_grants` cascade. The memories,
//! approval gates and sessions do not, so they are deleted here, in the same
//! transaction as the parent.
//!
//! `billing.usage_ledger` survives deliberately. Its `fleet_id` is
//! `ON DELETE SET NULL`, so a charge the wallet was already debited for outlives
//! the fleet with its tenant scope intact. Erasing one would falsify the
//! reconciliation between the ledger and the wallet, and no role here holds
//! `DELETE` on that table anyway.
//!
//! # The Redis stream is best-effort, after the commit
//!
//! Postgres is the source of truth. Once the row is gone the stream is
//! unreachable — the candidate query filters on `status = 'active'` and there is
//! no row to be active — so a failed cleanup orphans keys that age out, and is
//! worth a log line rather than failing a purge that already succeeded.

use afd_core::id::Uuid7;

use crate::error::{self, ErrorKind, Result};
use crate::{FleetStatus, Fleets, sql};

/// The contexts a failed statement on this path reports under.
const CONTEXT_PROBE: &str = "read fleet status before purge";
const CONTEXT_BEGIN: &str = "open fleet purge transaction";
const CONTEXT_CHILDREN: &str = "purge fleet child rows";
const CONTEXT_PARENT: &str = "purge fleet row";
const CONTEXT_COMMIT: &str = "commit fleet purge";

/// The hint a failed stream cleanup leaves for an operator to grep.
const HINT_STREAM_ORPHANED: &str = "pg_row_purged_stream_orphaned_until_ttl";

impl Fleets {
    /// Purges one killed fleet and everything keyed to it.
    ///
    /// # Errors
    /// Refuses an id naming no fleet this workspace holds, and one that has not
    /// been killed. Reports a datastore that would not answer.
    pub async fn purge(&self, workspace: &Uuid7, fleet: &Uuid7) -> Result<()> {
        let mut connection = self.database.acquire().await?;
        let mut transaction = sqlx::Acquire::begin(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_BEGIN))?;

        // Classified before any DELETE, so a refusal never leaves half a purge
        // behind. The guarded delete below closes the window this opens.
        let stored: Option<(String,)> = sqlx::query_as(sql::SELECT_FLEET_STATUS)
            .bind(fleet.as_str())
            .bind(workspace.as_str())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(error::query(CONTEXT_PROBE))?;
        let (status,) = stored.ok_or_else(|| crate::Error::from(ErrorKind::NotFound))?;
        if FleetStatus::parse(&status) != Some(FleetStatus::Killed) {
            return Err(ErrorKind::MustKillFirst.into());
        }

        for &statement in sql::PURGE_CHILDREN {
            sqlx::query(statement)
                .bind(fleet.as_str())
                .execute(&mut *transaction)
                .await
                .map_err(error::query(CONTEXT_CHILDREN))?;
        }

        // Guarded on `killed` again, and that is not belt-and-braces: between
        // the probe above and this statement a concurrent PATCH can resurrect
        // the fleet. The guard makes that a zero-row result the transaction
        // rolls back, rather than a purge of a fleet somebody just resumed.
        let purged = sqlx::query(sql::DELETE_FLEET_IN_STATUS)
            .bind(fleet.as_str())
            .bind(workspace.as_str())
            .bind(FleetStatus::Killed.as_str())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(error::query(CONTEXT_PARENT))?;
        if purged.is_none() {
            // Dropped without a commit, so the child deletes go back too.
            return Err(ErrorKind::MustKillFirst.into());
        }
        transaction
            .commit()
            .await
            .map_err(error::query(CONTEXT_COMMIT))?;

        self.live_sets.invalidate(workspace.as_str()).await;
        self.forget_state(fleet.as_str()).await;
        Ok(())
    }

    /// Drops the fleet's Redis state, logging rather than failing.
    ///
    /// Runs AFTER the commit, deliberately. Doing it first would delete a live
    /// fleet's stream if the transaction then rolled back; doing it inside would
    /// make Postgres's atomicity depend on a second datastore.
    ///
    /// The readiness field is cleared FIRST and independently of the stream, for
    /// `purgeFleetRedisState`'s reason: the row is already gone, so a surviving
    /// entry in the shared sample can only ever be wrong, and it must not be
    /// left squatting there by a transport failure on the `DEL` that follows.
    async fn forget_state(&self, fleet: &str) {
        if let Err(failure) = self.ready.force_clear(fleet).await {
            afd_observability::producers::fleet::ready_write_failed();
            report(fleet, &failure, "purge_readiness_clear_failed");
        }
        if let Err(failure) = self.streams.forget(fleet).await {
            report(fleet, &failure, "purge_stream_cleanup_failed");
        }
    }
}

/// Logs a best-effort cleanup that did not happen.
///
/// `warn` rather than `error`: the purge SUCCEEDED — Postgres committed — and
/// what is left behind is unreachable rather than harmful. Paging somebody for
/// keys that age out on their own would train them to ignore the signal.
fn report(fleet: &str, failure: &afd_redis::Error, event: &'static str) {
    let reason = failure.to_string();
    tracing::warn!(
        error_code = afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str(),
        fleet,
        reason,
        hint = HINT_STREAM_ORPHANED,
        event,
    );
}

#[cfg(all(test, feature = "test-util"))]
mod tests {
    /// The orphan report renders every Redis failure kind.
    ///
    /// The purge already COMMITTED when this runs — Postgres is done and what
    /// is left behind is an unreachable stream that ages out on its own. So the
    /// report is the only observable thing left on this path, and a panic in it
    /// would turn a successful purge into a failed one.
    ///
    /// `failure.to_string()` is the field that runs arbitrary per-kind `Display`
    /// code, which is why every kind is walked rather than one representative.
    #[test]
    fn the_orphan_report_renders_every_redis_failure() {
        for (label, failure) in afd_redis::error::one_of_each_kind() {
            assert!(
                !failure.to_string().is_empty(),
                "{label} renders to something an operator can act on"
            );
            super::report("fleet-fixture", &failure, "fleet_purge_stream_orphaned");
        }
    }
}
