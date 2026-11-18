//! Restore and capture keep the private root, memory and child book together.

use af_providers::Message;
use af_secrets::Scrub;
use af_wire::task::TaskContext;
use afr_memory::Hydrated;

use super::Ending;
use super::Harness;
use crate::engine::{AgentRun, RecoveryWitness, RootRecovery};
use crate::error::recovery_invalid;
use crate::nested::CapturePermit;
use crate::{
    CommittedRecovery, RecoverySave, RecoverySkip, RecoverySnapshot, RecoveryState, Result,
    TaskRecovery, TurnSnapshot,
};
use guffbolt::DriveState;

pub(super) struct RootSetup<'run> {
    pub(super) opening: Vec<Message>,
    pub(super) state: Option<RootRecovery>,
    pub(super) memory: Hydrated<'run>,
}

pub(super) fn prepare_root<'run>(
    run: &AgentRun<'run>,
    scrub: &Scrub,
    mut opening: Vec<Message>,
    message: &str,
) -> Result<RootSetup<'run>> {
    if (run.recovered_task.is_some() && run.recovered_turn.is_some())
        || (run.lease.turn_resume.is_some() && run.recovered_turn.is_none())
    {
        return Err(recovery_invalid(
            "offered recovery has no single complete restore owner",
        ));
    }
    let mut state = if let Some(recovered) = run.recovered_task {
        let current = run
            .prepared_task
            .and_then(|prepared| prepared.inputs.current_task.as_ref())
            .ok_or_else(|| recovery_invalid("recovered history has no current native task"))?;
        Some(TaskRecovery::restore(
            &recovered.history,
            current,
            &run.lease.policy.context.model,
            &run.lease.policy.provider,
            scrub,
        )?)
    } else if let Some(recovered) = run.recovered_turn {
        Some(recovered.restore(run.lease, recovered.workspace.clone(), scrub)?)
    } else {
        None
    };
    let memory = if let Some(saved) = &mut state {
        opening = std::mem::take(&mut saved.messages);
        opening.push(Message::User("Resume the accepted objective from this authenticated history. Reconcile native inputs and confirmed effects before taking another action.".to_owned()));
        Hydrated::recover(run.memory, std::mem::take(&mut saved.memory), scrub)?
    } else {
        opening.push(Message::User(scrub.text(message).into_owned()));
        Hydrated::new(run.memory)
    };
    Ok(RootSetup {
        opening,
        state,
        memory,
    })
}

impl Harness<'_, '_> {
    pub(super) async fn capture_task_root(
        &mut self,
        task: TaskContext<'static>,
        permit: Option<&CapturePermit>,
    ) -> Result<TaskRecovery> {
        let permit = permit
            .ok_or_else(|| recovery_invalid("child cleanup did not grant a terminal boundary"))?;
        let root = self.root_snapshot(&self.driver, permit).await?;
        TaskRecovery::capture(
            task,
            self.shared.model,
            &self.shared.payload.policy.provider,
            root,
        )
    }

    async fn root_snapshot(
        &self,
        driver: &DriveState,
        permit: &CapturePermit,
    ) -> Result<RootRecovery> {
        Ok(RootRecovery {
            messages: self.messages.clone(),
            driver: driver.clone(),
            children: self.shared.registry.snapshot(permit)?,
            memory: self.shared.lease.memory.lock().await.snapshot()?,
            pinned: self.context.pinned,
            folds: self.folded_inputs.clone(),
        })
    }

    pub(super) async fn save_boundary(&mut self, driver: &DriveState) -> Option<Ending> {
        if self.depth != 0 || driver.completed_turns() == 0 {
            return None;
        }
        let sink = self.shared.recovery?;
        let workspace = if self.task.is_none() {
            match sink.workspace() {
                Ok(workspace) => Some(workspace),
                Err(reason) => {
                    skipped(reason);
                    return None;
                }
            }
        } else {
            None
        };
        let Some(permit) = self.shared.registry.capture_permit().await else {
            skipped(RecoverySkip::BusyChildren);
            return None;
        };
        if !self.shared.lease.sessions.is_quiescent() {
            skipped(RecoverySkip::BusySessions);
            return None;
        }
        let snapshot = match self.snapshot_state(driver, &permit, workspace).await {
            Ok(state) => RecoverySnapshot {
                lease_id: self.shared.lease_id.to_owned(),
                event_id: self.shared.payload.event.event_id.to_string(),
                fencing_token: self.shared.fencing_token,
                state,
            },
            Err(failure) => {
                tracing::debug!(event = "run_checkpoint_skipped", reason = ?RecoverySkip::Limit, error_code = %failure.code());
                return None;
            }
        };
        let expected = snapshot.witness();
        self.accept_snapshot(sink.save(snapshot).await, &expected)
    }

