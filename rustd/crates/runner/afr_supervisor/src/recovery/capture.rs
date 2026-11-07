//! Capture commits only after every immutable object and ordered page verifies.

use super::{Session, catalog::Catalog, files::Workspace, transfer::Transfers};
use crate::{Result, client::ControlPlane, error};
use af_core::id::Uuid7;
use af_wire::activity::{TaskActivityOperation as Operation, TaskActivityPhase as Phase};
use af_wire::task::{
    CHECKPOINT_BATCH, CHECKPOINT_FORMAT, CheckpointCommitInput, CheckpointMode,
    CheckpointObjectStatus, CheckpointPageInput, CheckpointPlan, CheckpointPlanInput,
    DecisionTaskVersion, TaskContext, TaskTarget,
};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use zeroize::Zeroizing;

#[derive(Debug)]
pub(crate) struct Capture<'a> {
    pub(crate) plane: &'a ControlPlane,
    pub(crate) lease: &'a Uuid7,
    pub(crate) workspace: &'a Uuid7,
    pub(crate) fence: u64,
    pub(crate) task: &'a TaskContext<'a>,
    pub(crate) toolbox: &'a str,
    pub(crate) mode: CheckpointMode,
    pub(crate) base: Option<&'a CheckpointPlan>,
    pub(crate) stop: &'a tokio_util::sync::CancellationToken,
}

impl Capture<'_> {
    /// The caller owns a frozen sandbox for this entire future. Cancellation
    /// leaves an unfinished native plan, never a usable recovery pointer.
    pub(crate) async fn run(
        &self,
        root: Option<PathBuf>,
        history: Zeroizing<Vec<u8>>,
        base_paths: Vec<String>,
    ) -> Result<CheckpointPlan> {
        let activity = af_crypto::entropy::Entropy::new()
            .uuid7(af_core::clock::now())?
            .to_string();
        super::activity::emit(
            self.plane,
            self.lease,
            &self.task.task_id,
            &activity,
            Operation::Capture,
            Phase::Preparing,
            None,
        )
        .await;
        let mut progress = super::progress::Progress::default();
        let saved = self
            .capture(&activity, &mut progress, root, history, base_paths)
            .await;
        let phase = if saved.is_ok() {
            Phase::Saved
        } else {
            Phase::Failed
        };
        super::activity::emit(
            self.plane,
            self.lease,
            &self.task.task_id,
            &activity,
            Operation::Capture,
            phase,
            Some(&progress),
        )
        .await;
        saved
    }

    async fn capture(
        &self,
        activity: &str,
        progress: &mut super::progress::Progress,
        root: Option<PathBuf>,
        history: Zeroizing<Vec<u8>>,
        base_paths: Vec<String>,
    ) -> Result<CheckpointPlan> {
        if self.stop.is_cancelled() {
            return Err(error::config("checkpoint capture was cancelled"));
        }
        let stop = self.stop.clone();
        super::activity::emit(
            self.plane,
            self.lease,
            &self.task.task_id,
            activity,
            Operation::Capture,
            Phase::Scanning,
            None,
        )
        .await;
        let catalog = tokio::task::spawn_blocking(move || {
            let root = root
                .map(|path| Workspace::open(&path).map(Arc::new))
                .transpose()?;
            Catalog::build(root.as_ref(), history, base_paths, &stop)
        })
        .await??;
        if self.stop.is_cancelled() {
            return Err(error::config("checkpoint capture was cancelled"));
        }
        let task = DecisionTaskVersion {
            task_id: self.task.task_id.to_string(),
            generation: self.task.generation,
        };
        let allowance = match self.mode {
            CheckpointMode::RetainBase => 16 * 1024 * 1024 * 1024_u64,
            CheckpointMode::Incremental => 1024 * 1024 * 1024_u64,
        };
        let bytes = catalog
            .chunks
            .values()
            .try_fold(0_u64, |sum, chunk| {
                sum.checked_add(chunk.descriptor.size + af_crypto::TAG_LEN as u64)
            })
            .ok_or_else(|| error::config("checkpoint upload allowance overflow"))?;
        let plan = self
            .plane
            .task_checkpoint_plan(
                self.lease,
                &CheckpointPlanInput {
                    fencing_token: self.fence,
                    operation_id: activity.to_owned(),
                    task: task.clone(),
                    mode: self.mode,
                    base_id: self.base.and_then(|base| base.base_id.clone()),
                    commit_oid: match &self.task.target {
                        TaskTarget::ApiOnly => None,
                        TaskTarget::ExistingPr { head, .. } => Some(head.clone()),
                    },
                    format_version: CHECKPOINT_FORMAT,
                    toolbox_digest: self.toolbox.to_owned(),
                    page_count: u32::try_from(catalog.pages(None)?.len())
                        .map_err(|source| error::boundary("checkpoint count overflow", source))?,
                    upload_budget_bytes: bytes.min(allowance),
                    logical_bytes: catalog.logical_bytes,
                    entry_count: catalog.entry_count,
                },
            )
            .await?;
        if plan.mode != self.mode
            || plan.format_version != CHECKPOINT_FORMAT
            || plan.toolbox_digest != self.toolbox
        {
            return Err(error::config(
                "checkpoint plan differs from capture compatibility",
            ));
        }
        if plan.manifest_root.is_some() && self.mode == CheckpointMode::RetainBase {
            return Ok(plan);
        }
        let mut session = Session {
            plane: self.plane,
            lease: self.lease,
            workspace: self.workspace,
            fence: self.fence,
            task,
            plan,
            activity_id: activity.to_owned(),
            progress,
            transfers: Transfers::new()?,
            stop: self.stop,
        };
        let ids = session.capture_chunks(&catalog).await?;
        session
            .progress(
                Operation::Capture,
                Phase::Verifying,
                Some(
                    u32::try_from(catalog.chunks.len())
                        .map_err(|source| error::boundary("checkpoint count overflow", source))?,
                ),
            )
            .await;
        session.commit_catalog(&catalog, &ids, self.task).await
    }
}

