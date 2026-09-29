//! A resolve that finds the gate already answered, and what the standing
//! answer still owes.
//!
//! Split from [`super`] at the 350-line cap, along the seam between the
//! answer that wins the row and a retry that reads it: the winner does the
//! wake or the continuation once, and a retry redoes whichever the first
//! call may have lost.

use afd_core::clock::UnixMillis;
use afd_wire::approval::status;

use super::super::row::read_resolved;
use super::super::{Inbox, Resolved};
use crate::Result;

impl Inbox {
    /// What a gate answered before this call still owes, settled now.
    ///
    /// A retry is the one caller left to notice the first answer's wake or
    /// continuation never landed, so it redoes whichever the stored answer
    /// owes: the wake for a delivery left parked, and the continuation for an
    /// approved run. Both are idempotent — a mark mints a fresh token, and the
    /// continuation is keyed on the gate's action.
    ///
    /// # Errors
    /// Reports a ledger that would not answer, and a continuation that would
    /// not land.
    pub(super) async fn stood(
        &self,
        row: &sqlx::postgres::PgRow,
        now: UnixMillis,
    ) -> Result<Resolved> {
        let mut resolved = read_resolved(row)?;
        if leaves_delivery_parked(&resolved.status, resolved.event_id.as_deref()) {
            self.wake_parked_delivery(&resolved.fleet_id, &resolved.gate_id)
                .await;
        } else if resolved.status == status::APPROVED
            && let Some(event_id) = resolved.event_id.clone()
        {
            resolved.continuation_event_id =
                self.owed_continuation(&resolved, &event_id, now).await?;
        }
        Ok(resolved)
    }

    /// The continuation an approved run is owed: the one the ledger already
    /// holds, or one landed now because the answer that won the row failed
    /// before it could.
    ///
    /// The ledger is read first so a settled gate costs one read and writes
    /// nothing. An admission that landed while its event row did not is left
    /// to the lease, which writes that row on the same conflict arm.
    async fn owed_continuation(
        &self,
        resolved: &Resolved,
        event_id: &str,
        now: UnixMillis,
    ) -> Result<Option<String>> {
        let landed = self
            .admissions
            .find_repeated(
                afd_admission::Producer::GateContinuation,
                &resolved.action_id,
            )
            .await?;
        match landed {
            Some(admitted) => Ok(Some(admitted.id)),
            None => self.continue_from(resolved, event_id, now).await,
        }
    }
}

/// Whether a gate answered `answer` leaves its delivery parked on the stream
/// for the next poll to read: a gate that held no run, and a run the answer
/// ended. Only an approval of a run moves on without one, through the
/// continuation it lands.
pub(super) fn leaves_delivery_parked(answer: &str, event_id: Option<&str>) -> bool {
    event_id.is_none() || answer == status::DENIED || answer == status::TIMED_OUT
}
