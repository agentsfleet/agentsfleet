//! Removing a fleet, in whatever status it holds.
//!
//! # Why one step
//!
//! Archive and delete are separate operations a person picks between: archive
//! keeps the history, delete removes it, and neither waits on the other, so a
//! delete takes the fleet from any status, including `active`. A delete that
//! also ends the fleet's setup and live work runs [`delete_locked`] inside the
//! transaction that does that teardown (`Creations::delete`), so all of it
//! commits or none does. A live lease that survives is deleted here, and its
//! runner learns at its next renewal, which answers `RUN_LEASE_NOT_FOUND`.
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
use af_core::id::Uuid7;
use afd_state::sql::LEASE_STATUS_ACTIVE;

use crate::error::{self, ErrorKind, Result};
use crate::sql;

/// The contexts a failed statement on this path reports under.
const CONTEXT_PARENT: &str = "purge fleet row";
const CONTEXT_LEASES: &str = "purge fleet live leases";

/// Deletes `fleet`'s live leases, then its row, inside the caller's
/// transaction.
///
/// The only delete path: the caller already holds the row lock and has
/// checked the tag, so a delete that also ends the fleet's setup and work
/// commits all of it or none of it. It commits nothing itself; after the
/// caller commits, it owes [`crate::Fleets::invalidate_live_set`].
///
/// # Errors
/// Reports a fleet that is not in `workspace` and datastore failures.
pub async fn delete_locked(
    connection: &mut sqlx::PgConnection,
    workspace: &Uuid7,
    fleet: &Uuid7,
) -> Result<()> {
    sqlx::query(sql::DELETE_ACTIVE_LEASES)
        .bind(fleet)
        .bind(LEASE_STATUS_ACTIVE)
        .execute(&mut *connection)
        .await
        .map_err(error::query(CONTEXT_LEASES))?;
    sqlx::query_scalar::<_, Uuid7>(sql::DELETE_FLEET)
        .bind(fleet)
        .bind(workspace)
        .fetch_optional(connection)
        .await
        .map_err(error::query(CONTEXT_PARENT))?
        .ok_or(ErrorKind::NotFound)?;
    Ok(())
}
