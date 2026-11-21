//! Authenticate full root and matching files before adopting inputs or starting the model.

use super::{NativeTurn, session::TurnSession};
use crate::{Result, error};
use af_wire::run::{
    CheckpointOperationId, RunPayload, TurnCheckpointAction as Action, TurnCheckpointRead,
    TurnCheckpointRef, TurnCheckpointResponse as Response, TurnWorkspace,
};
use afr_agent::TurnRecovery;
use garde::Validate as _;
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

impl NativeTurn<'_> {
    pub(crate) async fn restore(
        &self,
        assignment: &RunPayload<'_>,
        workspace: TurnWorkspace,
        destination: Option<(PathBuf, (u32, u32))>,
        stop: &CancellationToken,
        permit: Option<&afr_agent::FileRecoveryPermit>,
    ) -> Result<Option<TurnRecovery>> {
        let Some(descriptor) = &assignment.turn_resume else {
            return Ok(None);
        };
        if descriptor.scope.workspace != workspace
            || matches!(workspace, TurnWorkspace::Files { .. }) != destination.is_some()
        {
            return Err(error::config(
                "ordinary restore has no matching actual workspace",
            ));
        }
        let Response::Read(saved) = self
            .command(Action::Read {
                descriptor: Box::new(descriptor.clone()),
            })
            .await?
        else {
            return Err(error::config(
                "turn read returned another command's response",
            ));
        };
        let reference = TurnCheckpointRef {
            checkpoint_id: saved.descriptor.checkpoint_id.clone(),
            pin_id: CheckpointOperationId(saved.pin.id.clone()),
        };
        let checked = async {
            validate(&saved, assignment, &workspace)?;
            if let Some(destination) = destination {
                let mut session = TurnSession::new(
                    self,
                    &saved.descriptor.scope,
                    reference.clone(),
                    saved.pin.clone(),
                    stop,
                )?;
                session
                    .restore_files(
                        &saved.page_ids,
                        &saved.descriptor.manifest_root,
                        destination,
                        permit,
                    )
                    .await?;
            } else {
                require_empty_manifest(&saved)?;
            }
            if stop.is_cancelled() {
                return Err(error::config("ordinary restore ended before adoption"));
            }
            Ok::<_, crate::Error>(())
        }
        .await;
        if let Err(failure) = checked {
            let _cleanup = self
                .command(Action::PinRelease {
                    checkpoint: reference,
                })
                .await;
            return Err(failure);
        }
        self.adopt(assignment, *saved, workspace).await.map(Some)
    }
}

fn require_empty_manifest(saved: &TurnCheckpointRead) -> Result<()> {
    let empty = af_crypto::checkpoint_digest::manifest_root(std::iter::empty::<(&str, &str)>());
    if !saved.page_ids.is_empty() || saved.descriptor.manifest_root != empty {
        return Err(error::config(
            "API-only recovery unexpectedly contains files",
        ));
    }
    Ok(())
}

fn validate(
    saved: &TurnCheckpointRead,
    assignment: &RunPayload<'_>,
    workspace: &TurnWorkspace,
) -> Result<()> {
    saved.validate()?;
    let proof = TurnRecovery {
        descriptor: saved.descriptor.clone(),
        history: Zeroizing::new(saved.root.history.as_bytes().to_vec()),
        adopted: saved.folds.clone(),
        workspace: workspace.clone(),
    };
    proof
        .validate_for(assignment)
        .map_err(|source| error::boundary("ordinary root validation refused restore", source))
}
