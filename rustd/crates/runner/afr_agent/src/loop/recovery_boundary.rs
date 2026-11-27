//! Restore and capture keep the private root, memory and the run's children
//! together: the children ride the root's drive state.

use af_secrets::Scrub;
use af_wire::durable_task::DurableTaskContext;
use af_wire::run::{EarlierCall, TurnWorkspace};
use afr_memory::Loaded;
use guffbolt::Message;
use guffbolt::agents::Pause;

use super::Ending;
use super::Harness;
use super::recovery_receipt::Owner;
use crate::engine::{AgentRun, RootRecovery};
use crate::error::recovery_invalid;
use crate::{
    DurableTaskRecovery, RecoverySkip, RecoverySnapshot, RecoveryState, Result, TurnSnapshot,
};
use guffbolt::DriveState;

pub(super) struct RootSetup<'run> {
    pub(super) opening: Vec<Message>,
    pub(super) state: Option<RootRecovery>,
    pub(super) memory: Loaded<'run>,
}

pub(super) fn prepare_root<'run>(
    run: &AgentRun<'run>,
    scrub: &Scrub,
    mut opening: Vec<Message>,
    message: &str,
) -> Result<RootSetup<'run>> {
    if (run.recovered_durable_task.is_some() && run.recovered_turn.is_some())
        || (run.run.turn_resume.is_some() && run.recovered_turn.is_none())
    {
        return Err(recovery_invalid(
            "assigned recovery has no single complete restore owner",
        ));
    }
    let mut state = if let Some(recovered) = run.recovered_durable_task {
        let current = run
            .prepared_durable_task
            .and_then(|prepared| prepared.inputs.current_durable_task.as_ref())
            .ok_or_else(|| recovery_invalid("recovered history has no current native task"))?;
        Some(DurableTaskRecovery::restore(
            &recovered.history,
            current,
            &run.run.policy.context.model,
            &run.run.policy.provider,
            scrub,
        )?)
    } else if let Some(recovered) = run.recovered_turn {
        Some(recovered.restore(run.run, recovered.workspace.clone(), scrub)?)
    } else {
        None
    };
    // What a dead run of this turn already did, so this run does not do it again.
    let finished = scrub
        .text(&finished_note(&run.run.finished_calls))
        .into_owned();
    let memory = if let Some(saved) = &mut state {
        opening = std::mem::take(&mut saved.messages);
        // An answer resumes at the asking call: its result is the next message.
        if run.run.event.question_answer().is_none() {
            opening.push(Message::User(format!("Resume the accepted objective from this authenticated history. Reconcile native inputs and confirmed effects before taking another action.{finished}")));
        }
        Loaded::recover(run.memory, std::mem::take(&mut saved.memory), scrub)?
    } else {
        opening.push(Message::User(format!("{}{finished}", scrub.text(message))));
        Loaded::new(run.memory)
    };
    Ok(RootSetup {
        opening,
        state,
        memory,
    })
}

