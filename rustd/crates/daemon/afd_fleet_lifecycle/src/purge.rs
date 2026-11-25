//! Removing a fleet, in whatever status it holds.
//!
//! # Why one step
//!
//! Archive and delete are separate operations a person picks between: archive
//! keeps the history, delete removes it, and neither waits on the other, so a
//! delete takes the fleet from any status, including `active`. A delete that
//! also ends the fleet's live work runs [`delete_locked`] inside the
//! transaction that does that teardown (`Creations::delete`), so all of it
//! commits or none does. A live run that survives is deleted here, and its
//! runner learns at its next renewal, which answers `RUN_NOT_FOUND`.
//!
//! # Schedules go first, and upstream
//!
//! A schedule row cascades with the fleet, but the external scheduler holds a
//! registration for it that no cascade reaches. The caller removes every
//! schedule upstream, and its row, before the purge (`afd_cron`'s
//! `release_fleet`); [`delete_locked`] refuses a fleet that still holds one,
//! read under the row lock a schedule insert waits on, so a schedule written
//! after that removal stops the purge instead of cascading away registered.
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
//! Runner runs are the one child the purge deletes by hand. `thread.runs`
//! holds no foreign key to the fleet, so a SETTLED run outlives it as runner
//! history, kept until a retention policy is chosen, with the fleet shown as
//! deleted. A LIVE run would name a fleet that no longer exists, so the purge
//! deletes those in the same transaction, before the row, as the cascade used
//! to.
//!
//! `billing.usage_lines` survives deliberately, WITH its attribution:
//! `fleet_id` is no foreign key, so a charge the wallet was already debited for
//! outlives the fleet still naming which fleet it paid for, beside the
//! `fleet_name` captured when the money moved. Erasing one would falsify the
//! reconciliation between the charges and the wallet. A create's retry key in
//! `thread.replay_receipts` survives too, for the rest of its 24-hour
//! window: it carries no foreign key, so a late retry finds its resource gone
//! rather than creating it again (slot 1980).
//!
use af_core::id::Uuid7;
use afd_state::sql::RUN_STATUS_ACTIVE;

use crate::error::{self, ErrorKind, Result};
use crate::sql;

/// The contexts a failed statement on this path reports under.
const CONTEXT_PARENT: &str = "purge fleet row";
const CONTEXT_RUNS: &str = "purge fleet live runs";
const CONTEXT_SCHEDULES: &str = "purge fleet schedule check";

/// Deletes `fleet`'s live runs, then its row, inside the caller's
/// transaction.
///
/// The only delete path: the caller already holds the row lock and has
/// checked the tag, so a delete that also ends the fleet's work
/// commits all of it or none of it. It commits nothing itself; after the
/// caller commits, it owes the workspace's fleet-set notice.
///
/// A fleet still holding a schedule is refused: the caller removes the
/// fleet's schedules from the external scheduler first, and the cascade must
/// not take a row whose registration may still stand.
///
/// # Errors
/// Reports a fleet that is not in `workspace`, a fleet that still holds a
/// schedule (`UZ-SCHED-006`), and datastore failures.
pub async fn delete_locked(
    connection: &mut sqlx::PgConnection,
    workspace: &Uuid7,
    fleet: &Uuid7,
) -> Result<()> {
    let scheduled: bool = sqlx::query_scalar(sql::SELECT_FLEET_HAS_SCHEDULES)
        .bind(fleet)
        .bind(workspace)
        .fetch_one(&mut *connection)
        .await
        .map_err(error::query(CONTEXT_SCHEDULES))?;
    if scheduled {
        return Err(ErrorKind::SchedulesRegistered.into());
    }
    sqlx::query(sql::DELETE_ACTIVE_RUNS)
        .bind(fleet)
        .bind(RUN_STATUS_ACTIVE)
        .execute(&mut *connection)
        .await
        .map_err(error::query(CONTEXT_RUNS))?;
    sqlx::query_scalar::<_, Uuid7>(sql::DELETE_FLEET)
        .bind(fleet)
        .bind(workspace)
        .fetch_optional(connection)
        .await
        .map_err(error::query(CONTEXT_PARENT))?
        .ok_or(ErrorKind::NotFound)?;
    Ok(())
}
