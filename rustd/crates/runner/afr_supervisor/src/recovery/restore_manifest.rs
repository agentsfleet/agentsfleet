//! Authenticate complete base and overlay manifests before restoring any file.

use super::Session;
use crate::{Result, error};
use af_core::id::Uuid7;
use af_wire::task::{
    CHECKPOINT_PAGE_BYTES, CHECKPOINT_PAGES, CheckpointEntry, CheckpointReadQuery,
};
use std::collections::BTreeMap;

impl Session<'_> {
    pub(super) async fn manifest(
        &mut self,
    ) -> Result<(BTreeMap<String, CheckpointEntry>, Vec<String>, Vec<String>)> {
        let mut entries = BTreeMap::new();
        let mut bytes = 0_usize;
        let base = self.plan.base_page_ids.clone();
        if !base.is_empty() {
            let expected = self
                .plan
                .base_manifest_root
                .clone()
                .ok_or_else(|| error::config("checkpoint base root is absent"))?;
            let history = self
                .pages(&base, &expected, &mut entries, &mut bytes)
                .await?;
            if !history.is_empty() {
                return Err(error::config("retained base contains private history"));
            }
        }
        let base_paths = entries.keys().cloned().collect();
        let ids = self.plan.page_ids.clone();
        let expected = self
            .plan
            .manifest_root
            .clone()
            .ok_or_else(|| error::config("checkpoint manifest is incomplete"))?;
        let history = self
            .pages(&ids, &expected, &mut entries, &mut bytes)
            .await?;
        let total = entries
            .values()
            .try_fold(0_u64, |sum, entry| {
                sum.checked_add(match entry {
                    CheckpointEntry::File(file) => file.size,
                    CheckpointEntry::Directory { .. }
                    | CheckpointEntry::Symlink { .. }
                    | CheckpointEntry::Remove { .. } => 0,
                })
            })
            .ok_or_else(|| error::config("checkpoint expanded size overflow"))?;
        if total > 24 * 1024 * 1024 * 1024 || entries.len() > 200_000 {
            return Err(error::config("checkpoint expanded bounds exceeded"));
        }
        Ok((entries, history, base_paths))
    }

    pub(super) async fn pages(
        &mut self,
        ids: &[String],
        expected: &str,
        entries: &mut BTreeMap<String, CheckpointEntry>,
        bytes: &mut usize,
    ) -> Result<Vec<String>> {
        if ids.len() > usize::try_from(CHECKPOINT_PAGES).unwrap_or(usize::MAX) {
            return Err(error::config("checkpoint page count exceeds its bound"));
        }
        let checkpoint = Uuid7::parse(&self.plan.checkpoint_id)?;
        let mut digests = Vec::new();
        let mut history = Vec::new();
        let mut previous: Option<String> = None;
        for (position, id) in ids.iter().enumerate() {
            self.renew().await?;
            let page = self
                .plane
                .task_checkpoint_page(
                    self.run,
                    &checkpoint,
                    &Uuid7::parse(id)?,
                    &CheckpointReadQuery {
                        fencing_token: self.fence,
                        pin_id: self.plan.pin.id.clone(),
                        starting_after: None,
                    },
                )
                .await?;
            garde::Validate::validate(&page)?;
            let encoded = serde_json::to_vec(&page).map_err(error::encode)?;
            *bytes = bytes
                .checked_add(encoded.len())
                .ok_or_else(|| error::config("checkpoint manifest size overflow"))?;
            if usize::try_from(page.page_number).unwrap_or(usize::MAX) != position
                || encoded.len() > CHECKPOINT_PAGE_BYTES
                || *bytes > 64 * CHECKPOINT_PAGE_BYTES
            {
                return Err(error::config(
                    "checkpoint manifest exceeds its ordered bounds",
                ));
            }
            digests.push((id.as_str(), af_core::etag::sha256_hex(&encoded)));
            for entry in page.entries {
                if previous.as_deref().is_some_and(|path| path >= entry.path()) {
                    return Err(error::config("checkpoint manifest paths are not ordered"));
                }
                previous = Some(entry.path().to_owned());
                overlay(entries, entry)?;
            }
            history.extend(page.history_objects);
            if history.len() > 256 {
                return Err(error::config("checkpoint history object bound exceeded"));
            }
        }
        if af_crypto::checkpoint_digest::manifest_root(
            digests.iter().map(|(id, digest)| (*id, digest.as_str())),
        ) != expected
        {
            return Err(error::config("checkpoint manifest root mismatch"));
        }
        Ok(history)
    }
}

fn overlay(entries: &mut BTreeMap<String, CheckpointEntry>, entry: CheckpointEntry) -> Result<()> {
    if let CheckpointEntry::Remove { path } = entry {
        entries.remove(&path);
        let prefix = format!("{path}/");
        let removed: Vec<_> = entries
            .range(prefix.clone()..)
            .take_while(|(path, _)| path.starts_with(&prefix))
            .map(|(path, _)| path.clone())
            .collect();
        for path in removed {
            entries.remove(&path);
        }
    } else {
        entries.insert(entry.path().to_owned(), entry);
    }
    if entries.len() > 400_000 {
        return Err(error::config("checkpoint overlay exceeds entry bound"));
    }
    Ok(())
}
