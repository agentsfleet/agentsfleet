//! A committed receipt leaves the owner only after the sandbox has thawed.

use std::panic::AssertUnwindSafe;

use af_core::{error_code, id::Uuid7};
use af_wire::task::{CheckpointMode, TaskTarget};
use afr_agent::{
    CommittedRecovery, RecoveryReceipt, RecoverySave, RecoverySkip, RecoverySnapshot,
    RecoveryState, TaskRecovery, TurnSnapshot,
};
use afr_sandbox::Sandbox;
use futures_util::FutureExt as _;

use super::{
    recovery::{State, api_toolbox},
    recovery_boundary::{BoundaryOwner, Workspace},
};
use crate::{Result, error, recovery::Capture};

enum Ready<'a> {
    ApiOnly,
    Sandbox(&'a dyn Sandbox, Option<&'a State>),
}

impl BoundaryOwner<'_, '_> {
    pub(super) async fn capture(
        &mut self,
        snapshot: RecoverySnapshot,
    ) -> afr_agent::Result<RecoverySave> {
        if snapshot.lease_id != self.run.ids.lease.as_str()
            || snapshot.event_id != self.run.lease.event.event_id
            || snapshot.fencing_token != self.run.lease.fencing_token
        {
            self.run.interrupt.cancel();
            return Err(afr_agent::Error::checkpoint(error::config(
                "capture belongs to a different lease, event or fence",
            )));
        }
        let ready = match self.workspace {
            Workspace::ApiOnly => Ready::ApiOnly,
            Workspace::Building { cell, recovery } => match cell.get() {
                Some(sandbox) => Ready::Sandbox(sandbox.as_ref(), recovery.get()),
                None => return Ok(RecoverySave::Skipped(RecoverySkip::WorkspaceBuilding)),
            },
        };
        let result = self.capture_ready(snapshot, ready).await;
        if let Err(failure) = &result {
            self.run.interrupt.cancel();
            tracing::warn!(event = "run_checkpoint_owner_failed", error_code = %failure.code());
        }
        result.map_err(afr_agent::Error::checkpoint)
    }

    async fn capture_ready(
        &mut self,
        snapshot: RecoverySnapshot,
        ready: Ready<'_>,
    ) -> Result<RecoverySave> {
        let sandbox = match ready {
            Ready::ApiOnly => None,
            Ready::Sandbox(sandbox, _) => Some(sandbox),
        };
        let mut attempted_freeze = false;
        let capturing = async {
            if let Some(sandbox) = sandbox {
                match executor_idle(sandbox).await {
                    Ok(true) => {}
                    Ok(false) => return Ok(RecoverySave::Skipped(RecoverySkip::BusyExecutor)),
                    Err(failure) => {
                        tracing::debug!(event = "run_checkpoint_executor_unproved", error_code = %failure.code());
                        return Ok(RecoverySave::Skipped(RecoverySkip::Unavailable));
                    }
                }
                attempted_freeze = true;
                if let Err(failure) = sandbox.freeze().await {
                    tracing::warn!(event = "run_checkpoint_freeze_failed", error_code = %failure.code());
                    return Ok(RecoverySave::Skipped(RecoverySkip::Unavailable));
                }
            }
            self.publish(snapshot, &ready).await
        };
        let captured = match AssertUnwindSafe(capturing).catch_unwind().await {
            Ok(captured) => captured,
            Err(_panic) => Err(error::config("live capture panicked before cleanup")),
        };
        // Even a refused freeze can have changed kernel state. Neither a lost
        // caller nor a refused publication may bypass the cleanup attempt.
        if let Some(sandbox) = sandbox.filter(|_sandbox| attempted_freeze) {
            AssertUnwindSafe(sandbox.thaw())
                .catch_unwind()
                .await
                .map_err(|_panic| error::config("sandbox thaw panicked after live capture"))?
                .map_err(|source| {
                    error::boundary("sandbox could not thaw after live capture", source)
                })?;
        }
        captured
    }

