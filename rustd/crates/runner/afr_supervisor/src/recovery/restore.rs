//! Verify complete base and overlay roots before writing a fresh frozen workspace.

use super::{Object, Session, transfer::Transfers, window};
use crate::{Result, client::ControlPlane, error};
use af_core::id::Uuid7;
use af_crypto::secret::SecretBytes;
use af_wire::activity::{TaskActivityOperation as Operation, TaskActivityPhase as Phase};
use af_wire::task::{
    CHECKPOINT_FORMAT, CheckpointAuthorizationInput, CheckpointEntry, CheckpointMode,
    CheckpointPlan, CheckpointTransferPurpose, DecisionTaskVersion, TaskContext,
};
use std::{collections::BTreeMap, path::PathBuf};
use zeroize::Zeroizing;

#[derive(Debug)]
pub(crate) struct Restore<'a> {
    pub(crate) plane: &'a ControlPlane,
    pub(crate) transfers: &'a Transfers,
    pub(crate) permit: Option<&'a afr_agent::FileRecoveryPermit>,
    pub(crate) run: &'a Uuid7,
    pub(crate) workspace: &'a Uuid7,
    pub(crate) fence: u64,
    pub(crate) task: &'a TaskContext<'a>,
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
        let task = DecisionTaskVersion {
            task_id: self.task.task_id.to_string(),
            generation: self.task.generation,
        };
        let response = self
            .plane
            .task_checkpoint_authorizations(
                self.run,
                &checkpoint,
                &CheckpointAuthorizationInput {
                    fencing_token: self.fence,
                    task: task.clone(),
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
        let mut entries = BTreeMap::new();
        let history = session.pages(&ids, &expected, &mut entries, &mut 0).await?;
        if !history.is_empty() || entries.len() > 200_000 {
            return Err(error::config("retained base manifest is incompatible"));
        }
        Ok(entries.into_keys().collect())
    }

    pub(crate) async fn run(&self, destination: Option<(PathBuf, (u32, u32))>) -> Result<Restored> {
        let activity = af_crypto::entropy::Entropy::new()
            .uuid7(af_core::clock::now())?
            .to_string();
        super::activity::emit(
            self.plane,
            self.run,
            &self.task.task_id,
            &activity,
            Operation::Restore,
            Phase::Preparing,
            None,
        )
        .await;
        let mut progress = super::progress::Progress::default();
        let restored = self.restore(&activity, &mut progress, destination).await;
        let phase = if restored.is_ok() {
            Phase::Restored
        } else {
            Phase::Failed
        };
        super::activity::emit(
            self.plane,
            self.run,
            &self.task.task_id,
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
        response: af_wire::task::CheckpointAuthorizations,
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
        destination: Option<(PathBuf, (u32, u32))>,
    ) -> Result<Restored> {
        let checkpoint = Uuid7::parse(
            self.task
                .checkpoint_id
                .as_deref()
                .ok_or_else(|| error::config("task has no recovery checkpoint"))?,
        )?;
        let task = DecisionTaskVersion {
            task_id: self.task.task_id.to_string(),
            generation: self.task.generation,
        };
        let response = self
            .plane
            .task_checkpoint_authorizations(
                self.run,
                &checkpoint,
                &CheckpointAuthorizationInput {
                    fencing_token: self.fence,
                    task: task.clone(),
                    purpose: CheckpointTransferPurpose::Restore,
                    pin_id: None,
                    object_ids: Vec::new(),
                },
            )
            .await?;
        let plan = self.checked_plan(&checkpoint, response)?;
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
        let (entries, history_ids, base_paths) = session.manifest().await?;
        session
            .restore_files(destination, entries, self.permit)
            .await?;
        let history = session.history(history_ids).await?;
        Ok(Restored {
            plan: session.plan,
            history,
            base_paths,
        })
    }
}

impl Session<'_> {
    async fn restore_files(
        &mut self,
        destination: Option<(PathBuf, (u32, u32))>,
        entries: BTreeMap<String, CheckpointEntry>,
        permit: Option<&afr_agent::FileRecoveryPermit>,
    ) -> Result<()> {
        super::links::validate(entries.values().filter_map(|entry| match entry {
            CheckpointEntry::Symlink { path, target } => Some((path.as_str(), target.as_str())),
            CheckpointEntry::File(_)
            | CheckpointEntry::Directory { .. }
            | CheckpointEntry::Remove { .. } => None,
        }))?;
        let Some((path, owner)) = destination else {
            return if entries.is_empty() {
                Ok(())
            } else {
                Err(error::config("checkpoint files require a workspace"))
            };
        };
        window::apply(self, (path, owner), entries, permit, |session, ids| {
            Box::pin(async move {
                if ids.is_empty() {
                    session.renew().await?;
                    return Ok(Vec::new());
                }
                let chunks = session.download_batch(ids).await?;
                session.renew().await?;
                Ok(chunks)
            })
        })
        .await
    }

    async fn history(&mut self, ids: Vec<String>) -> Result<SecretBytes> {
        let mut history = Zeroizing::new(Vec::new());
        let mut digests = Vec::new();
        for batch in ids.chunks(4) {
            let chunks = self.download_batch(batch).await?;
            for (id, bytes) in batch.iter().zip(chunks) {
                if history.len().saturating_add(bytes.expose().len()) > 64 * 1024 * 1024 {
                    return Err(error::config("checkpoint history expanded bound exceeded"));
                }
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
