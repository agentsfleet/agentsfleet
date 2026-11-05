//! Verify complete base and overlay roots before writing a fresh frozen workspace.

use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use afd_core::id::Uuid7;
use afd_crypto::secret::SecretBytes;
use afd_wire::task::*;
use afd_wire::activity::{TaskActivityOperation as Operation, TaskActivityPhase as Phase};
use zeroize::Zeroizing;
use crate::{Result, client::ControlPlane, error};
use super::{Object, Session, files::Workspace, transfer::Transfers};

#[derive(Debug)]
pub(super) struct Restore<'a> {
    pub(super) plane: &'a ControlPlane,
    pub(super) lease: &'a Uuid7,
    pub(super) workspace: &'a Uuid7,
    pub(super) fence: u64,
    pub(super) task: &'a TaskContext<'a>,
    pub(super) toolbox: &'a str,
    pub(super) stop: &'a tokio_util::sync::CancellationToken,
}

#[derive(Debug)]
pub(super) struct Restored {
    pub(super) plan: CheckpointPlan,
    pub(super) history: SecretBytes,
    pub(super) base_paths: Vec<String>,
}

impl Restore<'_> {
    /// Inspect the authenticated retained manifest, including on cross-task reuse.
    pub(super) async fn base_paths(&self, base: &CheckpointPlan) -> Result<Vec<String>> {
        let checkpoint = Uuid7::parse(&base.checkpoint_id)?;
        let task = DecisionTaskVersion { task_id: self.task.task_id.to_string(), generation: self.task.generation };
        let response = self.plane.task_checkpoint_authorizations(self.lease, &checkpoint, &CheckpointAuthorizationInput {
            fencing_token: self.fence, task: task.clone(), purpose: CheckpointTransferPurpose::Restore,
            pin_id: None, object_ids: Vec::new(),
        }).await?;
        let objects: Vec<Object> = response.objects.into_iter().map(Object::from).collect();
        let plan = response.plan;
        if !objects.is_empty() || plan.checkpoint_id != base.checkpoint_id || plan.base_id != base.base_id
            || plan.format_version != CHECKPOINT_FORMAT || plan.toolbox_digest != self.toolbox
            || plan.mode != CheckpointMode::RetainBase || plan.manifest_root != base.manifest_root {
            return Err(error::config("retained base inspection changed native identity"));
        }
        let mut session = Session { plane: self.plane, lease: self.lease, workspace: self.workspace, fence: self.fence,
            task, activity_id: plan.checkpoint_id.clone(), plan, transfers: Transfers::new()?, stop: self.stop };
        let ids = session.plan.page_ids.clone();
        let expected = session.plan.manifest_root.clone().ok_or_else(|| error::config("retained base is incomplete"))?;
        let mut entries = BTreeMap::new();
        let history = session.pages(&ids, &expected, &mut entries, &mut 0).await?;
        if !history.is_empty() || entries.len() > 200_000 { return Err(error::config("retained base manifest is incompatible")); }
        Ok(entries.into_keys().collect())
    }

    pub(super) async fn run(&self, destination: Option<(PathBuf, (u32, u32))>) -> Result<Restored> {
        let activity = self.task.checkpoint_id.as_deref().ok_or_else(|| error::config("task has no recovery checkpoint"))?;
        super::activity::emit(self.plane, self.lease, &self.task.task_id, activity, Operation::Restore, Phase::Restoring, 0, None).await;
        let restored = self.restore(destination).await;
        let phase = if restored.is_ok() { Phase::Restored } else { Phase::Failed };
        super::activity::emit(self.plane, self.lease, &self.task.task_id, activity, Operation::Restore, phase, 0, None).await;
        restored
    }

    async fn restore(&self, destination: Option<(PathBuf, (u32, u32))>) -> Result<Restored> {
        let checkpoint = Uuid7::parse(self.task.checkpoint_id.as_deref().ok_or_else(|| error::config("task has no recovery checkpoint"))?)?;
        let task = DecisionTaskVersion { task_id: self.task.task_id.to_string(), generation: self.task.generation };
        let response = self.plane.task_checkpoint_authorizations(self.lease, &checkpoint, &CheckpointAuthorizationInput {
            fencing_token: self.fence, task: task.clone(), purpose: CheckpointTransferPurpose::Restore,
            pin_id: None, object_ids: Vec::new(),
        }).await?;
        let objects: Vec<Object> = response.objects.into_iter().map(Object::from).collect();
        let plan = response.plan;
        if !objects.is_empty() || plan.checkpoint_id != checkpoint.as_str() || plan.format_version != CHECKPOINT_FORMAT
            || plan.toolbox_digest != self.toolbox || plan.mode != CheckpointMode::Incremental
            || plan.covered_input_sequence > self.task.covered_input_sequence
            || plan.effect_watermark > self.task.confirmed_effect_watermark {
            return Err(error::config("checkpoint bootstrap is incompatible with current task"));
        }
        let mut session = Session { plane: self.plane, lease: self.lease, workspace: self.workspace, fence: self.fence,
            task, activity_id: plan.checkpoint_id.clone(), plan, transfers: Transfers::new()?, stop: self.stop };
        session.progress(Operation::Restore, Phase::Verifying, 0, None).await;
        let (entries, history_ids, base_paths) = session.manifest().await?;
        session.restore_files(destination, entries).await?;
        let history = session.history(history_ids).await?;
        Ok(Restored { plan: session.plan, history, base_paths })
    }
}