    async fn publish(
        &mut self,
        snapshot: RecoverySnapshot,
        ready: &Ready<'_>,
    ) -> Result<RecoverySave> {
        let (root, toolbox, state) = match ready {
            Ready::ApiOnly => {
                if matches!(&snapshot.state, RecoveryState::Task(task)
                    if !matches!(task.task.target, TaskTarget::ApiOnly))
                {
                    return Err(error::config("file capture requires a ready sandbox"));
                }
                (None, api_toolbox(), None)
            }
            Ready::Sandbox(sandbox, state) => {
                if !sandbox.recovery_quiescent().await.map_err(|source| {
                    error::boundary("cannot prove live capture tenant quiescence", source)
                })? {
                    return Ok(RecoverySave::Skipped(RecoverySkip::BusyProcesses));
                }
                let Some(workspace) = sandbox.workspace() else {
                    return Ok(RecoverySave::Skipped(RecoverySkip::Unavailable));
                };
                let Some(toolbox) = sandbox.toolbox_digest() else {
                    return Ok(RecoverySave::Skipped(RecoverySkip::Unavailable));
                };
                (Some(workspace.root.to_owned()), toolbox.to_owned(), *state)
            }
        };
        match snapshot.state {
            RecoveryState::Task(task) => self.publish_task(task, root, &toolbox, state).await,
            RecoveryState::Turn(turn) => self.publish_turn(turn, root).await,
        }
    }

    async fn publish_turn(
        &mut self,
        turn: TurnSnapshot,
        root: Option<std::path::PathBuf>,
    ) -> Result<RecoverySave> {
        let result = self.turn.capture(turn, root, &self.stop).await;
        match result {
            Ok(descriptor) => Ok(RecoverySave::Committed(Box::new(RecoveryReceipt {
                lease_id: self.run.ids.lease.to_string(),
                event_id: self.run.lease.event.event_id.to_string(),
                fencing_token: self.run.lease.fencing_token,
                checkpoint: CommittedRecovery::Turn(descriptor),
            }))),
            Err(failure) if failure.code() == error_code::RUN_CHECKPOINT_LIMIT => {
                Ok(RecoverySave::Skipped(RecoverySkip::Limit))
            }
            Err(failure)
                if failure.is_retryable()
                    || matches!(
                        failure.code(),
                        error_code::RUN_CHECKPOINT_UNAVAILABLE
                            | error_code::RUN_CHECKPOINT_CONFLICT
                    ) =>
            {
                Ok(RecoverySave::Skipped(RecoverySkip::Unavailable))
            }
            Err(failure) => Err(failure),
        }
    }

    async fn publish_task(
        &self,
        task: TaskRecovery,
        root: Option<std::path::PathBuf>,
        toolbox: &str,
        state: Option<&State>,
    ) -> Result<RecoverySave> {
        let scope = Uuid7::parse(&self.run.lease.event.workspace_id)?;
        let digest = af_core::etag::sha256_hex(&task.history);
        let captured = Capture {
            plane: &self.run.lessee.plane,
            lease: &self.run.ids.lease,
            workspace: &scope,
            fence: self.run.lease.fencing_token,
            task: &task.task,
            toolbox,
            mode: CheckpointMode::Incremental,
            base: state.and_then(|state| state.base.as_ref()),
            stop: &self.stop,
        }
        .run(
            root,
            task.history,
            state.map_or_else(Vec::new, |state| state.base_paths.clone()),
        )
        .await;
        match captured {
            Ok(plan) => Ok(RecoverySave::Committed(Box::new(RecoveryReceipt {
                lease_id: self.run.ids.lease.to_string(),
                event_id: self.run.lease.event.event_id.to_string(),
                fencing_token: self.run.lease.fencing_token,
                checkpoint: CommittedRecovery::Task {
                    checkpoint_id: plan.checkpoint_id,
                    generation: plan.generation,
                    history_digest: digest,
                },
            }))),
            Err(failure)
                if matches!(
                    failure.code(),
                    error_code::TASK_LIMIT_EXHAUSTED | error_code::RUN_CHECKPOINT_LIMIT
                ) =>
            {
                Ok(RecoverySave::Skipped(RecoverySkip::Limit))
            }
            Err(failure)
                if matches!(
                    failure.code(),
                    error_code::TASK_STORAGE_UNAVAILABLE | error_code::RUN_CHECKPOINT_UNAVAILABLE
                ) =>
            {
                Ok(RecoverySave::Skipped(RecoverySkip::Unavailable))
            }
            Err(failure) => Err(failure),
        }
    }
}

impl super::LeaseRun<'_> {
    pub(super) async fn capture_idle(&self, sandbox: &dyn Sandbox) -> bool {
        match executor_idle(sandbox).await {
            Ok(idle) => idle,
            Err(failure) => {
                tracing::warn!(event = "run_checkpoint_executor_unproved", error_code = %failure.code());
                false
            }
        }
    }
}

/// The executor must answer before freeze because its resident threads freeze too.
async fn executor_idle(sandbox: &dyn Sandbox) -> Result<bool> {
    sandbox
        .executor()
        .recovery_idle()
        .await
        .map_err(|source| error::boundary("cannot prove executor file jobs idle", source))
}
