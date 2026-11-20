//! Ordinary private state binds to its original event and the matching workspace.

use af_wire::run::{
    CheckpointOperationId, FoldRef, RunPayload, TURN_CHECKPOINT_BYTES, TURN_CHECKPOINT_FORMAT,
    TurnResume, TurnScope, TurnWorkspace,
};
use garde::Validate;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::RootRecovery;
use crate::{
    Result,
    error::{recovery_invalid, recovery_source},
};

/// Private state awaiting event-owned publication by the supervisor.
pub struct TurnSnapshot {
    /// Derived from the issued run and the supervisor's actual workspace.
    pub scope: TurnScope,
    /// Last acknowledged descriptor; publication compares this exact head.
    pub previous: Option<TurnResume>,
    /// Exact incorporated turns, checked against the private root at commit.
    pub folds: Vec<FoldRef>,
    /// Bounded versioned root; no model credential or prior usage meter.
    pub history: Zeroizing<Vec<u8>>,
}

/// A fenced native read has authenticated these bytes with its exact descriptor.
#[derive(Clone)]
pub struct TurnRecovery {
    /// Exact assignment generation, including the files and root digests.
    pub descriptor: TurnResume,
    /// Complete root bytes returned by the fenced reader.
    pub history: Zeroizing<Vec<u8>>,
    /// Exact membership adopted under the replacement run before model startup.
    pub adopted: Vec<FoldRef>,
    /// Actual workspace proof supplied after restore, independently compared with the assignment.
    pub workspace: TurnWorkspace,
}

impl std::fmt::Debug for TurnSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TurnSnapshot")
            .field("scope", &self.scope)
            .field("history_bytes", &self.history.len())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for TurnRecovery {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TurnRecovery")
            .field("descriptor", &self.descriptor)
            .field("history_bytes", &self.history.len())
            .finish_non_exhaustive()
    }
}

#[derive(Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
struct History {
    #[garde(range(min = TURN_CHECKPOINT_FORMAT, max = TURN_CHECKPOINT_FORMAT))]
    format: u32,
    #[garde(dive)]
    scope: TurnScope,
    #[garde(length(bytes, min = 1, max = 1024))]
    model: String,
    #[garde(length(bytes, min = 1, max = 128))]
    provider: String,
    #[garde(skip)]
    root: RootRecovery,
}

#[derive(Validate)]
struct Encoded<'a> {
    #[garde(length(max = TURN_CHECKPOINT_BYTES))]
    bytes: &'a [u8],
}

impl TurnSnapshot {
    pub(crate) fn capture(
        run: &RunPayload<'_>,
        workspace: TurnWorkspace,
        previous: Option<TurnResume>,
        root: RootRecovery,
    ) -> Result<Self> {
        root.check()?;
        let scope = scope(run, workspace)?;
        if let Some(previous) = &previous {
            previous.validate()?;
            if previous.scope != scope {
                return Err(recovery_invalid(
                    "previous checkpoint belongs to a different setup or workspace",
                ));
            }
        }
        let folds = root.folds.clone();
        let history = History {
            format: TURN_CHECKPOINT_FORMAT,
            scope: scope.clone(),
            model: run.policy.context.model.to_string(),
            provider: run.policy.provider.to_string(),
            root,
        };
        history.validate()?;
        if !af_validate::serialized_within(&history, TURN_CHECKPOINT_BYTES) {
            return Err(recovery_invalid(
                "ordinary private history exceeds its byte limit",
            ));
        }
        Ok(Self {
            scope,
            previous,
            folds,
            history: Zeroizing::new(serde_json::to_vec(&history)?),
        })
    }
}

impl TurnRecovery {
    /// Check the complete private codec and assigned scope before adopting inputs.
    ///
    /// # Errors
    /// Refuses unsupported formats, corrupt root state, changed membership and scope.
    pub fn validate_for(&self, run: &RunPayload<'_>) -> Result<()> {
        let scrub = af_secrets::Scrub::new(&run.policy)?;
        self.restore(run, self.workspace.clone(), &scrub).map(drop)
    }

    pub(crate) fn restore(
        &self,
        run: &RunPayload<'_>,
        workspace: TurnWorkspace,
        scrub: &af_secrets::Scrub,
    ) -> Result<RootRecovery> {
        self.descriptor.validate()?;
        Encoded {
            bytes: &self.history,
        }
        .validate()?;
        let scope = scope(run, workspace)?;
        if run.turn_resume.as_ref() != Some(&self.descriptor)
            || self.descriptor.scope != scope
            || af_core::etag::sha256_hex(&self.history) != self.descriptor.history_digest
        {
            return Err(recovery_invalid(
                "ordinary restore differs from the assigned scope or digest",
            ));
        }
        let mut history: History = serde_json::from_slice(&self.history)
            .map_err(|source| recovery_source("ordinary private history", source))?;
        history.validate()?;
        history.root.check()?;
        if history.scope != scope || history.root.folds != self.adopted {
            return Err(recovery_invalid(
                "ordinary restore has a different setup or adopted input membership",
            ));
        }
        history.root.scrub(
            scrub,
            history.model != run.policy.context.model || history.provider != run.policy.provider,
        )?;
        Ok(history.root)
    }
}

fn scope(run: &RunPayload<'_>, workspace: TurnWorkspace) -> Result<TurnScope> {
    if run.durable_tasks || run.task.is_some() {
        return Err(recovery_invalid(
            "ordinary recovery cannot impersonate native task state",
        ));
    }
    let revision = run
        .setup_revision_id
        .as_ref()
        .ok_or_else(|| recovery_invalid("ordinary recovery has no issued setup witness"))?;
    let scope = TurnScope {
        workspace_id: CheckpointOperationId(run.event.workspace_id.to_string()),
        fleet_id: CheckpointOperationId(run.event.fleet_id.to_string()),
        turn_id: CheckpointOperationId(run.event.turn_id.to_string()),
        setup_revision_id: CheckpointOperationId(revision.to_string()),
        workspace,
    };
    scope.validate()?;
    Ok(scope)
}
