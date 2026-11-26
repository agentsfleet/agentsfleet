//! Sandbox build and quiescence own recovery; the model sees only authenticated history.

use af_core::id::Uuid7;
use af_wire::durable_task::{
    CheckpointMode, CheckpointPlan, DurableTaskDisposition, DurableTaskTarget,
};
use afr_agent::{DurableTaskRecovery, PreparedDurableTask};
use afr_engine::{Quiescence, Sandbox};
use zeroize::Zeroizing;

use super::{Run, build::Build};
use crate::{
    Result, error,
    recovery::{Capture, Restore},
    report::Ending,
};

#[derive(Debug)]
pub(super) struct State {
    pub(super) history: Option<DurableTaskRecovery>,
    pub(super) turn: Option<afr_agent::TurnRecovery>,
    pub(super) base: Option<CheckpointPlan>,
    pub(super) base_paths: Vec<String>,
}

pub(super) fn api_toolbox() -> String {
    af_core::etag::sha256_hex(b"agentsfleet.api-only-recovery.v1")
}

impl Run<'_> {
    pub(super) async fn prepare_recovery(
        &self,
        sandbox: &dyn Sandbox,
        build: &Build<'_>,
    ) -> Result<()> {
        if self.payload.turn_resume.is_some() {
            return self.prepare_turn_recovery(sandbox, build).await;
        }
        let Some(prepared) = build.prepared_durable_task else {
            return Ok(());
        };
        let task = prepared
            .inputs
            .current_durable_task
            .as_ref()
            .ok_or_else(|| error::config("durable workspace has no native task"))?;
        let toolbox = sandbox
            .environment()
            .root
            .toolbox()
            .ok_or_else(|| error::config("sandbox has no verified recovery toolbox"))?;
        let scope = Uuid7::parse(&self.payload.event.workspace_id)?;
        let permit = if task.checkpoint_id.is_some()
            || matches!(task.target, DurableTaskTarget::ExistingPr { .. })
        {
            Some(self.lessee.recovery.file(build.stop).await?)
        } else {
            None
        };
        // A failure is a failed build; teardown owns the entire sandbox, so
        // no partially restored executor enters the slot.
        prove_quiescent(sandbox).await?;
        let state: State = self
            .native_workspace(task, sandbox, &scope, toolbox, permit.as_ref())
            .await?;
        if build.recovery.set(state).is_err() {
            return Err(error::config("sandbox recovery was prepared twice"));
        }
        Ok(())
    }

    pub(super) async fn recover_without_sandbox(
        &self,
        prepared: Option<&PreparedDurableTask>,
    ) -> Result<Option<DurableTaskRecovery>> {
        let Some(task) =
            prepared.and_then(|prepared| prepared.inputs.current_durable_task.as_ref())
        else {
            return Ok(None);
        };
        if task.checkpoint_id.is_none() {
            return Ok(None);
        }
        if !matches!(task.target, DurableTaskTarget::ApiOnly) {
            return Err(error::config("repository recovery requires a sandbox"));
        }
        let scope = Uuid7::parse(&self.payload.event.workspace_id)?;
        let toolbox = api_toolbox();
        let permit = self.lessee.recovery.file(&self.interrupt).await?;
        let restored = Restore {
            plane: &self.lessee.plane,
            transfers: &self.lessee.recovery.transfers,
            permit: Some(&permit),
            run: &self.ids.run,
            workspace: &scope,
            fence: self.payload.fencing_token,
            task,
            toolbox: &toolbox,
            stop: &self.interrupt,
        }
        .run(None, std::future::ready(Ok(())))
        .await?;
        Ok(Some(recovered_history(
            task,
            &restored.plan,
            &restored.history,
        )))
    }

    pub(super) async fn capture_final(
        &self,
        sandbox: &dyn Sandbox,
        state: Option<&State>,
        ending: &mut Ending,
    ) {
        let result: Result<()> = async {
            let Ending::Ran { output, .. } = ending else {
                return Ok(());
            };
            let Some(recovery) = output.recovery.as_ref() else {
                return Ok(());
            };
            prove_quiescent(sandbox).await?;
            let state =
                state.ok_or_else(|| error::config("task workspace has no recovery owner"))?;
            let toolbox = sandbox
                .environment()
                .root
                .toolbox()
                .ok_or_else(|| error::config("task toolbox is unavailable for capture"))?;
            let scope = Uuid7::parse(&self.payload.event.workspace_id)?;
            let staged = self.lessee.staging.stage(sandbox.id())?;
            staged.export(sandbox.executor()).await?;
            let saved = Capture {
                plane: &self.lessee.plane,
                transfers: &self.lessee.recovery.transfers,
                run: &self.ids.run,
                workspace: &scope,
                fence: self.payload.fencing_token,
                task: &recovery.task,
                toolbox,
                mode: CheckpointMode::Incremental,
                base: state.base.as_ref(),
                stop: &self.interrupt,
            }
            .run(
                Some(staged.path().to_owned()),
                recovery.history.clone(),
                state.base_paths.clone(),
            )
            .await?;
            if let Some(report) = output.task.as_mut() {
                report.checkpoint_id = Some(saved.checkpoint_id);
            }
            output.recovery = None;
            Ok(())
        }
        .await;
        if let Err(failure) = result {
            capture_failed(ending, &failure);
        }
    }

    pub(super) async fn capture_without_sandbox(&self, ending: &mut Ending) {
        if self.interrupt.is_cancelled() {
            capture_unavailable(ending);
            return;
        }
        let result: Result<()> = async {
            let Ending::Ran { output, .. } = ending else {
                return Ok(());
            };
            let Some(recovery) = output.recovery.as_ref() else {
                return Ok(());
            };
            if !matches!(recovery.task.target, DurableTaskTarget::ApiOnly) {
                return Err(error::config("repository capture has no sandbox"));
            }
            let _reservation = self.lessee.recovery.file(&self.interrupt).await?;
            let scope = Uuid7::parse(&self.payload.event.workspace_id)?;
            let toolbox = api_toolbox();
            let saved = Capture {
                plane: &self.lessee.plane,
                transfers: &self.lessee.recovery.transfers,
                run: &self.ids.run,
                workspace: &scope,
                fence: self.payload.fencing_token,
                task: &recovery.task,
                toolbox: &toolbox,
                mode: CheckpointMode::Incremental,
                base: None,
                stop: &self.interrupt,
            }
            .run(None, recovery.history.clone(), Vec::new())
            .await?;
            if let Some(report) = output.task.as_mut() {
                report.checkpoint_id = Some(saved.checkpoint_id);
            }
            output.recovery = None;
            Ok(())
        }
        .await;
        if let Err(failure) = result {
            capture_failed(ending, &failure);
        }
    }
}

