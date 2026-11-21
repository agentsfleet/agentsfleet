//! A run's stored checkpoint: read back when the run can place it, a setup
//! run's and a fleet chat's alike, and a setup run's published where its
//! compaction saves it.

use af_context::{Checkpoint, CheckpointStore, Identity, PortFuture};
use af_wire::context::ContextSaveRequest;

use super::SetupContext;

const EVENT_IGNORED: &str = "context_checkpoint_ignored";
const EVENT_UNREADABLE: &str = "context_checkpoint_unreadable";
const EVENT_SAVED: &str = "setup_context_saved";

/// The stored `document`, when it is a valid checkpoint of `identity`. A
/// document that does not parse or validate is a corrupt store and logs a
/// warning; a valid one written for another identity is expected after a
/// model or prompt change and logs at `info`.
pub(crate) fn usable(
    document: serde_json::Value,
    identity: &Identity,
    run_id: &str,
) -> Option<Checkpoint> {
    let read = serde_json::from_value::<Checkpoint>(document)
        .map_err(crate::Error::from)
        .and_then(|checkpoint| {
            checkpoint.validate_document()?;
            Ok(checkpoint)
        });
    let checkpoint = match read {
        Ok(checkpoint) => checkpoint,
        Err(corrupt) => {
            // The code alone: a parse failure can quote the stored text.
            let error_code = corrupt.code().as_str();
            tracing::warn!(event = EVENT_UNREADABLE, error_code, run_id);
            return None;
        }
    };
    let placeable = Some(checkpoint).filter(|checkpoint| checkpoint.belongs_to(identity));
    if placeable.is_none() {
        tracing::info!(event = EVENT_IGNORED, run_id);
    }
    placeable
}

/// Publishes a compaction's checkpoint: through the context verb at the
/// version the run last read or saved, or in memory for a run that read none.
pub(super) struct Store<'s> {
    pub(super) context: &'s dyn SetupContext,
    pub(super) fencing_token: u64,
    pub(super) saving: bool,
    pub(super) version: &'s mut u64,
}

impl CheckpointStore for Store<'_> {
    fn commit(&mut self, expected_generation: u64, checkpoint: Checkpoint) -> PortFuture<'_, ()> {
        Box::pin(async move {
            if expected_generation != *self.version {
                return Err(af_context::Error::stale());
            }
            if !self.saving {
                *self.version = checkpoint.generation;
                return Ok(());
            }
            let request = ContextSaveRequest {
                fencing_token: self.fencing_token,
                resource_version: expected_generation,
                checkpoint: serde_json::to_value(&checkpoint)
                    .map_err(af_context::Error::boundary)?,
            };
            let saved = self
                .context
                .save(request)
                .await
                .map_err(af_context::Error::boundary)?;
            *self.version = saved.resource_version;
            let (generation, covered_through) = (checkpoint.generation, checkpoint.covered_through);
            tracing::info!(event = EVENT_SAVED, generation, covered_through);
            Ok(())
        })
    }
}
