//! What a save's answer does to the run: a receipt advances the root's
//! generation, a publication still running holds the next boundary off, and
//! the run's end waits for it.

use super::{Ending, Harness, recovery_boundary::skipped};
use crate::engine::RecoveryWitness;
use crate::{CommittedRecovery, RecoverySave, Result};

/// Where the run's last save stands at a boundary.
pub(super) enum Owner {
    /// No publication runs.
    Free,
    /// The last save is still publishing.
    Publishing,
    /// The last save's receipt ends the run.
    Ended(Ending),
}

impl Harness<'_, '_> {
    /// Takes the receipt of a publication that has ended, and says whether
    /// one still runs.
    pub(super) fn save_owner(&mut self) -> Owner {
        let Some((mut publication, expected)) = self.publication.take() else {
            return Owner::Free;
        };
        match publication.try_settled() {
            None => {
                self.publication = Some((publication, expected));
                Owner::Publishing
            }
            Some(published) => self
                .accept(published, expected)
                .map_or(Owner::Free, Owner::Ended),
        }
    }

    /// Waits for the run's last publication, so the run settles only after
    /// its commit; a receipt that stops the run replaces `ending`.
    pub(super) async fn settle_publication(&mut self, ending: &mut Ending) {
        while let Some((publication, expected)) = self.publication.take() {
            if let Some(stopped) = self.accept(publication.settled().await, expected) {
                *ending = stopped;
            }
        }
    }

    /// Applies a save's answer; the ending it calls for, when it ends the run.
    pub(super) fn accept(
        &mut self,
        saved: Result<RecoverySave>,
        expected: RecoveryWitness,
    ) -> Option<Ending> {
        match saved {
            Ok(RecoverySave::Skipped(reason)) => skipped(reason),
            Ok(RecoverySave::Publishing(publication)) => {
                self.publication = Some((publication, expected));
            }
            Ok(RecoverySave::Committed(receipt)) if receipt.matches(&expected) => {
                committed(&receipt);
                if let Some(current) = self
                    .task
                    .as_mut()
                    .and_then(|task| task.inputs.current_durable_task.as_mut())
                {
                    current.checkpoint_id = Some(receipt.checkpoint.id().to_owned().into());
                    current.checkpoint_generation = Some(receipt.checkpoint.generation());
                }
                if let CommittedRecovery::Turn(descriptor) = receipt.checkpoint {
                    self.turn_checkpoint = Some(descriptor);
                }
            }
            Ok(RecoverySave::Committed(_)) => {
                self.durable_task_report = None;
                tracing::warn!(event = "run_checkpoint_receipt_mismatch", error_code = %af_core::error_code::RUN_CHECKPOINT_INCOMPATIBLE);
                return Some(Ending::Stopped);
            }
            Err(failure) => {
                self.durable_task_report = None;
                tracing::warn!(event = "run_checkpoint_cleanup_failed", error_code = %failure.code());
                return Some(Ending::Stopped);
            }
        }
        None
    }
}

fn committed(receipt: &crate::RecoveryReceipt) {
    let run_id = receipt.run_id.as_str();
    let turn_id = receipt.turn_id.as_str();
    let fencing_token = receipt.fencing_token;
    let checkpoint_id = receipt.checkpoint.id();
    let generation = receipt.checkpoint.generation();
    tracing::debug!(
        event = "respawn_checkpoint_committed",
        run_id,
        turn_id,
        fencing_token,
        checkpoint_id,
        generation
    );
}
