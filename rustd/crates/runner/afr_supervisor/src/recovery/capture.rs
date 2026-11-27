//! Capture commits once every new object's PUT is accepted and every ordered
//! page is written.

use super::{Lineage, Session, catalog::Catalog, transfer::Transfers, tree::ChangeSource, uploads};
use crate::{Result, client::ControlPlane, error};
use af_core::id::Uuid7;
use af_wire::activity::{
    DurableTaskActivityOperation as Operation, DurableTaskActivityPhase as Phase,
};
use af_wire::durable_task::{
    CHECKPOINT_FORMAT, CheckpointCommitInput, CheckpointMode, CheckpointPageInput, CheckpointPlan,
    CheckpointPlanInput, DecisionDurableTaskVersion, DurableTaskContext, DurableTaskTarget,
};
use zeroize::Zeroizing;

#[derive(Debug)]
pub(crate) struct Capture<'a> {
    pub(crate) plane: &'a ControlPlane,
    pub(crate) transfers: &'a Transfers,
    pub(crate) run: &'a Uuid7,
    pub(crate) workspace: &'a Uuid7,
    pub(crate) fence: u64,
    pub(crate) task: &'a DurableTaskContext<'a>,
    pub(crate) toolbox: &'a str,
    pub(crate) mode: CheckpointMode,
    pub(crate) base: Option<&'a CheckpointPlan>,
    pub(crate) stop: &'a tokio_util::sync::CancellationToken,
    /// The tree the task's current checkpoint resolves to, when this run
    /// committed it: what a pin that read only what changed builds on.
    pub(crate) lineage: Option<&'a Lineage>,
}

impl Capture<'_> {
    /// Publishes what `source` yields beside `history`. Cancellation leaves
    /// an unfinished native plan, never a usable recovery pointer.
    pub(crate) async fn run(
        &self,
        source: Option<&dyn ChangeSource>,
        history: Zeroizing<Vec<u8>>,
        base_paths: Vec<String>,
    ) -> Result<CheckpointPlan> {
        let (plan, _lineage) = self.run_over(source, history, base_paths).await?;
        Ok(plan)
    }

    /// As [`Self::run`], with the tree the committed generation resolves to,
    /// for the next save to build on; none when it reused a base unchanged.
    pub(crate) async fn run_over(
        &self,
        source: Option<&dyn ChangeSource>,
        history: Zeroizing<Vec<u8>>,
        base_paths: Vec<String>,
    ) -> Result<(CheckpointPlan, Option<Lineage>)> {
        let activity = af_crypto::entropy::Entropy::new()
            .uuid7(af_core::clock::now())?
            .to_string();
        super::activity::emit(
            self.plane,
            self.run,
            &self.task.durable_task_id,
            &activity,
            Operation::Capture,
            Phase::Preparing,
            None,
        )
        .await;
        let mut progress = super::progress::Progress::default();
        let started = tokio::time::Instant::now();
        let saved = self
            .capture(&activity, &mut progress, source, history, base_paths)
            .await;
        uploads::logged("durable_task", started, &saved, |(plan, _)| plan.generation);
        let phase = if saved.is_ok() {
            Phase::Saved
        } else {
            Phase::Failed
        };
        super::activity::emit(
            self.plane,
            self.run,
            &self.task.durable_task_id,
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
        source: Option<&dyn ChangeSource>,
        history: Zeroizing<Vec<u8>>,
        base_paths: Vec<String>,
    ) -> Result<(CheckpointPlan, Option<Lineage>)> {
        if self.stop.is_cancelled() {
            return Err(error::config("checkpoint capture was cancelled"));
        }
        super::activity::emit(
            self.plane,
            self.run,
            &self.task.durable_task_id,
            activity,
            Operation::Capture,
            Phase::Scanning,
            None,
        )
        .await;
        let catalog = Catalog::read(
            source,
            history,
            (base_paths, self.lineage),
            af_wire::durable_task::CHECKPOINT_PAGE_BYTES,
            self.stop,
        )
        .await?;
        if self.stop.is_cancelled() {
            return Err(error::config("checkpoint capture was cancelled"));
        }
        let (task, plan) = self.plan(activity, &catalog).await?;
        if plan.mode != self.mode
            || plan.parent != catalog.parent.as_ref().map(ToString::to_string)
            || plan.format_version != CHECKPOINT_FORMAT
            || plan.toolbox_digest != self.toolbox
        {
            return Err(error::config(
                "checkpoint plan differs from capture compatibility",
            ));
        }
        if plan.manifest_root.is_some() && self.mode == CheckpointMode::RetainBase {
            return Ok((plan, None));
        }
        let mut session = Session {
            plane: self.plane,
            run: self.run,
            workspace: self.workspace,
            fence: self.fence,
            task,
            plan,
            activity_id: activity.to_owned(),
            progress,
            transfers: self.transfers.clone(),
            stop: self.stop,
        };
        let published = uploads::publish(&mut session, &catalog, source).await?;
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
        let saved = session
            .commit_catalog(&catalog, &published, self.task)
            .await?;
        let lineage = catalog.lineage(Uuid7::parse(&saved.checkpoint_id)?);
        Ok((saved, Some(lineage)))
    }

    async fn plan(
        &self,
        activity: &str,
        catalog: &Catalog,
    ) -> Result<(DecisionDurableTaskVersion, CheckpointPlan)> {
        let task = DecisionDurableTaskVersion {
            durable_task_id: self.task.durable_task_id.to_string(),
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
                sum.checked_add(chunk.descriptor.size + crate::byte_count(af_crypto::TAG_LEN))
            })
            .ok_or_else(|| error::config("checkpoint upload allowance overflow"))?;
        let plan = self
            .plane
            .durable_task_checkpoint_plan(
                self.run,
                &CheckpointPlanInput {
                    fencing_token: self.fence,
                    operation_id: activity.to_owned(),
                    durable_task: task.clone(),
                    mode: self.mode,
                    parent: catalog.parent.as_ref().map(ToString::to_string),
                    base_id: self.base.and_then(|base| base.base_id.clone()),
                    commit_oid: match &self.task.target {
                        DurableTaskTarget::ApiOnly => None,
                        DurableTaskTarget::ExistingPr { head, .. } => Some(head.clone()),
                    },
                    format_version: CHECKPOINT_FORMAT,
                    toolbox_digest: self.toolbox.to_owned(),
                    page_count: u32::try_from(
                        catalog
                            .pages(None, af_wire::durable_task::CHECKPOINT_PAGE_BYTES)?
                            .len(),
                    )
                    .map_err(|source| error::boundary("checkpoint count overflow", source))?,
                    upload_budget_bytes: bytes.min(allowance),
                    logical_bytes: catalog.logical_bytes,
                    entry_count: catalog.entry_count,
                },
            )
            .await?;
        Ok((task, plan))
    }
}