fn capture_failed(ending: &mut Ending, failure: &crate::Error) {
    tracing::warn!(event = "task_checkpoint_failed", error_code = %failure.code());
    capture_unavailable(ending);
}

pub(super) fn capture_unavailable(ending: &mut Ending) {
    if let Ending::Ran { output, .. } = ending {
        if output.task.is_none() && output.recovery.is_none() {
            return;
        }
        if let Some(task) = output.task.as_mut() {
            task.disposition = DurableTaskDisposition::Blocked;
            task.decision_call_id = None;
            task.checkpoint_id = None;
        }
        output.recovery = None;
        output.result.content = "The task is blocked because its durable state could not be saved. No automatic continuation was authorized.".into();
    }
}

/// Only a newly created checkout, exported into its stage, may be read by
/// Git on the host. Recovered or tenant-modified repositories never take
/// this path.
pub(super) async fn verify_fresh_checkout(
    root: &std::path::Path,
    repository: &str,
    head: &str,
) -> Result<()> {
    let name = repository
        .split_once('/')
        .map(|(_, name)| name)
        .filter(|name| !name.is_empty() && !name.contains('/') && !matches!(*name, "." | ".."))
        .ok_or_else(|| error::config("native recovery repository name is invalid"))?;
    let destination = root.join(name);
    let expected = gix::hash::ObjectId::from_hex(head.as_bytes()).map_err(|source| {
        error::boundary("fresh task checkout expected head is invalid", source)
    })?;
    tokio::task::spawn_blocking(move || {
        let repository = gix::open_opts(destination, gix::open::Options::isolated())
            .map_err(|source| error::boundary("fresh task checkout cannot be opened", source))?;
        let actual = repository
            .head_id()
            .map_err(|source| error::boundary("fresh task checkout has no head", source))?;
        if actual.detach() != expected {
            return Err(error::config("task branch moved before initial capture"));
        }
        Ok(())
    })
    .await?
}

/// A restored history carries the cursors authenticated by its saved manifest.
pub(super) fn recovered_history(
    task: &af_wire::durable_task::DurableTaskContext<'static>,
    plan: &CheckpointPlan,
    history: &af_crypto::secret::SecretBytes,
) -> DurableTaskRecovery {
    let mut task = task.clone();
    task.covered_input_sequence = plan.covered_input_sequence;
    task.confirmed_effect_watermark = plan.effect_watermark;
    DurableTaskRecovery {
        task,
        history: Zeroizing::new(history.expose().to_vec()),
    }
}

#[cfg(test)]
#[path = "tests/recovery/ownership.rs"]
mod ownership_tests;

/// Refuses a capture or restore while a tenant process remains: only the
/// supervisor starts processes, and it is busy with this one, so a quiet
/// sandbox stays quiet until the work is done.
pub(super) async fn prove_quiescent(sandbox: &dyn Sandbox) -> Result<()> {
    match sandbox
        .quiesce()
        .await
        .map_err(|source| error::boundary("cannot prove tenant quiescence", source))?
    {
        Quiescence::Quiet => Ok(()),
        Quiescence::Busy => Err(error::config(
            "a tenant process still owns the recovery workspace",
        )),
    }
}
