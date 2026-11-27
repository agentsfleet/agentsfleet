//! Verify complete base and overlay roots, then land the tree through the
//! sandbox's engine, before the task's history is trusted.

use super::source::{Custody as _, Landing, TaskReading, land};
use super::{Object, Session, transfer::Transfers};
use crate::{Result, client::ControlPlane, error};
use af_core::id::Uuid7;
use af_crypto::secret::SecretBytes;
use af_wire::activity::{
    DurableTaskActivityOperation as Operation, DurableTaskActivityPhase as Phase,
};
use af_wire::durable_task::{
    CHECKPOINT_FORMAT, CheckpointAuthorizationInput, CheckpointMode, CheckpointPlan,
    CheckpointTransferPurpose, DecisionDurableTaskVersion, DurableTaskContext,
};
use zeroize::Zeroizing;

#[derive(Debug)]
pub(crate) struct Restore<'a> {
    pub(crate) plane: &'a ControlPlane,
    pub(crate) transfers: &'a Transfers,
    pub(crate) run: &'a Uuid7,
    pub(crate) workspace: &'a Uuid7,
    pub(crate) fence: u64,
    pub(crate) task: &'a DurableTaskContext<'a>,
    pub(crate) toolbox: &'a str,
    pub(crate) stop: &'a tokio_util::sync::CancellationToken,
}

#[derive(Debug)]
pub(crate) struct Restored {
    pub(crate) plan: CheckpointPlan,
    pub(crate) history: SecretBytes,
    pub(crate) base_paths: Vec<String>,
}

