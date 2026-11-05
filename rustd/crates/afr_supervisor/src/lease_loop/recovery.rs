//! Sandbox build and freeze own recovery; the model sees only authenticated history.

use super::{LeaseRun, build::Build};
use crate::{
    Result, error,
    recovery::{Capture, Restore},
    report::Ending,
};
use afd_core::id::Uuid7;
use afd_wire::task::{CheckpointMode, CheckpointPlan, TaskDisposition, TaskTarget};
use afr_agent::{PreparedTask, TaskRecovery};
use afr_sandbox::Sandbox;
use zeroize::Zeroizing;

#[derive(Debug)]
pub(super) struct State {
    pub(super) history: Option<TaskRecovery>,
    base: Option<CheckpointPlan>,
    base_paths: Vec<String>,
}

fn api_toolbox() -> String {
    afd_core::etag::sha256_hex(b"agentsfleet.api-only-recovery.v1")
}

impl LeaseRun<'_> {
    pub(super) async fn prepare_recovery(
        &self,
        sandbox: &dyn Sandbox,
        build: &Build<'_>,
    ) -> Result<()> {
        let Some(prepared) = build
            .prepared_task
            .filter(|prepared| prepared.allow_actions)
        else {
            return Ok(());
        };
        let task = prepared
            .inputs
            .current_task
            .as_ref()
            .ok_or_else(|| error::config("durable workspace has no native task"))?;
        let workspace = sandbox
            .workspace()
            .ok_or_else(|| error::config("sandbox has no trusted snapshot workspace"))?;
        let toolbox = sandbox
            .toolbox_digest()
            .ok_or_else(|| error::config("sandbox has no verified recovery toolbox"))?;
        let scope = Uuid7::parse(&self.lease.event.workspace_id)?;
        sandbox
            .freeze()
            .await
            .map_err(|source| error::boundary("sandbox could not freeze for recovery", source))?;
        let restored: Result<State> = async {
            if task.checkpoint_id.is_some() {
                let restored = Restore {
                    plane: &self.lessee.plane,
                    lease: &self.ids.lease,
                    workspace: &scope,
                    fence: self.lease.fencing_token,
                    task,
                    toolbox,
                    stop: &self.interrupt,
                }
                .run(Some((workspace.root.to_owned(), workspace.owner)))
                .await?;
                Ok(State {
                    history: Some(recovered_history(task, &restored.plan, &restored.history)),
                    base: Some(restored.plan),
                    base_paths: restored.base_paths,
                })
            } else if let TaskTarget::ExistingPr {
                repository, head, ..
            } = &task.target
            {
                let root = workspace.root.to_owned();
                verify_fresh_checkout(&root, repository, head).await?;
                let base = Capture {
                    plane: &self.lessee.plane,
                    lease: &self.ids.lease,
                    workspace: &scope,
                    fence: self.lease.fencing_token,
                    task,
                    toolbox,
                    mode: CheckpointMode::RetainBase,
                    base: None,
                    stop: &self.interrupt,
                }
                .run(Some(root), Zeroizing::new(Vec::new()), Vec::new())
                .await?;
                let base_paths = Restore {
                    plane: &self.lessee.plane,
                    lease: &self.ids.lease,
                    workspace: &scope,
                    fence: self.lease.fencing_token,
                    task,
                    toolbox,
                    stop: &self.interrupt,
                }
                .base_paths(&base)
                .await?;
                Ok(State {
                    history: None,
                    base: Some(base),
                    base_paths,
                })
            } else {
                Ok(State {
                    history: None,
                    base: None,
                    base_paths: Vec::new(),
                })
            }
        }
        .await;
        // Failure remains a failed build even when thaw also fails; teardown owns
        // the entire sandbox. No partially restored executor enters the slot.
        let thawed = sandbox
            .thaw()
            .await
            .map_err(|source| error::boundary("sandbox could not thaw after recovery", source));
        let state: State = restored?;
        thawed?;
        if build.recovery.set(state).is_err() {
            return Err(error::config("sandbox recovery was prepared twice"));
        }
        Ok(())
    }

    pub(super) async fn recover_without_sandbox(
        &self,
        prepared: Option<&PreparedTask>,
    ) -> Result<Option<TaskRecovery>> {
        let Some(task) = prepared
            .filter(|prepared| prepared.allow_actions)
            .and_then(|prepared| prepared.inputs.current_task.as_ref())
        else {
            return Ok(None);
        };
        if task.checkpoint_id.is_none() {
            return Ok(None);
        }
        if !matches!(task.target, TaskTarget::ApiOnly) {
            return Err(error::config("repository recovery requires a sandbox"));
        }
        let scope = Uuid7::parse(&self.lease.event.workspace_id)?;
        let toolbox = api_toolbox();
        let restored = Restore {
            plane: &self.lessee.plane,
            lease: &self.ids.lease,
            workspace: &scope,
            fence: self.lease.fencing_token,
            task,
            toolbox: &toolbox,
            stop: &self.interrupt,
        }
        .run(None)
        .await?;
        Ok(Some(recovered_history(
            task,
            &restored.plan,
            &restored.history,
        )))
    }

    pub(super) async fn capture_frozen(
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
            let state =
                state.ok_or_else(|| error::config("task workspace has no recovery owner"))?;
            let workspace = sandbox
                .workspace()
                .ok_or_else(|| error::config("task workspace is unavailable for capture"))?;
            let toolbox = sandbox
                .toolbox_digest()
                .ok_or_else(|| error::config("task toolbox is unavailable for capture"))?;
            let scope = Uuid7::parse(&self.lease.event.workspace_id)?;
            let saved = Capture {
                plane: &self.lessee.plane,
                lease: &self.ids.lease,
                workspace: &scope,
                fence: self.lease.fencing_token,
                task: &recovery.task,
                toolbox,
                mode: CheckpointMode::Incremental,
                base: state.base.as_ref(),
                stop: &self.interrupt,
            }
            .run(
                Some(workspace.root.to_owned()),
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
            if !matches!(recovery.task.target, TaskTarget::ApiOnly) {
                return Err(error::config("repository capture has no sandbox"));
            }
            let scope = Uuid7::parse(&self.lease.event.workspace_id)?;
            let toolbox = api_toolbox();
            let saved = Capture {
                plane: &self.lessee.plane,
                lease: &self.ids.lease,
                workspace: &scope,
                fence: self.lease.fencing_token,
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
            task.disposition = TaskDisposition::Blocked;
            task.decision_call_id = None;
            task.checkpoint_id = None;
        }
        output.recovery = None;
        output.result.content = "The task is blocked because its durable state could not be saved. No automatic continuation was authorized.".into();
    }
}

/// Only a newly created checkout may be read by Git on the host.
/// Recovered or tenant-modified repositories never take this path.
async fn verify_fresh_checkout(root: &std::path::Path, repository: &str, head: &str) -> Result<()> {
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
fn recovered_history(
    task: &afd_wire::task::TaskContext<'static>,
    plan: &CheckpointPlan,
    history: &afd_crypto::secret::SecretBytes,
) -> TaskRecovery {
    let mut task = task.clone();
    task.covered_input_sequence = plan.covered_input_sequence;
    task.confirmed_effect_watermark = plan.effect_watermark;
    TaskRecovery {
        task,
        history: Zeroizing::new(history.expose().to_vec()),
    }
}
