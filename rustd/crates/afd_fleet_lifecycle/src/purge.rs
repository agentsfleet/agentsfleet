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
//! # What goes, and what stays
//!
//! Every other child row cascades from the fleet row (deletion matrix: Fleet), so
//! the purge runs no statement on them. That includes the
//! approval gates, whose table grants `api_runtime` no DELETE at all: a gate
//! goes only by this cascade, which runs as the table owner, so no session
//! setting has to open a guard. It includes memory too, which `api_runtime`
//! reaches only by assuming `memory_runtime`; the cascade needs no such step,
//! and `the_purge_itself_runs_as_api_runtime` proves both through a pool that
//! holds `api_runtime` alone.
//!
//! Runner leases are the one child the purge deletes by hand. `runner.leases`
//! holds no foreign key to the fleet, so a SETTLED lease outlives it as runner
//! history for its own 30-day window, with the fleet shown as deleted. A LIVE
//! lease would name a fleet that no longer exists, so the purge deletes those
//! in the same transaction, before the row, as the cascade used to.
//!
//! `billing.usage_lines` survives deliberately, WITH its attribution:
//! `fleet_id` is no foreign key, so a charge the wallet was already debited for
//! outlives the fleet still naming which fleet it paid for, beside the
//! `fleet_name` captured when the money moved. Erasing one would falsify the
//! reconciliation between the charges and the wallet. A setup retry key in
//! `authoring.replay_receipts` survives too, for the rest of its 24-hour
//! window: it carries no foreign key, so a late retry finds its resource gone
//! rather than creating it again (slot 1980).
//!
use afd_core::id::Uuid7;
use afd_state::sql::LEASE_STATUS_ACTIVE;

use crate::error::{self, ErrorKind, Result};
use crate::{FleetStatus, Fleets, sql};

/// The contexts a failed statement on this path reports under.
const CONTEXT_PROBE: &str = "read fleet status before purge";
const CONTEXT_BEGIN: &str = "open fleet purge transaction";
const CONTEXT_PARENT: &str = "purge fleet row";
const CONTEXT_LEASES: &str = "purge fleet live leases";
const CONTEXT_COMMIT: &str = "commit fleet purge";

impl Fleets {
    /// Purges one killed fleet and everything keyed to it, under the same
    /// current resource version native reads return.
    ///
    /// # Errors
    /// Refuses stale tags, foreign fleets, live fleets and datastore failures.
    pub async fn purge_tagged(
        &self,
        workspace: &Uuid7,
        fleet: &Uuid7,
        expected: &str,
    ) -> Result<()> {
        let mut connection = self.database.acquire().await?;
        let mut transaction = sqlx::Acquire::begin(&mut *connection)
            .await
            .map_err(error::query(CONTEXT_BEGIN))?;
        guard(&mut transaction, workspace, fleet, expected).await?;
        sqlx::query(sql::DELETE_ACTIVE_LEASES)
            .bind(fleet.as_str())
            .bind(LEASE_STATUS_ACTIVE)
            .execute(&mut *transaction)
            .await
            .map_err(error::query(CONTEXT_LEASES))?;
        let purged = sqlx::query_scalar::<_, String>(sql::DELETE_FLEET_IN_STATUS)
            .bind(fleet.as_str())
            .bind(workspace.as_str())
            .bind(FleetStatus::Killed.as_str())
            .fetch_optional(&mut *transaction)
            .await
            .map_err(error::query(CONTEXT_PARENT))?;
        if purged.is_none() {
            return Err(ErrorKind::MustKillFirst.into());
        }
        transaction
            .commit()
            .await
            .map_err(error::query(CONTEXT_COMMIT))?;
        self.live_sets.invalidate(workspace.as_str()).await;
        Ok(())
    }
}

/// Locks the row and checks the tag and the status under the lock, so no
/// concurrent edit lands between the check and the delete.
async fn guard(
    connection: &mut sqlx::PgConnection,
    workspace: &Uuid7,
    fleet: &Uuid7,
    expected: &str,
) -> Result<()> {
    let locked = sqlx::query_as::<_, LockedFleet>(sql::LOCK_FLEET)
        .bind(fleet.as_str())
        .bind(workspace.as_str())
        .fetch_optional(connection)
        .await
        .map_err(error::query(CONTEXT_PROBE))?
        .ok_or(ErrorKind::NotFound)?;
    let current = afd_core::etag::versioned(fleet, locked.resource_version);
    if expected != current {
        return Err(error::source_stale(current));
    }
    if FleetStatus::parse(&locked.status) != Some(FleetStatus::Killed) {
        return Err(ErrorKind::MustKillFirst.into());
    }
    Ok(())
}

/// The row [`sql::LOCK_FLEET`] holds while the tag is compared.
#[derive(Debug, sqlx::FromRow)]
struct LockedFleet {
    status: String,
    resource_version: i64,
}
