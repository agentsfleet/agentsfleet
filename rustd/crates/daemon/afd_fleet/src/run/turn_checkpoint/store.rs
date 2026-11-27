//! One encryption owner; each operation receives the connection-bound run handle.

use std::sync::Arc;

use crate::durable_task::storage::CheckpointKeys;
use af_wire::run::{
    TurnCheckpointAbandon, TurnCheckpointAction, TurnCheckpointAdopt, TurnCheckpointAuthorize,
    TurnCheckpointCommand, TurnCheckpointCommit, TurnCheckpointObjects, TurnCheckpointPageRead,
    TurnCheckpointPageWrite, TurnCheckpointPinRelease, TurnCheckpointPinRenew, TurnCheckpointRead,
    TurnCheckpointResponse, TurnCheckpointUploads,
};

use super::{TurnCheckpointRefusal, TurnRun, commit, objects, pages, pins, restore, transfer};
use crate::{Result, run::Runs};

/// Event-owned recovery seals under each workspace's checkpoint key, API-only saves included.
#[derive(Clone)]
pub struct TurnCheckpoints {
    pub(super) keys: CheckpointKeys,
    pub(super) storage: Option<Arc<crate::durable_task::storage::R2CheckpointStore>>,
}

impl std::fmt::Debug for TurnCheckpoints {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TurnCheckpoints(private)")
    }
}

impl TurnCheckpoints {
    /// Share the deployment's checkpoint keys and cipher with the file backend.
    #[must_use]
    pub const fn new(keys: CheckpointKeys) -> Self {
        Self {
            keys,
            storage: None,
        }
    }

    /// The deployment's checkpoint keys, shared with the file backend.
    #[must_use]
    pub const fn keys(&self) -> &CheckpointKeys {
        &self.keys
    }

    /// Share the existing private file backend; API-only state does not require it.
    #[must_use]
    pub fn with_storage(
        mut self,
        storage: Option<crate::durable_task::storage::R2CheckpointStore>,
    ) -> Self {
        self.storage = storage.map(Arc::new);
        self
    }

    pub(super) fn files(&self) -> Result<&crate::durable_task::storage::R2CheckpointStore> {
        self.storage
            .as_deref()
            .ok_or_else(|| TurnCheckpointRefusal::Unavailable.into())
    }

    /// Perform one native operation under the caller's current physical connection.
    ///
    /// # Errors
    /// Preserves run ownership failures and typed recovery refusals. Private bytes
    /// and encryption material never enter public error details.
    pub async fn execute(
        &self,
        runs: &Runs,
        presented: TurnRun<'_>,
        request: &garde::Valid<TurnCheckpointCommand>,
    ) -> Result<TurnCheckpointResponse> {
        if request.fencing_token != presented.fencing_token {
            return Err(crate::error::stale_fence());
        }
        self.perform(runs, presented, request)
            .await
            .map_err(super::files::public_failure)
    }

    async fn perform(
        &self,
        runs: &Runs,
        presented: TurnRun<'_>,
        request: &garde::Valid<TurnCheckpointCommand>,
    ) -> Result<TurnCheckpointResponse> {
        match &request.action {
            TurnCheckpointAction::Begin(begin) => {
                if matches!(begin.workspace, af_wire::run::TurnWorkspace::Files { .. }) {
                    self.files()?;
                }
                Ok(TurnCheckpointResponse::Begun(Box::new(
                    runs.begin_turn_checkpoint(presented, request, self.keys.cipher())
                        .await?,
                )))
            }
            TurnCheckpointAction::Commit(TurnCheckpointCommit {
                checkpoint,
                root,
                manifest_root,
                folds,
                uploaded,
            }) => {
                let body = commit::Publication {
                    root,
                    manifest: manifest_root,
                    folds,
                    uploaded,
                };
                commit::publish(self, runs, presented, checkpoint, &body).await
            }
            TurnCheckpointAction::Read(TurnCheckpointRead { descriptor }) => {
                restore::read(self, runs, presented, &request.operation_id, descriptor).await
            }
            TurnCheckpointAction::Adopt(TurnCheckpointAdopt { checkpoint }) => {
                restore::adopt(runs, presented, checkpoint).await
            }
            TurnCheckpointAction::PinRenew(TurnCheckpointPinRenew {
                checkpoint,
                resource_version,
            }) => {
                pins::update(
                    runs,
                    presented,
                    checkpoint,
                    pins::Change::Renew(*resource_version),
                )
                .await
            }
            TurnCheckpointAction::PinRelease(TurnCheckpointPinRelease { checkpoint }) => {
                pins::update(runs, presented, checkpoint, pins::Change::Release).await
            }
            TurnCheckpointAction::Abandon(TurnCheckpointAbandon { checkpoint }) => {
                pins::update(runs, presented, checkpoint, pins::Change::Abandon).await
            }
            TurnCheckpointAction::Objects(TurnCheckpointObjects {
                checkpoint,
                objects,
            }) => objects::register(self, runs, presented, checkpoint, objects).await,
            TurnCheckpointAction::Uploads(TurnCheckpointUploads {
                checkpoint,
                objects,
            }) => {
                let transfer = transfer::Transfer::Uploads(objects);
                transfer::execute(self, runs, presented, checkpoint, transfer).await
            }
            TurnCheckpointAction::Authorize(TurnCheckpointAuthorize {
                checkpoint,
                purpose,
                object_ids,
            }) => {
                let transfer = transfer::Transfer::Authorize(*purpose, object_ids);
                transfer::execute(self, runs, presented, checkpoint, transfer).await
            }
            TurnCheckpointAction::PageWrite(TurnCheckpointPageWrite {
                checkpoint,
                page_id,
                digest,
                contents,
            }) => {
                let page = pages::Write {
                    page_id,
                    digest,
                    contents,
                };
                pages::write(runs, presented, checkpoint, &page).await
            }
            TurnCheckpointAction::PageRead(TurnCheckpointPageRead {
                checkpoint,
                page_id,
            }) => pages::read(runs, presented, checkpoint, page_id).await,
        }
    }
}