impl Session<'_> {
    async fn capture_chunks(&mut self, catalog: &Catalog) -> Result<BTreeMap<String, String>> {
        let chunks: Vec<_> = catalog.chunks.iter().collect();
        let mut ids = BTreeMap::new();
        for batch in chunks.chunks(CHECKPOINT_BATCH) {
            let objects = self
                .register(
                    batch
                        .iter()
                        .map(|(_, chunk)| chunk.descriptor.clone())
                        .collect(),
                )
                .await?;
            for object in &objects {
                if !matches!(object.status, CheckpointObjectStatus::Busy) {
                    self.progress.discover(
                        &object.object_id,
                        object.plaintext.size + af_crypto::TAG_LEN as u64,
                        matches!(object.status, CheckpointObjectStatus::Reused),
                    );
                }
            }
            self.progress(
                Operation::Capture,
                Phase::Uploading,
                Some(
                    u32::try_from(chunks.len())
                        .map_err(|source| error::boundary("checkpoint count overflow", source))?,
                ),
            )
            .await;
            let mut pending = Vec::new();
            let mut pending_bytes = 0_u64;
            for ((key, chunk), object) in batch.iter().zip(objects) {
                ids.insert((*key).clone(), object.object_id.clone());
                if matches!(object.status, CheckpointObjectStatus::Busy) {
                    return Err(error::config("checkpoint chunk has another live writer"));
                }
                if !matches!(object.status, CheckpointObjectStatus::Reused) {
                    let bytes = object.plaintext.size + af_crypto::TAG_LEN as u64;
                    if !pending.is_empty() && pending_bytes + bytes > super::UPLOAD_BATCH_BYTES {
                        self.upload_batch(std::mem::take(&mut pending)).await?;
                        pending_bytes = 0;
                    }
                    let chunk = (**chunk).clone();
                    let plaintext = tokio::task::spawn_blocking(move || chunk.read()).await??;
                    pending.push((object, plaintext));
                    pending_bytes += bytes;
                }
            }
            if !pending.is_empty() {
                self.upload_batch(pending).await?;
            }
        }
        Ok(ids)
    }

    async fn commit_catalog(
        &mut self,
        catalog: &Catalog,
        ids: &BTreeMap<String, String>,
        task: &TaskContext<'_>,
    ) -> Result<CheckpointPlan> {
        let pages = catalog.pages(Some(ids))?;
        if pages.len() != self.plan.page_ids.len() {
            return Err(error::config(
                "checkpoint page layout changed after registration",
            ));
        }
        let checkpoint = Uuid7::parse(&self.plan.checkpoint_id)?;
        let mut digests = Vec::with_capacity(pages.len());
        for (page_id, contents) in self.plan.page_ids.clone().into_iter().zip(pages) {
            self.renew().await?;
            let digest =
                af_core::etag::sha256_hex(&serde_json::to_vec(&contents).map_err(error::encode)?);
            let accepted = self
                .plane
                .task_checkpoint_page_write(
                    self.lease,
                    &checkpoint,
                    &Uuid7::parse(&page_id)?,
                    &CheckpointPageInput {
                        fencing_token: self.fence,
                        task: self.task.clone(),
                        pin_id: self.plan.pin.id.clone(),
                        digest: digest.clone(),
                        contents,
                    },
                )
                .await?;
            if af_core::etag::sha256_hex(&serde_json::to_vec(&accepted).map_err(error::encode)?)
                != digest
            {
                return Err(error::config("checkpoint page acknowledgement differs"));
            }
            digests.push((page_id, digest));
        }
        self.renew().await?;
        let input = CheckpointCommitInput {
            fencing_token: self.fence,
            task: self.task.clone(),
            pin_id: self.plan.pin.id.clone(),
            manifest_root: af_crypto::checkpoint_digest::manifest_root(
                digests
                    .iter()
                    .map(|(id, digest)| (id.as_str(), digest.as_str())),
            ),
            history_digest: catalog.history_root(ids)?,
            covered_input_sequence: task.covered_input_sequence,
            effect_watermark: task.confirmed_effect_watermark,
        };
        let saved = self
            .plane
            .task_checkpoint_commit(self.lease, &checkpoint, &input)
            .await?;
        if saved.checkpoint_id != self.plan.checkpoint_id
            || saved.base_id != self.plan.base_id
            || saved.mode != self.plan.mode
            || saved.repository != self.plan.repository
            || saved.format_version != self.plan.format_version
            || saved.toolbox_digest != self.plan.toolbox_digest
            || saved.page_ids != self.plan.page_ids
            || saved.manifest_root.as_deref() != Some(input.manifest_root.as_str())
            || saved.history_digest.as_deref() != Some(input.history_digest.as_str())
            || saved.covered_input_sequence != input.covered_input_sequence
            || saved.effect_watermark != input.effect_watermark
        {
            return Err(error::config(
                "checkpoint commit acknowledgement differs from captured boundary",
            ));
        }
        Ok(saved)
    }
}
