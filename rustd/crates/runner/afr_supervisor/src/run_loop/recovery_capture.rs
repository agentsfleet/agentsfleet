//! A boundary's capture runs only when the executor is idle and no tenant
//! process remains, so nothing writes the workspace while it is exported into
//! the stage the checkpoint reads.

use std::panic::AssertUnwindSafe;

use af_core::{error_code, id::Uuid7};
use af_wire::durable_task::{CheckpointMode, DurableTaskTarget};
use afr_agent::{
    CommittedRecovery, DurableTaskRecovery, FileRecoveryPermit, RecoveryReceipt, RecoverySave,
    RecoverySkip, RecoverySnapshot, RecoveryState, TurnSnapshot,
};
use afr_engine::{Quiescence, Sandbox};
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
        mut snapshot: RecoverySnapshot,
    ) -> afr_agent::Result<RecoverySave> {
        if snapshot.run_id != self.run.ids.run.as_str()
            || snapshot.turn_id != self.run.payload.event.turn_id
            || snapshot.fencing_token != self.run.payload.fencing_token
        {
            self.run.interrupt.cancel();
            return Err(afr_agent::Error::checkpoint(error::config(
                "capture belongs to a different run, event or fence",
            )));
        }
        let ready = match self.workspace {
            Workspace::ApiOnly => Ready::ApiOnly,
            Workspace::Building { cell, recovery } => match cell.get() {
                Some(sandbox) => Ready::Sandbox(sandbox.as_ref(), recovery.get()),
                None => return Ok(RecoverySave::Skipped(RecoverySkip::WorkspaceBuilding)),
            },
        };
        let _reservation = snapshot.file_permit.take();
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
        if let Some(sandbox) = sandbox {
            match executor_idle(sandbox).await {
                Ok(true) => {}
                Ok(false) => return Ok(RecoverySave::Skipped(RecoverySkip::BusyExecutor)),
                Err(failure) => {
                    tracing::debug!(event = "run_checkpoint_executor_unproved", error_code = %failure.code());
                    return Ok(RecoverySave::Skipped(RecoverySkip::Unavailable));
                }
            }
        }
        match AssertUnwindSafe(self.publish(snapshot, &ready))
            .catch_unwind()
            .await
        {
            Ok(captured) => captured,
            Err(_panic) => Err(error::config("live capture panicked")),
        }
    }

    async fn publish(
        &mut self,
        snapshot: RecoverySnapshot,
        ready: &Ready<'_>,
    ) -> Result<RecoverySave> {
        let (staged, toolbox, state) = match ready {
            Ready::ApiOnly => {
                if matches!(&snapshot.state, RecoveryState::DurableTask(task)
                    if !matches!(task.task.target, DurableTaskTarget::ApiOnly))
                {
                    return Err(error::config("file capture requires a ready sandbox"));
                }
                (None, api_toolbox(), None)
            }
            Ready::Sandbox(sandbox, state) => {
                let quiescence = sandbox.quiesce().await.map_err(|source| {
                    error::boundary("cannot prove live capture tenant quiescence", source)
                })?;
                if quiescence == Quiescence::Busy {
                    return Ok(RecoverySave::Skipped(RecoverySkip::BusyProcesses));
                }
                let Some(toolbox) = sandbox.environment().root.toolbox() else {
                    return Ok(RecoverySave::Skipped(RecoverySkip::Unavailable));
                };
                let staged = self.run.lessee.staging.stage(sandbox.id())?;
                let disk_bytes = sandbox.granted().limits().disk_bytes;
                if let Err(failure) = staged.export(sandbox.executor(), disk_bytes).await {
                    // A workspace that cannot be read out skips this save, as
                    // one the host could not scan did: the run goes on.
                    let error_code = failure.code();
                    tracing::warn!(event = "run_checkpoint_export_failed", %error_code);
                    return Ok(RecoverySave::Skipped(RecoverySkip::Unavailable));
                }
                (Some(staged), toolbox.to_owned(), *state)
            }
        };
        // The stage is the capture's byte source until it returns, uploads
        // included, and goes with it.
        let root = staged.as_ref().map(|staged| staged.path().to_owned());
        match snapshot.state {
            RecoveryState::DurableTask(task) => {
                self.publish_durable_task(task, root, &toolbox, state).await
            }
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
                run_id: self.run.ids.run.to_string(),
                turn_id: self.run.payload.event.turn_id.to_string(),
                fencing_token: self.run.payload.fencing_token,
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

    async fn publish_durable_task(
        &self,
        task: DurableTaskRecovery,
        root: Option<std::path::PathBuf>,
        toolbox: &str,
        state: Option<&State>,
    ) -> Result<RecoverySave> {
        let scope = Uuid7::parse(&self.run.payload.event.workspace_id)?;
        let digest = af_core::etag::sha256_hex(&task.history);
        let captured = Capture {
            plane: &self.run.lessee.plane,
            transfers: &self.run.lessee.recovery.transfers,
            run: &self.run.ids.run,
            workspace: &scope,
            fence: self.run.payload.fencing_token,
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
                run_id: self.run.ids.run.to_string(),
                turn_id: self.run.payload.event.turn_id.to_string(),
                fencing_token: self.run.payload.fencing_token,
                checkpoint: CommittedRecovery::DurableTask {
                    checkpoint_id: plan.checkpoint_id,
                    generation: plan.generation,
                    history_digest: digest,
                },
            }))),
            Err(failure)
                if matches!(
                    failure.code(),
                    error_code::DURABLE_TASK_LIMIT_EXHAUSTED | error_code::RUN_CHECKPOINT_LIMIT
                ) =>
            {
                Ok(RecoverySave::Skipped(RecoverySkip::Limit))
            }
            // An outage skips this boundary; the next one saves again, and the
            // run's final checkpoint still decides whether the task blocks.
            Err(failure)
                if failure.is_retryable()
                    || matches!(
                        failure.code(),
                        error_code::DURABLE_TASK_STORAGE_UNAVAILABLE
                            | error_code::RUN_CHECKPOINT_UNAVAILABLE
                    ) =>
            {
                // The record is the skip; the outage that caused it is its cause.
                let error_code = error_code::RUN_CHECKPOINT_UNAVAILABLE;
                let cause_error_code = failure.code();
                tracing::warn!(
                    event = "task_checkpoint_skipped",
                    %error_code,
                    %cause_error_code
                );
                Ok(RecoverySave::Skipped(RecoverySkip::Unavailable))
            }
            Err(failure) => Err(failure),
        }
    }
}

impl super::Run<'_> {
    pub(super) async fn reserve_terminal_capture(
        &self,
        sandbox: &dyn Sandbox,
        recoverable: bool,
    ) -> Result<Option<FileRecoveryPermit>> {
        if !recoverable {
            return Ok(None);
        }
        match executor_idle(sandbox).await {
            Ok(true) => self.lessee.recovery.file(&self.interrupt).await.map(Some),
            Ok(false) => Err(error::capture_unavailable(
                "terminal capture has active file work",
            )),
            Err(failure) => {
                let error_code = failure.code();
                tracing::warn!(event = "run_checkpoint_executor_unproved", %error_code);
                Err(failure)
            }
        }
    }
}

/// The executor's own file jobs must be idle before a capture reads the workspace.
async fn executor_idle(sandbox: &dyn Sandbox) -> Result<bool> {
    sandbox
        .executor()
        .recovery_idle()
        .await
        .map_err(|source| error::boundary("cannot prove executor file jobs idle", source))
}