impl Session<'_> {
    async fn commit_catalog(
        &mut self,
        catalog: &Catalog,
        published: &uploads::Published,
        task: &DurableTaskContext<'_>,
    ) -> Result<CheckpointPlan> {
        let ids = &published.ids;
        let pages = catalog.pages(Some(ids), af_wire::durable_task::CHECKPOINT_PAGE_BYTES)?;
        if pages.len() != self.plan.page_ids.len() {
            return Err(error::config(
                "checkpoint page layout changed after registration",
            ));
        }
        let checkpoint = Uuid7::parse(&self.plan.checkpoint_id)?;
        let digests = self.write_pages(&checkpoint, pages).await?;
        self.renew().await?;
        let input = CheckpointCommitInput {
            fencing_token: self.fence,
            durable_task: self.task.clone(),
            pin_id: self.plan.pin.id.clone(),
            manifest_root: af_crypto::checkpoint_digest::manifest_root(
                digests
                    .iter()
                    .map(|(id, digest)| (id.as_str(), digest.as_str())),
            ),
            history_digest: catalog.history_root(ids)?,
            covered_input_sequence: task.covered_input_sequence,
            effect_watermark: task.confirmed_effect_watermark,
            uploaded: published.uploaded.clone(),
        };
        let saved = self
            .plane
            .durable_task_checkpoint_commit(self.run, &checkpoint, &input)
            .await?;
        if saved.checkpoint_id != self.plan.checkpoint_id
            || saved.parent != self.plan.parent
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

    async fn write_pages(
        &mut self,
        checkpoint: &Uuid7,
        pages: Vec<af_wire::durable_task::CheckpointPage>,
    ) -> Result<Vec<(String, String)>> {
        let mut digests = Vec::with_capacity(pages.len());
        for (page_id, contents) in self.plan.page_ids.clone().into_iter().zip(pages) {
            self.renew().await?;
            let digest =
                af_core::etag::sha256_hex(&serde_json::to_vec(&contents).map_err(error::encode)?);
            let accepted = self
                .plane
                .durable_task_checkpoint_page_write(
                    self.run,
                    checkpoint,
                    &Uuid7::parse(&page_id)?,
                    &CheckpointPageInput {
                        fencing_token: self.fence,
                        durable_task: self.task.clone(),
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
        Ok(digests)
    }
}
