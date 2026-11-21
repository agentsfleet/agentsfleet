//! Native task files and base custody remain separate from event-owned recovery.

use super::{
    Run,
    recovery::{State, recovered_history, verify_fresh_checkout},
};
use crate::{
    Result,
    recovery::{Capture, Restore},
};
use af_core::id::Uuid7;
use af_wire::task::{CheckpointMode, TaskContext, TaskTarget};
use zeroize::Zeroizing;

impl Run<'_> {
    pub(super) async fn native_workspace(
        &self,
        task: &TaskContext<'static>,
        destination: (std::path::PathBuf, (u32, u32)),
        scope: &Uuid7,
        toolbox: &str,
        permit: Option<&afr_agent::FileRecoveryPermit>,
    ) -> Result<State> {
        if task.checkpoint_id.is_some() {
            let restored = Restore {
                plane: &self.lessee.plane,
                transfers: &self.lessee.recovery.transfers,
                permit,
                run: &self.ids.run,
                workspace: scope,
                fence: self.payload.fencing_token,
                task,
                toolbox,
                stop: &self.interrupt,
            }
            .run(Some(destination))
            .await?;
            return Ok(State {
                turn: None,
                history: Some(recovered_history(task, &restored.plan, &restored.history)),
                base: Some(restored.plan),
                base_paths: restored.base_paths,
            });
        }
        if let TaskTarget::ExistingPr {
            repository, head, ..
        } = &task.target
        {
            verify_fresh_checkout(&destination.0, repository, head).await?;
            return self
                .native_base(task, destination.0, scope, toolbox, permit)
                .await;
        }
        Ok(State {
            turn: None,
            history: None,
            base: None,
            base_paths: Vec::new(),
        })
    }

    async fn native_base(
        &self,
        task: &TaskContext<'static>,
        root: std::path::PathBuf,
        scope: &Uuid7,
        toolbox: &str,
        permit: Option<&afr_agent::FileRecoveryPermit>,
    ) -> Result<State> {
        let base = Capture {
            plane: &self.lessee.plane,
            transfers: &self.lessee.recovery.transfers,
            run: &self.ids.run,
            workspace: scope,
            fence: self.payload.fencing_token,
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
            transfers: &self.lessee.recovery.transfers,
            permit,
            run: &self.ids.run,
            workspace: scope,
            fence: self.payload.fencing_token,
            task,
            toolbox,
            stop: &self.interrupt,
        }
        .base_paths(&base)
        .await?;
        Ok(State {
            turn: None,
            history: None,
            base: Some(base),
            base_paths,
        })
    }
}
