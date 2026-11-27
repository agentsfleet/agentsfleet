//! A boundary's save in two steps. Under the pause, the executor and tenant
//! are proved idle and the workspace is pinned; the run then resumes, and the
//! publication registers, seals, uploads, writes pages and commits after it.

use std::panic::AssertUnwindSafe;

use af_core::{error_code, id::Uuid7};
use af_wire::durable_task::{CheckpointMode, DurableTaskTarget};
use afr_agent::{
    CommittedRecovery, DurableTaskRecovery, FileRecoveryPermit, RecoveryReceipt, RecoverySave,
    RecoverySkip, RecoverySnapshot, RecoveryState, TurnSnapshot,
};
use afr_engine::{Resumed, Sandbox};
use futures_util::FutureExt as _;

use super::{
    pin::{Pinned as PinSource, Pinning, executor_idle, pin_sandbox, release},
    recovery::{State, api_toolbox},
    recovery_boundary::{BoundaryOwner, Workspace},
};
use crate::{
    Result, error,
    recovery::{Capture, ChangeSource},
    staging::{ExportPin, ExportVolume},
};

/// How a boundary's pause ended.
pub(super) enum Pinned<'a> {
    /// The save is answered already: it skipped, or failed.
    Answered(afr_agent::Result<RecoverySave>),
    /// The run may resume; this publishes after it.
    Publish(Box<Publish<'a>>),
}

/// A save whose run resumed, waiting to publish.
pub(super) struct Publish<'a> {
    /// The runner's file capacity, held until the publication ends.
    _reservation: Option<FileRecoveryPermit>,
    state: RecoveryState,
    files: Option<(ExportVolume<'a>, Resumed<ExportPin>)>,
    toolbox: String,
    recovery: Option<&'a State>,
}

