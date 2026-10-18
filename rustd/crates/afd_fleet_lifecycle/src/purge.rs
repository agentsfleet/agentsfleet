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
//! Every child row cascades from the fleet row (deletion matrix: Fleet), so the
//! purge runs one DELETE and no statement on any child. That includes the
//! approval gates, whose table grants `api_runtime` no DELETE at all: a gate
//! goes only by this cascade, which runs as the table owner, so no session
//! setting has to open a guard. It includes memory too, which `api_runtime`
//! reaches only by assuming `memory_runtime`; the cascade needs no such step,
//! and `the_purge_itself_runs_as_api_runtime` proves both through a pool that
//! holds `api_runtime` alone.
//!
//! `billing.fleet_charges` survives deliberately, WITH its attribution:
//! `fleet_id` is no foreign key, so a charge the wallet was already debited for
//! outlives the fleet still naming which fleet it paid for, beside the
//! `fleet_name` captured when the money moved. Erasing one would falsify the
//! reconciliation between the charges and the wallet. A setup retry key in
//! `authoring.replay_receipts` survives too: it carries no foreign key, so a
//! late retry finds its resource gone rather than creating it again (slot
//! 1980).
//!
//! # The Dragonfly stream is best-effort, after the commit
//!
//! Postgres is the source of truth. Once the row is gone the stream is
//! unreachable — the candidate query filters on `status = 'active'` and there is
//! no row to be active — so a failed cleanup orphans keys that age out, and is
//! worth a log line rather than failing a purge that already succeeded.

use afd_core::error_code::{Coded as _, Logged};
use afd_core::id::Uuid7;

use crate::error::{self, ErrorKind, Result};
use crate::{FleetStatus, Fleets, sql};

/// The contexts a failed statement on this path reports under.
const CONTEXT_PROBE: &str = "read fleet status before purge";
const CONTEXT_BEGIN: &str = "open fleet purge transaction";
const CONTEXT_PARENT: &str = "purge fleet row";
const CONTEXT_COMMIT: &str = "commit fleet purge";

/// The hint a failed stream cleanup leaves for an operator to grep.
const HINT_STREAM_ORPHANED: &str = "pg_row_purged_stream_orphaned_until_ttl";

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
        self.forget_state(fleet.as_str()).await;
        Ok(())
    }

    /// Drops the fleet's Dragonfly state, logging rather than failing.
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

/// Logs a best-effort cleanup that did not happen.
///
/// `warn` rather than `error`: the purge SUCCEEDED — Postgres committed — and
/// what is left behind is unreachable rather than harmful. Paging somebody for
/// keys that age out on their own would train them to ignore the signal.
fn report(fleet: &str, failure: &afd_dragonfly::Error, event: &'static str) {
    let Logged {
        error_code: cause_error_code,
        reason,
    } = failure.logged();
    tracing::warn!(
        error_code = afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str(),
        cause_error_code,
        fleet,
        reason,
        hint = HINT_STREAM_ORPHANED,
        event,
    );
}

#[cfg(all(test, feature = "test-util"))]
mod tests {
    /// The orphan report renders every Dragonfly failure kind.
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
        for (label, failure) in afd_dragonfly::error::one_of_each_kind() {
            assert!(
                !failure.to_string().is_empty(),
                "{label} renders to something an operator can act on"
            );
            super::report("fleet-fixture", &failure, "fleet_purge_stream_orphaned");
        }
    }

    #[test]
    fn a_purge_cleanup_reason_carries_no_code_prefix() {
        for (label, failure) in afd_dragonfly::error::one_of_each_kind() {
            let log = afd_core::test_util::trace::Capture::install();

            super::report("fleet-fixture", &failure, "fleet_purge_stream_orphaned");

            let line = log.only("fleet_purge_stream_orphaned");
            assert_eq!(
                line.field("error_code"),
                Some(afd_core::error_code::INTERNAL_OPERATION_FAILED.as_str()),
                "{label}"
            );
            assert_eq!(
                line.field("cause_error_code"),
                Some(failure.code().as_str()),
                "{label}"
            );
            assert_eq!(
                line.field("reason"),
                Some(failure.told().as_str()),
                "{label}"
            );
        }
    }
}