/// Each call an earlier run of the turn finished, one line apiece: its tool,
/// what it was called with and what it answered. Empty when there were none.
pub(super) fn finished_note(calls: &[EarlierCall<'_>]) -> String {
    if calls.is_empty() {
        return String::new();
    }
    let lines: Vec<String> = calls
        .iter()
        .map(|call| {
            let arguments = serde_json::Value::Object(call.arguments.clone());
            format!("\n- {} {arguments} -> {}", call.tool, call.output)
        })
        .collect();
    let lines = lines.concat();
    format!(
        "\n\nAn earlier run of this turn died after finishing these calls; their effects stand, so do not repeat them:{lines}"
    )
}

impl<'run> Harness<'_, 'run> {
    pub(super) async fn capture_durable_task_root(
        &self,
        task: DurableTaskContext<'static>,
        pause: Option<&Pause>,
    ) -> Result<DurableTaskRecovery> {
        let pause = pause
            .ok_or_else(|| recovery_invalid("child cleanup did not grant a terminal boundary"))?;
        let root = self.root_snapshot(&self.driver, pause).await?;
        DurableTaskRecovery::capture(
            task,
            self.shared.model,
            &self.shared.payload.policy.provider,
            root,
        )
    }

    /// The root as a checkpoint saves it: its transcript, and its loop's
    /// progress with the run's children written in under `pause`.
    async fn root_snapshot(&self, driver: &DriveState, pause: &Pause) -> Result<RootRecovery> {
        let mut driver = driver.clone();
        if !self.shared.registry.save(pause, &mut driver) {
            return Err(recovery_invalid(
                "the child capture permit belongs to another run",
            ));
        }
        let root = RootRecovery {
            messages: self.transcript.messages().to_vec(),
            driver,
            memory: self.shared.run.memory.snapshot().await?,
            pinned: self.context.pinned,
            folds: self.folded_inputs.clone(),
        };
        // Checked as a restore will check it, so a checkpoint whose children
        // could not come back is skipped rather than saved.
        root.check()?;
        Ok(root)
    }

    pub(super) async fn save_boundary(&mut self, driver: &DriveState) -> Option<Ending> {
        let (sink, workspace) = self.boundary(driver)?;
        match self.save_owner() {
            Owner::Free => {}
            Owner::Publishing => {
                skipped(RecoverySkip::BusyPublication);
                return None;
            }
            Owner::Ended(ending) => return Some(ending),
        }
        let Some(pause) = self.shared.registry.pause().await else {
            skipped(RecoverySkip::BusyChildren);
            return None;
        };
        self.save_under(sink, workspace, driver, &pause).await
    }

    /// A checkpoint at the call that asked the person, under the pause the
    /// run's end took, so the answer resumes the run from the asking call. A
    /// refused save already logged why; the run still reports its question.
    pub(super) async fn save_waiting(&mut self, pause: Option<&Pause>) {
        let driver = self.driver.clone();
        let Some((sink, workspace)) = self.boundary(&driver) else {
            return;
        };
        let Some(pause) = pause else {
            skipped(RecoverySkip::BusyChildren);
            return;
        };
        let _ending = self.save_under(sink, workspace, &driver, pause).await;
    }

    /// Where a root's checkpoint goes and the workspace it records, when this
    /// loop takes one at all.
    fn boundary(
        &self,
        driver: &DriveState,
    ) -> Option<(&'run dyn crate::RecoveryBoundarySink, Option<TurnWorkspace>)> {
        if self.depth != 0 || driver.completed_steps() == 0 {
            return None;
        }
        let sink = self.shared.recovery?;
        if self.task.is_some() {
            return Some((sink, None));
        }
        match sink.workspace() {
            Ok(workspace) => Some((sink, Some(workspace))),
            Err(reason) => {
                skipped(reason);
                None
            }
        }
    }

    async fn save_under(
        &mut self,
        sink: &dyn crate::RecoveryBoundarySink,
        workspace: Option<TurnWorkspace>,
        driver: &DriveState,
        pause: &Pause,
    ) -> Option<Ending> {
        if !self.shared.run.sessions.is_quiescent() {
            skipped(RecoverySkip::BusySessions);
            return None;
        }
        let file_permit = match sink.try_reserve(self.task.is_some()) {
            Ok(file_permit) => file_permit,
            Err(reason) => {
                skipped(reason);
                return None;
            }
        };
        let snapshot = match self.snapshot_state(driver, pause, workspace).await {
            Ok(state) => RecoverySnapshot {
                run_id: self.shared.run_id.to_owned(),
                turn_id: self.shared.payload.event.turn_id.to_string(),
                fencing_token: self.shared.fencing_token,
                state,
                file_permit,
            },
            Err(failure) => {
                tracing::debug!(event = "run_checkpoint_skipped", reason = ?RecoverySkip::Limit, error_code = %failure.code());
                return None;
            }
        };
        let expected = snapshot.witness();
        self.accept(sink.save(snapshot).await, expected)
    }

    async fn snapshot_state(
        &self,
        driver: &DriveState,
        pause: &Pause,
        workspace: Option<TurnWorkspace>,
    ) -> Result<RecoveryState> {
        let root = self.root_snapshot(driver, pause).await?;
        if let Some(mut task) = self
            .task
            .as_ref()
            .and_then(|task| task.inputs.current_durable_task.clone())
        {
            task.covered_input_sequence = self.incorporated_input_sequence;
            task.confirmed_effect_watermark = self.confirmed_effect_watermark;
            return DurableTaskRecovery::capture(
                task,
                self.shared.model,
                &self.shared.payload.policy.provider,
                root,
            )
            .map(RecoveryState::DurableTask);
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

pub(super) fn restored(run: &AgentRun<'_>) {
    let (checkpoint_id, generation) = if let Some(recovered) = run.recovered_turn {
        (
            recovered.descriptor.checkpoint_id.0.as_str(),
            recovered.descriptor.generation,
        )
    } else if let Some(recovered) = run.recovered_durable_task {
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
    let run_id = run.run.run_id.as_ref();
    let turn_id = run.run.event.turn_id.as_ref();
    let fencing_token = run.run.fencing_token;
    tracing::debug!(
        event = "respawn_checkpoint_restored",
        run_id,
        turn_id,
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

pub(super) fn skipped(reason: RecoverySkip) {
    tracing::debug!(event = "run_checkpoint_skipped", reason = ?reason);
}