impl Session<'_> {
    async fn manifest(&mut self) -> Result<(BTreeMap<String, CheckpointEntry>, Vec<String>, Vec<String>)> {
        let mut entries = BTreeMap::new();
        let mut bytes = 0_usize;
        let base = self.plan.base_page_ids.clone();
        if !base.is_empty() {
            let expected = self.plan.base_manifest_root.clone().ok_or_else(|| error::config("checkpoint base root is absent"))?;
            let history = self.pages(&base, &expected, &mut entries, &mut bytes).await?;
            if !history.is_empty() { return Err(error::config("retained base contains private history")); }
        }
        let base_paths = entries.keys().cloned().collect();
        let ids = self.plan.page_ids.clone();
        let expected = self.plan.manifest_root.clone().ok_or_else(|| error::config("checkpoint manifest is incomplete"))?;
        let history = self.pages(&ids, &expected, &mut entries, &mut bytes).await?;
        let total = entries.values().try_fold(0_u64, |sum, entry| sum.checked_add(match entry {
            CheckpointEntry::File(file) => file.size, _ => 0,
        })).ok_or_else(|| error::config("checkpoint expanded size overflow"))?;
        if total > 24 * 1024 * 1024 * 1024 || entries.len() > 200_000 { return Err(error::config("checkpoint expanded bounds exceeded")); }
        Ok((entries, history, base_paths))
    }

    async fn pages(&mut self, ids: &[String], expected: &str, entries: &mut BTreeMap<String, CheckpointEntry>,
        bytes: &mut usize) -> Result<Vec<String>> {
        if ids.len() > CHECKPOINT_PAGES as usize { return Err(error::config("checkpoint page count exceeds its bound")); }
        let checkpoint = Uuid7::parse(&self.plan.checkpoint_id)?;
        let mut digests = Vec::new();
        let mut history = Vec::new();
        let mut previous: Option<String> = None;
        for (position, id) in ids.iter().enumerate() {
            self.renew().await?;
            let page = self.plane.task_checkpoint_page(self.lease, &checkpoint, &Uuid7::parse(id)?, &CheckpointReadQuery {
                fencing_token: self.fence, pin_id: self.plan.pin.id.clone(), starting_after: None,
            }).await?;
            garde::Validate::validate(&page)?;
            let encoded = serde_json::to_vec(&page).map_err(error::encode)?;
            *bytes = bytes.checked_add(encoded.len()).ok_or_else(|| error::config("checkpoint manifest size overflow"))?;
            if page.page_number as usize != position || encoded.len() > CHECKPOINT_PAGE_BYTES || *bytes > 64 * CHECKPOINT_PAGE_BYTES {
                return Err(error::config("checkpoint manifest exceeds its ordered bounds"));
            }
            digests.push((id.as_str(), afd_core::etag::sha256_hex(&encoded)));
            for entry in page.entries {
                if previous.as_deref().is_some_and(|path| path >= entry.path()) { return Err(error::config("checkpoint manifest paths are not ordered")); }
                previous = Some(entry.path().to_owned());
                overlay(entries, entry)?;
            }
            history.extend(page.history_objects);
            if history.len() > 256 { return Err(error::config("checkpoint history object bound exceeded")); }
        }
        if afd_crypto::checkpoint_digest::manifest_root(digests.iter().map(|(id, digest)| (*id, digest.as_str()))) != expected {
            return Err(error::config("checkpoint manifest root mismatch"));
        }
        Ok(history)
    }

    async fn restore_files(&mut self, destination: Option<(PathBuf, (u32, u32))>, entries: BTreeMap<String, CheckpointEntry>) -> Result<()> {
        super::links::validate(entries.values().filter_map(|entry| match entry {
            CheckpointEntry::Symlink { path, target } => Some((path.as_str(), target.as_str())), _ => None,
        }))?;
        let Some((path, owner)) = destination else {
            return if entries.is_empty() { Ok(()) } else { Err(error::config("checkpoint files require a workspace")) };
        };
        let workspace = tokio::task::spawn_blocking(move || {
            let workspace = Workspace::open(&path)?;
            workspace.require_empty()?;
            Ok::<_, crate::Error>(Arc::new(workspace))
        }).await??;
        for entry in entries.values() {
            if let CheckpointEntry::Directory { path } = entry {
                let (workspace, path) = (Arc::clone(&workspace), path.clone());
                tokio::task::spawn_blocking(move || workspace.make_directory(&path, owner)).await??;
            }
        }
        for entry in entries.into_values() {
            self.renew().await?;
            match entry {
                CheckpointEntry::File(file) => {
                    let root = Arc::clone(&workspace);
                    let mut writer = tokio::task::spawn_blocking(move || root.make_file(&file.path, file.size, file.executable, owner)).await??;
                    for ids in file.objects.chunks(4) {
                        let chunks = self.download_batch(ids).await?;
                        writer = tokio::task::spawn_blocking(move || {
                            for bytes in chunks { writer.append(bytes.expose())?; }
                            Ok::<_, crate::Error>(writer)
                        }).await??;
                    }
                    tokio::task::spawn_blocking(move || writer.finish()).await??;
                }
                CheckpointEntry::Symlink { path, target } => {
                    let root = Arc::clone(&workspace);
                    tokio::task::spawn_blocking(move || root.make_link(&path, &target, owner)).await??;
                }
                CheckpointEntry::Directory { .. } => {},
                CheckpointEntry::Remove { .. } => return Err(error::config("checkpoint deletion survived overlay expansion")),
            }
        }
        Ok(())
    }

    async fn history(&mut self, ids: Vec<String>) -> Result<SecretBytes> {
        let mut history = Zeroizing::new(Vec::new());
        let mut digests = Vec::new();
        for batch in ids.chunks(4) {
            let chunks = self.download_batch(batch).await?;
            for (id, bytes) in batch.iter().zip(chunks) {
                if history.len().saturating_add(bytes.expose().len()) > 64 * 1024 * 1024 { return Err(error::config("checkpoint history expanded bound exceeded")); }
                digests.push((id.clone(), afd_core::etag::sha256_hex(bytes.expose()), bytes.expose().len() as u64));
                history.extend_from_slice(bytes.expose());
            }
        }
        let root = afd_crypto::checkpoint_digest::history_root(digests.iter().map(|(id, digest, size)| (id.as_str(), digest.as_str(), *size)));
        if self.plan.history_digest.as_deref() != Some(root.as_str()) { return Err(error::config("checkpoint history root mismatch")); }
        Ok(SecretBytes::new(std::mem::take(&mut *history)))
    }
}

fn overlay(entries: &mut BTreeMap<String, CheckpointEntry>, entry: CheckpointEntry) -> Result<()> {
    if let CheckpointEntry::Remove { path } = entry {
        entries.remove(&path);
        let prefix = format!("{path}/");
        let removed: Vec<_> = entries.range(prefix.clone()..).take_while(|(path, _)| path.starts_with(&prefix)).map(|(path, _)| path.clone()).collect();
        for path in removed { entries.remove(&path); }
    } else { entries.insert(entry.path().to_owned(), entry); }
    if entries.len() > 400_000 { return Err(error::config("checkpoint overlay exceeds entry bound")); }
    Ok(())
}