impl<'a> BoundaryOwner<'a, '_> {
    /// Everything a save does under the pause; no daemon call runs here.
    pub(super) async fn pin(&self, snapshot: RecoverySnapshot) -> Pinned<'a> {
        if snapshot.run_id != self.run.ids.run.as_str()
            || snapshot.turn_id != self.run.payload.event.turn_id
            || snapshot.fencing_token != self.run.payload.fencing_token
        {
            self.run.interrupt.cancel();
            return Pinned::Answered(Err(afr_agent::Error::checkpoint(error::config(
                "capture belongs to a different run, event or fence",
            ))));
        }
        match AssertUnwindSafe(Box::pin(self.pin_ready(snapshot)))
            .catch_unwind()
            .await
        {
            Ok(Ok(pinned)) => pinned,
            Ok(Err(failure)) => Pinned::Answered(Err(self.failed(failure))),
            Err(_panic) => {
                Pinned::Answered(Err(self.failed(error::config("live capture panicked"))))
            }
        }
    }

    async fn pin_ready(&self, mut snapshot: RecoverySnapshot) -> Result<Pinned<'a>> {
        let reservation = snapshot.file_permit.take();
        let sandbox: &'a dyn Sandbox = match self.workspace {
            Workspace::ApiOnly => {
                if matches!(&snapshot.state, RecoveryState::DurableTask(task)
                    if !matches!(task.task.target, DurableTaskTarget::ApiOnly))
                {
                    return Err(error::config("file capture requires a ready sandbox"));
                }
                return Ok(Pinned::Publish(Box::new(Publish {
                    _reservation: reservation,
                    state: snapshot.state,
                    files: None,
                    toolbox: api_toolbox(),
                    recovery: None,
                })));
            }
            Workspace::Building { cell, .. } => match cell.get() {
                Some(sandbox) => sandbox.as_ref(),
                None => return Ok(skipped(RecoverySkip::WorkspaceBuilding)),
            },
        };
        let Workspace::Building { recovery, .. } = self.workspace else {
            return Ok(skipped(RecoverySkip::Unavailable));
        };
        let volume = ExportVolume::new(sandbox, &self.run.lessee.staging, self.stop.clone());
        let parent = self.parent(&snapshot.state)?;
        let pinning = pin_sandbox(sandbox, &volume, parent.as_ref()).await?;
        let pin = match pinning {
            Pinning::Skipped(reason) => return Ok(skipped(reason)),
            Pinning::Pinned(pin) => pin,
        };
        let toolbox = sandbox.environment().root.toolbox().unwrap_or_default();
        Ok(Pinned::Publish(Box::new(Publish {
            _reservation: reservation,
            state: snapshot.state,
            toolbox: toolbox.to_owned(),
            files: Some((volume, pin)),
            recovery: recovery.get(),
        })))
    }

    /// The generation the owner's last save committed: what the pin reads
    /// against.
    fn parent(&self, state: &RecoveryState) -> Result<Option<Uuid7>> {
        let committed = match state {
            RecoveryState::DurableTask(task) => task.task.checkpoint_id.as_deref(),
            RecoveryState::Turn(_) => self.turn.head().map(|head| head.checkpoint_id.0.as_str()),
        };
        Ok(committed.map(Uuid7::parse).transpose()?)
    }

    /// The whole save at once, pin then publication, as a test drives it.
    #[cfg(test)]
    pub(super) async fn capture(
        &mut self,
        snapshot: RecoverySnapshot,
    ) -> afr_agent::Result<RecoverySave> {
        match self.pin(snapshot).await {
            Pinned::Answered(saved) => saved,
            Pinned::Publish(publish) => self.publish(publish).await,
        }
    }

    /// Everything a save does after the run resumed.
    pub(super) async fn publish(
        &mut self,
        publish: Box<Publish<'a>>,
    ) -> afr_agent::Result<RecoverySave> {
        let published = Box::pin(self.publish_files(publish));
        let saved = match AssertUnwindSafe(published).catch_unwind().await {
            Ok(saved) => saved,
            Err(_panic) => Err(error::config("checkpoint publication panicked")),
        };
        saved.map_err(|failure| self.failed(failure))
    }

    async fn publish_files(&mut self, publish: Box<Publish<'a>>) -> Result<RecoverySave> {
        let Publish {
            _reservation,
            state,
            files,
            toolbox,
            recovery,
        } = *publish;
        let Some((volume, pin)) = files else {
            return self.publish_state(state, None, &toolbox, recovery).await;
        };
        let source = PinSource {
            volume: &volume,
            pin: &pin,
        };
        let saved = self
            .publish_state(state, Some(&source), &toolbox, recovery)
            .await;
        release(&volume, pin, saved.is_ok()).await;
        saved
    }

    async fn publish_state(
        &mut self,
        state: RecoveryState,
        source: Option<&dyn ChangeSource>,
        toolbox: &str,
        recovery: Option<&State>,
    ) -> Result<RecoverySave> {
        match state {
            RecoveryState::DurableTask(task) => {
                self.publish_durable_task(task, source, toolbox, recovery)
                    .await
            }
            RecoveryState::Turn(turn) => self.publish_turn(turn, source).await,
        }
    }

    /// Stops the run on a failure no skip covers, and says so.
    fn failed(&self, failure: crate::Error) -> afr_agent::Error {
        self.run.interrupt.cancel();
        tracing::warn!(event = "run_checkpoint_owner_failed", error_code = %failure.code());
        afr_agent::Error::checkpoint(failure)
    }

    async fn publish_turn(
        &mut self,
        turn: TurnSnapshot,
        source: Option<&dyn ChangeSource>,
    ) -> Result<RecoverySave> {
        let result = self.turn.capture(turn, source, &self.stop).await;
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
        source: Option<&dyn ChangeSource>,
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
            source,
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
            Err(failure) => skip_durable_task(failure),
        }
    }
}

/// A task save that failed: a limit or an outage skips this boundary, and the
/// next one saves again; the run's final checkpoint still decides whether the
/// task blocks. Anything else stops the run.
fn skip_durable_task(failure: crate::Error) -> Result<RecoverySave> {
    if matches!(
        failure.code(),
        error_code::DURABLE_TASK_LIMIT_EXHAUSTED | error_code::RUN_CHECKPOINT_LIMIT
    ) {
        return Ok(RecoverySave::Skipped(RecoverySkip::Limit));
    }
    if failure.is_retryable()
        || matches!(
            failure.code(),
            error_code::DURABLE_TASK_STORAGE_UNAVAILABLE | error_code::RUN_CHECKPOINT_UNAVAILABLE
        )
    {
        // The record is the skip; the outage that caused it is its cause.
        let error_code = error_code::RUN_CHECKPOINT_UNAVAILABLE;
        let cause_error_code = failure.code();
        tracing::warn!(
            event = "task_checkpoint_skipped",
            %error_code,
            %cause_error_code
        );
        return Ok(RecoverySave::Skipped(RecoverySkip::Unavailable));
    }
    Err(failure)
}

const fn skipped(reason: RecoverySkip) -> Pinned<'static> {
    Pinned::Answered(Ok(RecoverySave::Skipped(reason)))
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