impl Restore<'_> {
    /// Inspect the authenticated retained manifest, including on cross-task reuse.
    pub(crate) async fn base_paths(&self, base: &CheckpointPlan) -> Result<Vec<String>> {
        let checkpoint = Uuid7::parse(&base.checkpoint_id)?;
        let task = DecisionDurableTaskVersion {
            durable_task_id: self.task.durable_task_id.to_string(),
            generation: self.task.generation,
        };
        let response = self
            .plane
            .durable_task_checkpoint_authorizations(
                self.run,
                &checkpoint,
                &CheckpointAuthorizationInput {
                    fencing_token: self.fence,
                    durable_task: task.clone(),
                    purpose: CheckpointTransferPurpose::Restore,
                    pin_id: None,
                    object_ids: Vec::new(),
                },
            )
            .await?;
        let unexpected = !response.objects.is_empty();
        // Wrapped so each unexpected object's data key is wiped as it drops.
        response
            .objects
            .into_iter()
            .map(Object::from)
            .for_each(drop);
        let plan = response.plan;
        if unexpected
            || plan.checkpoint_id != base.checkpoint_id
            || plan.base_id != base.base_id
            || plan.format_version != CHECKPOINT_FORMAT
            || plan.toolbox_digest != self.toolbox
            || plan.mode != CheckpointMode::RetainBase
            || plan.manifest_root != base.manifest_root
        {
            return Err(error::config(
                "retained base inspection changed native identity",
            ));
        }
        let mut progress = super::progress::Progress::default();
        let mut session = Session {
            plane: self.plane,
            run: self.run,
            workspace: self.workspace,
            fence: self.fence,
            task,
            activity_id: plan.checkpoint_id.clone(),
            plan,
            progress: &mut progress,
            transfers: self.transfers.clone(),
            stop: self.stop,
        };
        let ids = session.plan.page_ids.clone();
        let expected = session
            .plan
            .manifest_root
            .clone()
            .ok_or_else(|| error::config("retained base is incomplete"))?;
        let mut entries = af_chunk_store::Resolved::default();
        let history = session.pages(&ids, &expected, &mut entries, &mut 0).await?;
        if !history.is_empty() {
            return Err(error::config("retained base manifest is incompatible"));
        }
        Ok(entries.into_entries().into_keys().collect())
    }

    /// Restores into `landing`'s sandbox, or with none a checkpoint that
    /// holds no file, and only then reports the restore done: a restore whose
    /// landing fails is reported failed.
    pub(crate) async fn run(&self, landing: Option<Landing<'_>>) -> Result<Restored> {
        let activity = af_crypto::entropy::Entropy::new()
            .uuid7(af_core::clock::now())?
            .to_string();
        super::activity::emit(
            self.plane,
            self.run,
            &self.task.durable_task_id,
            &activity,
            Operation::Restore,
            Phase::Preparing,
            None,
        )
        .await;
        let mut progress = super::progress::Progress::default();
        let restored = self.restore(&activity, &mut progress, landing).await;
        let phase = if restored.is_ok() {
            Phase::Restored
        } else {
            Phase::Failed
        };
        super::activity::emit(
            self.plane,
            self.run,
            &self.task.durable_task_id,
            &activity,
            Operation::Restore,
            phase,
            Some(&progress),
        )
        .await;
        restored
    }

    fn checked_plan(
        &self,
        checkpoint: &Uuid7,
        response: af_wire::durable_task::CheckpointAuthorizations,
    ) -> Result<CheckpointPlan> {
        let unexpected = !response.objects.is_empty();
        // Wrapped so each unexpected object's data key is wiped as it drops.
        response
            .objects
            .into_iter()
            .map(Object::from)
            .for_each(drop);
        let plan = response.plan;
        if unexpected
            || plan.checkpoint_id != checkpoint.as_str()
            || plan.format_version != CHECKPOINT_FORMAT
            || plan.toolbox_digest != self.toolbox
            || plan.mode != CheckpointMode::Incremental
            || plan.covered_input_sequence > self.task.covered_input_sequence
            || plan.effect_watermark > self.task.confirmed_effect_watermark
        {
            return Err(error::config(
                "checkpoint bootstrap is incompatible with current task",
            ));
        }
        Ok(plan)
    }

    async fn restore(
        &self,
        activity: &str,
        progress: &mut super::progress::Progress,
        landing: Option<Landing<'_>>,
    ) -> Result<Restored> {
        let checkpoint = Uuid7::parse(
            self.task
                .checkpoint_id
                .as_deref()
                .ok_or_else(|| error::config("task has no recovery checkpoint"))?,
        )?;
        let task = DecisionDurableTaskVersion {
            durable_task_id: self.task.durable_task_id.to_string(),
            generation: self.task.generation,
        };
        let response = self
            .plane
            .durable_task_checkpoint_authorizations(
                self.run,
                &checkpoint,
                &CheckpointAuthorizationInput {
                    fencing_token: self.fence,
                    durable_task: task.clone(),
                    purpose: CheckpointTransferPurpose::Restore,
                    pin_id: None,
                    object_ids: Vec::new(),
                },
            )
            .await?;
        let plan = self.checked_plan(&checkpoint, response)?;
        let generation = plan.generation;
        // Read only where files land: a task that ran with no sandbox lands
        // none.
        let taken = plan
            .sandbox
            .as_ref()
            .map(|sandbox| super::taken::environment(&plan.toolbox_digest, sandbox))
            .ok_or_else(|| error::config("checkpoint was taken in no sandbox"));
        let mut session = Session {
            plane: self.plane,
            run: self.run,
            workspace: self.workspace,
            fence: self.fence,
            task,
            activity_id: activity.to_owned(),
            plan,
            progress,
            transfers: self.transfers.clone(),
            stop: self.stop,
        };
        session
            .progress(Operation::Restore, Phase::Verifying, None)
            .await;
        let mut reading = TaskReading::new(&mut session);
        if let Some(landing) = landing {
            reading = land(reading, (checkpoint, generation), taken?, landing).await?;
        } else if !reading.tree().await?.entries().is_empty() {
            return Err(error::config("checkpoint files require a workspace"));
        }
        let TaskReading {
            history: history_ids,
            base_paths,
            ..
        } = reading;
        let history = session.history(history_ids).await?;
        Ok(Restored {
            plan: session.plan,
            history,
            base_paths,
        })
    }
}

impl Session<'_> {
    async fn history(&mut self, ids: Vec<String>) -> Result<SecretBytes> {
        let mut history = Zeroizing::new(Vec::new());
        let mut digests = Vec::new();
        for batch in ids.chunks(4) {
            let chunks = self.download_batch(batch).await?;
            for (id, bytes) in batch.iter().zip(chunks) {
                digests.push((
                    id.clone(),
                    af_core::etag::sha256_hex(bytes.expose()),
                    crate::byte_count(bytes.expose().len()),
                ));
                history.extend_from_slice(bytes.expose());
            }
        }
        let root = af_crypto::checkpoint_digest::history_root(
            digests
                .iter()
                .map(|(id, digest, size)| (id.as_str(), digest.as_str(), *size)),
        );
        if self.plan.history_digest.as_deref() != Some(root.as_str()) {
            return Err(error::config("checkpoint history root mismatch"));
        }
        Ok(SecretBytes::new(std::mem::take(&mut *history)))
    }
}

#[cfg(test)]
#[path = "tests/restore/nonempty.rs"]
mod nonempty_tests;