    fn accept_snapshot(
        &mut self,
        saved: Result<RecoverySave>,
        expected: &RecoveryWitness,
    ) -> Option<Ending> {
        match saved {
            Ok(RecoverySave::Skipped(reason)) => skipped(reason),
            Ok(RecoverySave::Committed(receipt)) if receipt.matches(expected) => {
                committed(&receipt);
                if let Some(current) = self
                    .task
                    .as_mut()
                    .and_then(|task| task.inputs.current_task.as_mut())
                {
                    current.checkpoint_id = Some(receipt.checkpoint.id().to_owned().into());
                    current.checkpoint_generation = Some(receipt.checkpoint.generation());
                }
                if let CommittedRecovery::Turn(descriptor) = receipt.checkpoint {
                    self.turn_checkpoint = Some(descriptor);
                }
            }
            Ok(RecoverySave::Committed(_)) => {
                self.task_report = None;
                tracing::warn!(event = "run_checkpoint_receipt_mismatch", error_code = %af_core::error_code::RUN_CHECKPOINT_INCOMPATIBLE);
                return Some(Ending::Stopped);
            }
            Err(failure) => {
                self.task_report = None;
                tracing::warn!(event = "run_checkpoint_cleanup_failed", error_code = %failure.code());
                return Some(Ending::Stopped);
            }
        }
        None
    }

    async fn snapshot_state(
        &self,
        driver: &DriveState,
        permit: &CapturePermit,
        workspace: Option<af_wire::lease::TurnWorkspace>,
    ) -> Result<RecoveryState> {
        let root = self.root_snapshot(driver, permit).await?;
        if let Some(mut task) = self
            .task
            .as_ref()
            .and_then(|task| task.inputs.current_task.clone())
        {
            task.covered_input_sequence = self.incorporated_input_sequence;
            task.confirmed_effect_watermark = self.confirmed_effect_watermark;
            return TaskRecovery::capture(
                task,
                self.shared.model,
                &self.shared.payload.policy.provider,
                root,
            )
            .map(RecoveryState::Task);
        }
        let workspace =
            workspace.ok_or_else(|| recovery_invalid("ordinary workspace is not ready"))?;
        TurnSnapshot::capture(
            self.shared.payload,
            workspace,
            self.turn_checkpoint.clone(),
            root,
        )
        .map(RecoveryState::Turn)
    }
}

fn committed(receipt: &crate::RecoveryReceipt) {
    let lease_id = receipt.lease_id.as_str();
    let event_id = receipt.event_id.as_str();
    let fencing_token = receipt.fencing_token;
    let checkpoint_id = receipt.checkpoint.id();
    let generation = receipt.checkpoint.generation();
    tracing::debug!(
        event = "respawn_checkpoint_committed",
        lease_id,
        event_id,
        fencing_token,
        checkpoint_id,
        generation
    );
}

pub(super) fn restored(run: &AgentRun<'_>) {
    let (checkpoint_id, generation) = if let Some(recovered) = run.recovered_turn {
        (
            recovered.descriptor.checkpoint_id.0.as_str(),
            recovered.descriptor.generation,
        )
    } else if let Some(recovered) = run.recovered_task {
        let (Some(id), Some(generation)) = (
            recovered.task.checkpoint_id.as_deref(),
            recovered.task.checkpoint_generation,
        ) else {
            return;
        };
        (id, generation)
    } else {
        return;
    };
    let lease_id = run.lease.lease_id.as_ref();
    let event_id = run.lease.event.event_id.as_ref();
    let fencing_token = run.lease.fencing_token;
    tracing::debug!(
        event = "respawn_checkpoint_restored",
        lease_id,
        event_id,
        fencing_token,
        checkpoint_id,
        generation
    );
}

/// Continuing plain roots use the same ordered boundary hook as task roots.
pub(super) struct Root;

impl<'s, 'run> guffbolt::Extension<Harness<'s, 'run>> for Root {
    async fn before_model(
        &mut self,
        root: &mut Harness<'s, 'run>,
        _number: u64,
        state: &DriveState,
    ) -> Option<Ending> {
        root.save_boundary(state).await
    }
}

fn skipped(reason: RecoverySkip) {
    tracing::debug!(event = "run_checkpoint_skipped", reason = ?reason);
}
