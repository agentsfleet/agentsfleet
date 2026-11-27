//! Authenticate full root and land matching files through the sandbox's
//! engine before adopting inputs or starting the model.

use super::{NativeTurn, reading::TurnReading, session::TurnSession};
use crate::recovery::source::{Landing, land};
use crate::recovery::taken::environment;
use crate::{Result, error};
use af_core::id::Uuid7;
use af_wire::run::{
    CheckpointOperationId, RunPayload, TurnCheckpointPinRelease, TurnCheckpointRead,
    TurnCheckpointRef, TurnCheckpointSaved, TurnWorkspace,
};
use afr_agent::TurnRecovery;
use garde::Validate as _;
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

impl NativeTurn<'_> {
    pub(crate) async fn restore(
        &self,
        assignment: &RunPayload<'_>,
        workspace: TurnWorkspace,
        landing: Option<Landing<'_>>,
        stop: &CancellationToken,
    ) -> Result<Option<TurnRecovery>> {
        let Some(descriptor) = &assignment.turn_resume else {
            return Ok(None);
        };
        if !same_toolbox(&descriptor.scope.workspace, &workspace)
            || matches!(workspace, TurnWorkspace::Files { .. }) != landing.is_some()
        {
            return Err(error::config(
                "ordinary restore has no matching actual workspace",
            ));
        }
        let saved = self
            .command(TurnCheckpointRead {
                descriptor: Box::new(descriptor.clone()),
            })
            .await?;
        let reference = TurnCheckpointRef {
            checkpoint_id: saved.descriptor.checkpoint_id.clone(),
            pin_id: CheckpointOperationId(saved.pin.id.clone()),
        };
        // The recorded workspace names the sandbox the files were taken in:
        // the engine, not this check, refuses landing them in any other.
        let workspace = saved.descriptor.scope.workspace.clone();
        let checked = async {
            validate(&saved, assignment, &workspace)?;
            if let (
                Some(landing),
                TurnWorkspace::Files {
                    toolbox_digest,
                    sandbox,
                },
            ) = (landing, &workspace)
            {
                let mut session = TurnSession::new(
                    self,
                    &saved.descriptor.scope,
                    reference.clone(),
                    saved.pin.clone(),
                    stop,
                )?;
                let reading = TurnReading {
                    session: &mut session,
                    chain: &saved.chain,
                    pages: &saved.page_ids,
                    digest: &saved.descriptor.manifest_root,
                };
                let checkpoint = Uuid7::parse(&saved.descriptor.checkpoint_id.0)?;
                let taken = environment(toolbox_digest, sandbox);
                let generation = saved.descriptor.generation;
                land(reading, (checkpoint, generation), taken, landing).await?;
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
                .command(TurnCheckpointPinRelease {
                    checkpoint: reference,
                })
                .await;
            return Err(failure);
        }
        self.adopt(assignment, *saved, workspace).await.map(Some)
    }
}

/// Whether `recorded` and `actual` both hold files on one toolbox, or both
/// none: the sandbox each names is the engine's to compare.
fn same_toolbox(recorded: &TurnWorkspace, actual: &TurnWorkspace) -> bool {
    match (recorded, actual) {
        (TurnWorkspace::ApiOnly, TurnWorkspace::ApiOnly) => true,
        (
            TurnWorkspace::Files { toolbox_digest, .. },
            TurnWorkspace::Files {
                toolbox_digest: actual,
                ..
            },
        ) => toolbox_digest == actual,
        (TurnWorkspace::ApiOnly, TurnWorkspace::Files { .. })
        | (TurnWorkspace::Files { .. }, TurnWorkspace::ApiOnly) => false,
    }
}

fn require_empty_manifest(saved: &TurnCheckpointSaved) -> Result<()> {
    let empty = af_crypto::checkpoint_digest::manifest_root(std::iter::empty::<(&str, &str)>());
    if !saved.page_ids.is_empty() || saved.descriptor.manifest_root != empty {
        return Err(error::config(
            "API-only recovery unexpectedly contains files",
        ));
    }
    Ok(())
}

fn validate(
    saved: &TurnCheckpointSaved,
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
