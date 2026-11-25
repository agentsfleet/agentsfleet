//! A resolve that finds the gate already answered, and what the standing
//! answer still owes.
//!
//! Split from [`super`] at the 350-line cap, along the seam between the
//! answer that wins the row and a retry that reads it: the winner lands the
//! answer turn once, and a retry lands it when the first call lost it.

use af_core::id::Uuid7;

use super::super::{Inbox, Resolved};
use super::{ANSWER_PRODUCER, Continues};
use crate::Result;

impl Inbox {
    /// What a gate answered before this call still owes, settled now.
    ///
    /// A retry is the one caller left to notice the first answer's turn
    /// never landed, so it lands the one the stored answer owes. The turn is
    /// keyed on the gate's action, so a retry never lands a second.
    ///
    /// # Errors
    /// Reports a ledger that would not answer, and an answer turn that would
    /// not land.
    pub(super) async fn stood(&self, mut resolved: Resolved) -> Result<Resolved> {
        if let Some(continues) = Continues::of(&resolved.status, &resolved.gate_type)
            && let Some(turn_id) = resolved.turn_id.clone()
        {
            resolved.continuation_turn_id = self
                .owed_continuation(&resolved, &turn_id, continues)
                .await?;
        }
        Ok(resolved)
    }

    /// The answer turn a waiting run is owed: the one the ledger already
    /// holds, or one landed now because the answer that won the row failed
    /// before it could.
    ///
    /// The ledger is read first so a settled gate costs one read and writes
    /// nothing. A turn that landed while its event row did not is left
    /// to the run, which writes that row on the same conflict arm.
    async fn owed_continuation(
        &self,
        resolved: &Resolved,
        turn_id: &Uuid7,
        continues: Continues,
    ) -> Result<Option<Uuid7>> {
        let landed = self
            .turns
            .find_repeated(ANSWER_PRODUCER, resolved.action_id.as_str())
            .await?;
        match landed {
            Some(admitted) => Ok(Some(admitted.id)),
            None => self.continue_from(resolved, turn_id, continues).await,
        }
    }
}
